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
    const [a, b] = await Promise.all([sessionManager.ensureProfile('flight-profile'), sessionManager.ensureProfile('flight-profile')]);
    assert.equal(launches, 1);
    assert.deepEqual(a, b);
  } finally {
    sessionManager.startProfile = original;
    sessionManager.startFlights.clear();
  }
});

function request(overrides = {}) {
  return { protocol: 'floword-production', protocolVersion: 1, requestId: 'req-unit', jobId: 'job-unit', stepId: 'image', attemptId: 'attempt-1', leaseId: 'lease-1', profileId: 'unit-session', method: 'grok.image.edit', params: {}, ...overrides };
}

test('ensureWorker failure does not poison the next dispatch', async () => {
  const session = { profileId: 'unit-session', worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  sessionManager.ensureWorker = async () => { throw new Error('CONTENT_SCRIPT_NOT_READY: fixture failure'); };
  await assert.rejects(sessionManager.dispatch(request()), /CONTENT_SCRIPT_NOT_READY/);
  assert.equal(session.activeRequest, null);
  assert.equal(sessionManager.activeRequests.size, 0);
  const worker = { evaluate: async (_fn, payload) => ({ protocol: 'floword-production', protocolVersion: 1, ok: true, requestId: payload.requestId, jobId: payload.jobId, stepId: payload.stepId, attemptId: payload.attemptId, leaseId: payload.leaseId, profileId: payload.profileId }) };
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
  const worker = { evaluate: async (_fn, payload) => { calls.push(payload.method); if (payload.method === 'production.task.cancel') return { ok: true, protocol: 'floword-production', protocolVersion: 1 }; return new Promise(() => {}); } };
  sessionManager.ensureWorker = async () => worker;
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
  }
});

test('active cancel waits for dispatch settlement and uses the authoritative entry', async () => {
  let rejectDispatch;
  const session = { profileId: 'success-cancel', worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => {
    if (payload.method === 'production.task.cancel') { rejectDispatch(new Error('cancelled')); return { ok: true }; }
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
