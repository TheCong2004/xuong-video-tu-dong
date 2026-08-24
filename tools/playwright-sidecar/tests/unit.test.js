const test = require('node:test');
const assert = require('node:assert/strict');
const sessionManager = require('../src/session-manager');

test('profile paths are isolated by UUID and reject traversal', () => {
  assert.throws(() => sessionManager.profileDir('../escape'), /INVALID_PROFILE/);
  const dir = sessionManager.profileDir('unit-profile');
  assert.match(dir, /unit-profile$/);
});

test('extension manifest is required before browser launch', () => {
  assert.throws(() => sessionManager.extensionDir('C:\\missing-floword-extension'), /EXTENSION_NOT_LOADED/);
});

test('concurrent ensureProfile calls share one launch flight', async () => {
  const original = sessionManager.startProfile;
  let launches = 0;
  sessionManager.startProfile = async (id) => { launches += 1; await new Promise((r) => setTimeout(r, 10)); return { profileId: id }; };
  try {
    const identity = { cdpEndpoint: 'http://127.0.0.1:9222', browserPid: 11, launchGeneration: 1 };
    const [a, b] = await Promise.all([sessionManager.ensureProfile('flight-profile', identity), sessionManager.ensureProfile('flight-profile', identity)]);
    assert.equal(launches, 1);
    assert.deepEqual(a, b);
  } finally {
    sessionManager.startProfile = original;
    sessionManager.startFlights.clear();
  }
});

test('CDP attach requires authoritative browser identity', async () => {
  await assert.rejects(
    sessionManager.startProfile('identity-profile', { cdpEndpoint: 'http://127.0.0.1:9222' }),
    /CDP_IDENTITY_REQUIRED/,
  );
});

test('different CDP identities do not share an attach flight', async () => {
  const original = sessionManager.startProfile;
  let launches = 0;
  sessionManager.startProfile = async (id, options) => { launches += 1; return { profileId: id, browserPid: options.browserPid, launchGeneration: options.launchGeneration }; };
  try {
    const first = sessionManager.ensureProfile('identity-flight', { cdpEndpoint: 'http://127.0.0.1:9222', browserPid: 11, launchGeneration: 1 });
    await assert.rejects(sessionManager.ensureProfile('identity-flight', { cdpEndpoint: 'http://127.0.0.1:9222', browserPid: 12, launchGeneration: 2 }), /CDP_SESSION_STALE/);
    await first;
    assert.equal(launches, 1);
  } finally {
    sessionManager.startProfile = original;
    sessionManager.startFlights.clear();
  }
});

test('CDP identity rejects non-loopback and non-canonical values', () => {
  assert.throws(() => sessionManager.validateAndNormalizeIdentity('identity', { cdpEndpoint: 'http://10.0.0.2:9222', browserPid: 1, launchGeneration: 1 }), /CDP_IDENTITY_REQUIRED/);
  assert.throws(() => sessionManager.validateAndNormalizeIdentity('identity', { cdpEndpoint: 'http://127.0.0.1:9222', browserPid: '1', launchGeneration: 1 }), /CDP_IDENTITY_REQUIRED/);
  assert.throws(() => sessionManager.validateAndNormalizeIdentity('identity', { cdpEndpoint: 'http://127.0.0.1:9222', browserPid: 1, launchGeneration: 0 }), /CDP_IDENTITY_REQUIRED/);
});

test('existing sessions cannot be reused without complete CDP identity', async () => {
  const session = { profileId: 'existing-identity', cdpEndpoint: 'http://127.0.0.1:9222', browserPid: 11, launchGeneration: 1, activeRequest: { requestId: 'active' }, browser: { close: async () => {} } };
  sessionManager.sessions.set(session.profileId, session);
  try {
    await assert.rejects(sessionManager.ensureProfile(session.profileId), /CDP_IDENTITY_REQUIRED/);
    await assert.rejects(sessionManager.ensureProfile(session.profileId, { cdpEndpoint: 'http://127.0.0.1:9222', browserPid: 11, launchGeneration: 2 }), /CDP_SESSION_STALE/);
  } finally { sessionManager.sessions.delete(session.profileId); }
});

function request(overrides = {}) {
  return { protocol: 'floword-production', protocolVersion: 1, requestId: 'req-unit', jobId: 'job-unit', stepId: 'image', attemptId: 'attempt-1', leaseId: 'lease-1', profileId: 'unit-session', method: 'grok.image.edit', params: {}, ...overrides };
}

function cancelAck(payload, overrides = {}) {
  return { protocol: 'floword-production', protocolVersion: 1, ok: true, requestId: payload.requestId, jobId: payload.jobId, stepId: payload.stepId, attemptId: payload.attemptId, leaseId: payload.leaseId, profileId: payload.profileId, result: { cancelled: true }, ...overrides };
}

test('ensureWorker failure does not poison the next dispatch', async () => {
  const session = { profileId: 'unit-session', worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  sessionManager.ensureWorker = async () => { throw new Error('CONTENT_SCRIPT_NOT_READY: fixture failure'); };
  await assert.rejects(sessionManager.dispatch(request()), /CONTENT_SCRIPT_NOT_READY/);
  assert.equal(session.activeRequest, null);
  assert.equal(sessionManager.activeRequests.size, 0);
  const worker = { evaluate: async (_fn, payload) => payload.method === 'production.task.cancel' ? cancelAck(payload) : ({ protocol: 'floword-production', protocolVersion: 1, ok: true, requestId: payload.requestId, jobId: payload.jobId, stepId: payload.stepId, attemptId: payload.attemptId, leaseId: payload.leaseId, profileId: payload.profileId }) };
  sessionManager.ensureWorker = async () => worker;
  try {
    const result = await sessionManager.dispatch(request());
    assert.equal(result.requestId, 'req-unit');
  } finally {
    sessionManager.ensureWorker = original;
    sessionManager.sessions.delete(session.profileId);
    sessionManager.activeRequests.clear();
    sessionManager.completedRequests.clear();
  }
});

test('dispatch timeout sends a cancel acknowledgement before releasing the profile', async () => {
  const calls = [];
  const session = { profileId: 'timeout-session', worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  const originalTimeout = sessionManager.timeoutForRequest;
  const worker = { evaluate: async (_fn, payload) => { calls.push(payload.method); if (payload.method === 'production.task.cancel') return cancelAck(payload); return new Promise(() => {}); } };
  sessionManager.ensureWorker = async () => worker;
  sessionManager.timeoutForRequest = () => 10;
  const originalSettle = sessionManager.cancelSettleTimeoutMs;
  sessionManager.cancelSettleTimeoutMs = 20;
  try {
    await assert.rejects(sessionManager.dispatch(request({ profileId: session.profileId, requestId: 'timeout-1', params: { timeoutMs: 1000 } })), /CANCEL_UNCONFIRMED/);
    assert.deepEqual(calls, ['grok.image.edit', 'production.task.cancel']);
    assert.notEqual(session.activeRequest, null);
    assert.equal(session.state, 'RECONCILING');
  } finally {
    sessionManager.ensureWorker = original;
    sessionManager.sessions.delete(session.profileId);
    sessionManager.activeRequests.clear();
    sessionManager.cancelSettleTimeoutMs = originalSettle;
    sessionManager.timeoutForRequest = originalTimeout;
  }
});

test('active cancel waits for dispatch settlement and uses the authoritative entry', async () => {
  let rejectDispatch;
  const session = { profileId: 'success-cancel', worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => {
    if (payload.method === 'production.task.cancel') { rejectDispatch(new Error('cancelled')); return cancelAck(payload); }
    return new Promise((_, reject) => { rejectDispatch = reject; });
  } });
  try {
    const dispatch = sessionManager.dispatch(request({ profileId: session.profileId, requestId: 'cancel-1', jobId: 'cancel-job' }));
    await new Promise((resolve) => setImmediate(resolve));
    const result = await sessionManager.cancel('cancel-job', 'cancel-1');
    assert.equal(result.cancelled, true);
    assert.equal(result.acknowledgment.ok, true);
    await assert.rejects(dispatch, /cancelled/);
    assert.equal(session.activeRequest, null);
  } finally {
    sessionManager.ensureWorker = original;
    sessionManager.sessions.delete(session.profileId);
    sessionManager.activeRequests.clear();
  }
});

test('health never upgrades BUSY or active sessions to READY', async () => {
  const session = { profileId: 'health-busy', worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: { requestId: 'busy-1' } };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  sessionManager.ensureWorker = async () => ({ evaluate: async () => ({ protocol: 'floword-production', protocolVersion: 1, ok: true, result: { profileId: session.profileId, status: 'READY', workerState: 'IDLE', loggedIn: true } }) });
  try { await sessionManager.health(session.profileId); assert.equal(session.state, 'BUSY'); }
  finally { sessionManager.ensureWorker = original; sessionManager.sessions.delete(session.profileId); }
});

test('late dispatch settlement is reaped without returning READY', async () => {
  let resolveDispatch;
  const session = { profileId: 'late-settle', worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
  sessionManager.sessions.set(session.profileId, session);
  const originalWorker = sessionManager.ensureWorker;
  const originalTimeout = sessionManager.timeoutForRequest;
  const originalSettle = sessionManager.cancelSettleTimeoutMs;
  sessionManager.timeoutForRequest = () => 10;
  sessionManager.cancelSettleTimeoutMs = 15;
  sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => payload.method === 'production.task.cancel' ? cancelAck(payload) : new Promise((resolve) => { resolveDispatch = resolve; }) });
  try {
    await assert.rejects(sessionManager.dispatch(request({ profileId: session.profileId, requestId: 'late-1' })), /CANCEL_UNCONFIRMED/);
    assert.equal(session.state, 'RECONCILING');
    assert.equal(sessionManager.activeRequests.has('late-1'), true);
    resolveDispatch({ protocol: 'floword-production', protocolVersion: 1, requestId: 'late-1', jobId: 'job-unit', stepId: 'image', attemptId: 'attempt-1', leaseId: 'lease-1', profileId: session.profileId });
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(session.activeRequest, null);
    assert.equal(sessionManager.activeRequests.has('late-1'), false);
    assert.equal(session.state, 'RECONCILING');
  } finally {
    sessionManager.ensureWorker = originalWorker;
    sessionManager.timeoutForRequest = originalTimeout;
    sessionManager.cancelSettleTimeoutMs = originalSettle;
    sessionManager.sessions.delete(session.profileId);
    sessionManager.activeRequests.clear();
  }
});

test('cancel acknowledgement with result.cancelled=false is not success', async () => {
  const session = { profileId: 'cancel-false', worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  let rejectDispatch;
  sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => payload.method === 'production.task.cancel' ? { ok: true, result: { cancelled: false } } : new Promise((_, reject) => { rejectDispatch = reject; }) });
  try {
    const dispatch = sessionManager.dispatch(request({ profileId: session.profileId, requestId: 'false-cancel' }));
    await new Promise((resolve) => setImmediate(resolve));
    const result = await sessionManager.cancel('job-unit', 'false-cancel');
    assert.equal(result.cancelled, false);
    assert.equal(result.acknowledgment.code, 'CANCEL_UNCONFIRMED');
    rejectDispatch(new Error('cancel failed'));
    await assert.rejects(dispatch);
  } finally {
    sessionManager.ensureWorker = original;
    sessionManager.sessions.delete(session.profileId);
    sessionManager.activeRequests.clear();
  }
});

test('cancel acknowledgement requires a cancelled result object', async () => {
  for (const response of [{ ok: true }, { ok: true, result: {} }, { ok: true, result: { cancelled: false } }]) {
    const session = { profileId: `missing-result-${Math.random()}`, worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
    sessionManager.sessions.set(session.profileId, session);
    const original = sessionManager.ensureWorker;
    let rejectDispatch;
    sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => payload.method === 'production.task.cancel' ? response : new Promise((_, reject) => { rejectDispatch = reject; }) });
    try {
      const dispatch = sessionManager.dispatch(request({ profileId: session.profileId, requestId: `missing-${Math.random()}` }));
      await new Promise((resolve) => setImmediate(resolve));
      const result = await sessionManager.cancel('job-unit', session.activeRequest.requestId);
      assert.equal(result.cancelled, false);
      assert.equal(result.acknowledgment.code, 'CANCEL_UNCONFIRMED');
      rejectDispatch(new Error('cancelled'));
      await assert.rejects(dispatch);
    } finally {
      sessionManager.ensureWorker = original;
      sessionManager.sessions.delete(session.profileId);
      sessionManager.activeRequests.clear();
    }
  }
});

test('cancel acknowledgement requires protocol and complete correlation', async () => {
  const cases = [
    ['missing protocol', (ack) => { const { protocol, ...rest } = ack; return rest; }],
    ['wrong protocol version', (ack) => ({ ...ack, protocolVersion: 2 })],
    ['wrong cancel request id', (ack) => ({ ...ack, requestId: 'CANCEL_wrong' })],
    ['wrong job id', (ack) => ({ ...ack, jobId: 'job-other' })],
    ['wrong lease id', (ack) => ({ ...ack, leaseId: 'lease-other' })],
    ['wrong profile id', (ack) => ({ ...ack, profileId: 'profile-other' })],
  ];
  for (const [label, mutate] of cases) {
    const profileId = `cancel-contract-${label.replaceAll(' ', '-')}`;
    const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
    sessionManager.sessions.set(profileId, session);
    const original = sessionManager.ensureWorker;
    let rejectDispatch;
    sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => payload.method === 'production.task.cancel' ? mutate(cancelAck(payload)) : new Promise((_, reject) => { rejectDispatch = reject; }) });
    try {
      const dispatch = sessionManager.dispatch(request({ profileId, requestId: `contract-${label.replaceAll(' ', '-')}` }));
      await new Promise((resolve) => setImmediate(resolve));
      const result = await sessionManager.cancel('job-unit', session.activeRequest.requestId);
      assert.equal(result.cancelled, false, label);
      assert.match(result.acknowledgment.code, /CANCEL_UNCONFIRMED|CORRELATION_MISMATCH/, label);
      assert.equal(session.state, 'RECONCILING', label);
      rejectDispatch(new Error('cancelled'));
      await assert.rejects(dispatch);
    } finally {
      sessionManager.ensureWorker = original;
      sessionManager.sessions.delete(profileId);
      sessionManager.activeRequests.clear();
    }
  }
});

test('complete cancel acknowledgement is accepted', async () => {
  const profileId = 'cancel-contract-valid';
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
  sessionManager.sessions.set(profileId, session);
  const original = sessionManager.ensureWorker;
  let rejectDispatch;
  sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => {
    if (payload.method === 'production.task.cancel') { rejectDispatch(new Error('cancelled')); return cancelAck(payload); }
    return new Promise((_, reject) => { rejectDispatch = reject; });
  } });
  try {
    const dispatch = sessionManager.dispatch(request({ profileId, requestId: 'contract-valid' }));
    await new Promise((resolve) => setImmediate(resolve));
    const result = await sessionManager.cancel('job-unit', 'contract-valid');
    assert.equal(result.cancelled, true);
    rejectDispatch(new Error('cancelled'));
    await assert.rejects(dispatch);
  } finally {
    sessionManager.ensureWorker = original;
    sessionManager.sessions.delete(profileId);
    sessionManager.activeRequests.clear();
  }
});

test('cancel is fail-closed for missing or mismatched active requests', async () => {
  const missing = await sessionManager.cancel('missing-job', 'req-missing');
  assert.equal(missing.cancelled, false);
  assert.equal(missing.acknowledgment.ok, false);
  const session = { profileId: 'cancel-session', activeRequest: { jobId: 'job-1', requestId: 'req-1' } };
  sessionManager.sessions.set(session.profileId, session);
  try {
    const mismatch = await sessionManager.cancel('job-1', 'req-other');
    assert.equal(mismatch.cancelled, false);
    assert.equal(mismatch.acknowledgment.code, 'CORRELATION_MISMATCH');
  } finally {
    sessionManager.sessions.delete(session.profileId);
  }
});
