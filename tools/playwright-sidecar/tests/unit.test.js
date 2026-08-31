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

test('production bootstrap worker is observed without page messaging', async () => {
  const worker = { url: () => 'chrome-extension://floword/background.js', evaluate: async () => true };
  const session = { context: { serviceWorkers: () => [worker] }, browser: { newBrowserCDPSession: async () => ({ send: async () => {}, detach: async () => {} }) } };
  const page = {
    url: () => 'https://grok.com/imagine',
    isClosed: () => false,
    waitForLoadState: async () => {},
    evaluate: async () => JSON.stringify({ state: 'ACKNOWLEDGED', wakeId: 'test', protocolVersion: 1, error: null }),
  };
  const result = await sessionManager.wakeServiceWorker(page, 2000, session);
  assert.equal(result.ok, true);
  assert.equal(result.protocolVersion, 1);
  assert.equal(result.delivery, 'bootstrap-observed');
});

test('missing bootstrapped Floword service worker is fail-closed', async () => {
  const page = { url: () => 'https://grok.com/imagine', isClosed: () => false, waitForLoadState: async () => {} };
  await assert.rejects(sessionManager.wakeServiceWorker(page, 1000, { context: { serviceWorkers: () => [] } }), /GROK_PRODUCTION_BOOTSTRAP_NOT_INJECTED/);
});

test('bootstrap observation only reads the marker and never sends a page message', async () => {
  let evaluates = 0;
  const page = { evaluate: async () => { evaluates += 1; return JSON.stringify({ state: 'ACKNOWLEDGED', wakeId: 'test', protocolVersion: 1, error: null }); } };
  const worker = { url: () => 'chrome-extension://floword/background.js', evaluate: async () => true };
  const result = await sessionManager.wakeServiceWorker(page, 2000, { context: { serviceWorkers: () => [worker] }, browser: { newBrowserCDPSession: async () => ({ send: async () => {}, detach: async () => {} }) } });
  assert.equal(result.ok, true);
  assert.ok(evaluates > 0);
});

test('wake remains fail-closed when no worker is available', async () => {
  const page = { isClosed: () => true };
  await assert.rejects(sessionManager.wakeServiceWorker(page, 1000, { context: { serviceWorkers: () => [] } }), /GROK_PRODUCTION_BOOTSTRAP_NOT_INJECTED/);
});

test('wake times out when bootstrap worker never appears', async () => {
  const page = {};
  await assert.rejects(sessionManager.wakeServiceWorker(page, 1000, { context: { serviceWorkers: () => [] } }), /GROK_PRODUCTION_BOOTSTRAP_NOT_INJECTED/);
});

test('worker without Floword bind and health is not accepted as wake success', async () => {
  const page = { url: () => 'https://grok.com/imagine', isClosed: () => false, waitForLoadState: async () => {}, evaluate: async () => JSON.stringify({ state: 'ACKNOWLEDGED', wakeId: 'test', protocolVersion: 1, error: null }) };
  const worker = { url: () => 'chrome-extension://floword/background.js', evaluate: async () => false };
  await assert.rejects(sessionManager.wakeServiceWorker(page, 1000, { context: { serviceWorkers: () => [worker] } }), /EXTENSION_PRODUCTION_CONTRACT_MISSING/);
});

test('wake does not change the authoritative Donut CDP identity', async () => {
  const identity = { browserPid: 33952, cdpEndpoint: 'http://127.0.0.1:56741', launchGeneration: 1787632881 };
  const before = { ...identity };
  const page = {
    url: () => 'https://grok.com/imagine',
    isClosed: () => false,
    waitForLoadState: async () => {},
    evaluate: async () => ({ ok: true, protocol: 'floword-production', protocolVersion: 1 }),
  };
  await sessionManager.wakeServiceWorker(page, 1000, { context: { serviceWorkers: () => [{ url: () => 'chrome-extension://floword/background.js', evaluate: async () => true }] } });
  assert.deepEqual(identity, before);
});

test('Grok attach reuses the existing tab and never creates a duplicate', async () => {
  let broughtToFront = 0;
  const page = { url: () => 'https://grok.com/imagine', isClosed: () => false, bringToFront: async () => { broughtToFront += 1; }, waitForLoadState: async () => {} };
  const session = { context: { pages: () => [page] }, grokPage: null, managedGrokTabId: null };
  const attached = await sessionManager.ensureGrokPage(session);
  assert.equal(attached, page);
  assert.equal(broughtToFront, 1);
  assert.equal(session.context.pages().length, 1);
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

test('post-submit correlation errors preserve root code and retain ownership', async () => {
  const profileId = 'post-submit-root-error';
  const requestId = 'post-submit-root-error-1';
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: false };
  sessionManager.sessions.set(profileId, session);
  const originals = { ensureContent: sessionManager.ensureContentContract, cdp: sessionManager.cdpEvaluate, quarantine: sessionManager.reconciliationQuarantineMs };
  sessionManager.ensureContentContract = async () => {};
  sessionManager.reconciliationQuarantineMs = 20;
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('.dispatch(')) return {
      protocol: 'floword-production', protocolVersion: 1,
      requestId, jobId: 'job-unit', stepId: 'image', attemptId: 'attempt-1', leaseId: 'lease-1', profileId,
      ok: false, error: { code: 'POST_CORRELATION_CONFLICT', message: 'post root changed', retryable: false, details: { submissionState: 'SUBMITTED' } },
    };
    return null;
  };
  const req = request({ profileId, requestId, jobId: 'job-unit' });
  try {
    let error;
    try { await sessionManager.dispatch(req); } catch (caught) { error = caught; }
    assert.ok(error);
    assert.match(error.message, /^POST_CORRELATION_CONFLICT:/);
    assert.equal(error.details.submissionState, 'SUBMITTED');
    assert.equal(error.details.resolutionState, 'RECONCILING');
    assert.equal(error.details.retryable, false);
    assert.equal(error.details.ownershipHeld, true);
    assert.equal(session.activeRequest?.requestId, requestId);
    assert.equal(sessionManager.requestJournals.get(requestId).some((entry) => entry.stage === 'DISPATCH_RECONCILING' && entry.errorCode === 'POST_CORRELATION_CONFLICT'), true);
    await new Promise((resolve) => setTimeout(resolve, 40));
    const orphan = sessionManager.requestJournals.get(requestId).find((entry) => entry.stage === 'DISPATCH_ORPHANED');
    assert.equal(orphan.reason, 'RESULT_RECONCILE_EXHAUSTED');
  } finally {
    sessionManager.ensureContentContract = originals.ensureContent;
    sessionManager.cdpEvaluate = originals.cdp;
    sessionManager.reconciliationQuarantineMs = originals.quarantine;
    sessionManager.sessions.delete(profileId);
    sessionManager.activeRequests.delete(requestId);
    sessionManager.terminalRequests.delete(requestId);
    sessionManager.requestJournals.delete(requestId);
    sessionManager.requestStateCache.delete(requestId);
  }
});

test('request journal records success and duplicate request does not dispatch twice', async () => {
  const profileId = 'journal-success';
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(profileId, session);
  const original = sessionManager.ensureWorker;
  const originalEnsureContent = sessionManager.ensureContentContract;
  const originalCdp = sessionManager.cdpEvaluate;
  let dispatchCalls = 0;
  sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => {
    if (payload.method === 'production.task.cancel') return cancelAck(payload);
    dispatchCalls += 1;
    return { protocol: 'floword-production', protocolVersion: 1, requestId: payload.requestId, jobId: payload.jobId, stepId: payload.stepId, attemptId: payload.attemptId, leaseId: payload.leaseId, profileId: payload.profileId, ok: true, result: { mediaType: 'image', locator: 'artifact://ok' } };
  } });
  sessionManager.ensureContentContract = async () => {};
  sessionManager.cdpEvaluate = async (_session, expression) => { if (expression.includes('.dispatch(')) dispatchCalls += 1; return { protocol: 'floword-production', protocolVersion: 1, requestId: req.requestId, jobId: req.jobId, stepId: req.stepId, attemptId: req.attemptId, leaseId: req.leaseId, profileId: req.profileId, ok: true, result: { mediaType: 'image', locator: 'artifact://ok' } }; };
  const req = request({ profileId, requestId: 'journal-success-request' });
  try {
    const first = await sessionManager.dispatch(req);
    const second = await sessionManager.dispatch(req);
    assert.deepEqual(second, first);
    assert.equal(dispatchCalls, 1);
    const stages = sessionManager.requestJournals.get(req.requestId).map((entry) => entry.stage);
    assert.deepEqual(stages, ['DISPATCH_RECEIVED', 'DISPATCH_COMPLETED', 'DISPATCH_RECEIVED', 'DISPATCH_DUPLICATE']);
    assert.ok(!JSON.stringify(sessionManager.requestJournals.get(req.requestId)).includes('base64'));
  } finally { sessionManager.ensureWorker = original; sessionManager.ensureContentContract = originalEnsureContent; sessionManager.cdpEvaluate = originalCdp; sessionManager.sessions.delete(profileId); sessionManager.requestJournals.delete(req.requestId); sessionManager.activeRequests.clear(); sessionManager.completedRequests.clear(); }
});

test('request journal records dispatch failure without secrets', async () => {
  const profileId = 'journal-error';
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(profileId, session);
  const original = sessionManager.ensureContentContract;
  sessionManager.ensureContentContract = async () => { throw new Error('CONTRACT_FAILED: secret-token'); };
  const req = request({ profileId, requestId: 'journal-error-request', params: { prompt: 'private prompt', dataUrl: 'data:image/png;base64,SECRET' } });
  try {
    await assert.rejects(sessionManager.dispatch(req), /CONTRACT_FAILED/);
    const journal = JSON.stringify(sessionManager.requestJournals.get(req.requestId));
    assert.match(journal, /DISPATCH_FAILED/);
    assert.doesNotMatch(journal, /private prompt|SECRET|base64|secret-token/);
  } finally { sessionManager.ensureContentContract = original; sessionManager.sessions.delete(profileId); sessionManager.requestJournals.delete(req.requestId); }
});

test('sidecar imports inner executor journal state and preserves correlation counters', () => {
  const req = request({ profileId: 'journal-inner', requestId: 'journal-inner-request', jobId: 'journal-inner-job' });
  sessionManager.requestJournals.delete(req.requestId);
  const state = {
    requestId: req.requestId,
    submissionState: 'SUBMITTED',
    stage: 'RESULT_SELECTED',
    entries: [
      { stage: 'BASELINE_CAPTURED', timestamp: '2026-01-01T00:00:00.000Z', submissionState: 'NOT_SUBMITTED', baselineCount: 2 },
      { stage: 'RESULT_SCAN', timestamp: '2026-01-01T00:00:01.000Z', submissionState: 'SUBMITTED', baselineCount: 2, newCandidateCount: 1, newContainerCount: 1 },
      { stage: 'RESULT_SELECTED', timestamp: '2026-01-01T00:00:02.000Z', submissionState: 'SUBMITTED', baselineCount: 2, newCandidateCount: 1, newContainerCount: 1, selectedFingerprint: 'fp-safe' },
    ],
  };
  try {
    sessionManager.appendInnerJournal(req, state);
    const entries = sessionManager.requestJournals.get(req.requestId);
    assert.deepEqual(entries.map((entry) => entry.stage), ['BASELINE_CAPTURED', 'RESULT_SCAN', 'RESULT_SELECTED']);
    assert.equal(entries[1].baselineCount, 2);
    assert.equal(entries[1].newCandidateCount, 1);
    assert.equal(entries[1].newContainerCount, 1);
    assert.equal(entries[2].selectedFingerprint, 'fp-safe');
    assert.equal(entries.every((entry) => entry.requestId === req.requestId && entry.jobId === req.jobId), true);
  } finally {
    sessionManager.requestJournals.delete(req.requestId);
  }
});

test('ensureWorker failure does not poison the next dispatch', async () => {
  const session = { profileId: 'unit-session', worker: {}, context: { serviceWorkers: () => [] }, state: 'READY', activeRequest: null };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  const originalEnsureContent = sessionManager.ensureContentContract;
  const originalCdp = sessionManager.cdpEvaluate;
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
  const session = { profileId: 'timeout-session', worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  const originalEnsureContent = sessionManager.ensureContentContract;
  const originalCdp = sessionManager.cdpEvaluate;
  const originalTimeout = sessionManager.timeoutForRequest;
  const worker = { evaluate: async (_fn, payload) => { calls.push(payload.method); if (payload.method === 'production.task.cancel') return cancelAck(payload); return new Promise(() => {}); } };
  sessionManager.ensureWorker = async () => worker;
  sessionManager.ensureContentContract = async () => {};
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('.dispatch(')) { calls.push('grok.image.edit'); return new Promise(() => {}); }
    if (expression.includes('requestState(')) return { submissionState: 'NOT_SUBMITTED', stage: 'PREPARING', entries: [] };
    if (expression.includes('.cancel(')) {
      calls.push('production.task.cancel');
      const payload = JSON.parse(expression.slice(expression.indexOf('.cancel(') + 8, -1));
      return cancelAck(payload);
    }
    return null;
  };
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
    sessionManager.ensureContentContract = originalEnsureContent;
    sessionManager.cdpEvaluate = originalCdp;
    sessionManager.sessions.delete(session.profileId);
    sessionManager.activeRequests.clear();
    sessionManager.requestStateCache.clear();
    sessionManager.cancelSettleTimeoutMs = originalSettle;
    sessionManager.timeoutForRequest = originalTimeout;
  }
});

test('invalid timeout cancellation acknowledgement retains ownership for orphan reaping', async () => {
  const profileId = 'timeout-invalid-cancel';
  const requestId = 'timeout-invalid-cancel-1';
  const calls = [];
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(profileId, session);
  const originals = { ensureContent: sessionManager.ensureContentContract, cdp: sessionManager.cdpEvaluate, timeout: sessionManager.timeoutForRequest, quarantine: sessionManager.reconciliationQuarantineMs };
  sessionManager.ensureContentContract = async () => {};
  sessionManager.timeoutForRequest = () => 10;
  sessionManager.reconciliationQuarantineMs = 20;
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('.dispatch(')) { calls.push('dispatch'); return new Promise(() => {}); }
    if (expression.includes('requestState(')) return { submissionState: 'NOT_SUBMITTED', stage: 'PREPARING', entries: [] };
    if (expression.includes('.cancel(')) { calls.push('cancel'); return { ok: true }; }
    return null;
  };
  const req = request({ profileId, requestId });
  try {
    await assert.rejects(sessionManager.dispatch(req), /CANCEL_UNCONFIRMED/);
    assert.deepEqual(calls, ['dispatch', 'cancel']);
    assert.equal(session.activeRequest?.requestId, requestId);
    assert.equal(sessionManager.activeRequests.has(requestId), true);
    await new Promise((resolve) => setTimeout(resolve, 50));
    const orphan = sessionManager.requestJournals.get(requestId).find((entry) => entry.stage === 'DISPATCH_ORPHANED');
    assert.ok(orphan);
    assert.equal(orphan.resolutionState, 'ORPHANED');
    assert.equal(session.activeRequest, null);
  } finally {
    sessionManager.ensureContentContract = originals.ensureContent;
    sessionManager.cdpEvaluate = originals.cdp;
    sessionManager.timeoutForRequest = originals.timeout;
    sessionManager.reconciliationQuarantineMs = originals.quarantine;
    sessionManager.sessions.delete(profileId);
    sessionManager.activeRequests.delete(requestId);
    sessionManager.terminalRequests.delete(requestId);
    sessionManager.requestJournals.delete(requestId);
    sessionManager.requestStateCache.delete(requestId);
  }
});

test('submitted timeout enters reconcile without cancellation', async () => {
  const calls = [];
  const profileId = 'submitted-timeout';
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(profileId, session);
  const originals = { ensureWorker: sessionManager.ensureWorker, ensureContent: sessionManager.ensureContentContract, cdp: sessionManager.cdpEvaluate, timeout: sessionManager.timeoutForRequest };
  sessionManager.ensureWorker = async () => ({});
  sessionManager.ensureContentContract = async () => {};
  sessionManager.timeoutForRequest = () => 10;
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('requestState(')) return { submissionState: 'SUBMITTED', stage: 'RESULT_SCAN', entries: [{ stage: 'RESULT_SCAN', submissionState: 'SUBMITTED', baselineCount: 1, newCandidateCount: 0, newContainerCount: 0 }] };
    if (expression.includes('.dispatch(')) { calls.push('dispatch'); return new Promise(() => {}); }
    if (expression.includes('.cancel(')) { calls.push('cancel'); return null; }
    return null;
  };
  try {
    await assert.rejects(sessionManager.dispatch(request({ profileId, requestId: 'submitted-timeout-1' })), /DISPATCH_RECONCILING/);
    assert.deepEqual(calls, ['dispatch']);
    assert.notEqual(session.activeRequest, null);
    assert.equal(session.state, 'RECONCILING');
    assert.ok(sessionManager.requestJournals.get('submitted-timeout-1').some((entry) => entry.stage === 'DISPATCH_RECONCILING'));
  } finally {
    sessionManager.ensureWorker = originals.ensureWorker;
    sessionManager.ensureContentContract = originals.ensureContent;
    sessionManager.cdpEvaluate = originals.cdp;
    sessionManager.timeoutForRequest = originals.timeout;
    sessionManager.sessions.delete(profileId);
    sessionManager.activeRequests.clear();
    sessionManager.requestJournals.delete('submitted-timeout-1');
  }
});

test('unknown state after submit intent is reconciled without cancellation', async () => {
  const profileId = 'unknown-after-submit';
  const requestId = 'unknown-after-submit-1';
  const calls = [];
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(profileId, session);
  const originals = {
    ensureContent: sessionManager.ensureContentContract,
    cdp: sessionManager.cdpEvaluate,
    timeout: sessionManager.timeoutForRequest,
    quarantine: sessionManager.reconciliationQuarantineMs,
  };
  sessionManager.ensureContentContract = async () => {};
  sessionManager.timeoutForRequest = () => 10;
  sessionManager.reconciliationQuarantineMs = 1000;
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('requestState(')) return {
      submissionState: 'UNKNOWN',
      stage: 'RESULT_SCAN',
      summary: { submissionState: 'UNKNOWN', sideEffectPossible: false, submitIntentObserved: false, submitClicked: false, submitAcknowledged: false, postCreated: false, generationAccepted: false, resultScanCount: 2 },
      entries: [{ stage: 'RESULT_SCAN', submissionState: 'UNKNOWN', baselineCount: 2, newCandidateCount: 0, newContainerCount: 0 }],
    };
    if (expression.includes('.dispatch(')) { calls.push('dispatch'); return new Promise(() => {}); }
    if (expression.includes('.cancel(')) { calls.push('cancel'); return null; }
    return null;
  };
  const req = request({ profileId, requestId, jobId: 'unknown-after-submit-job' });
  try {
    await assert.rejects(sessionManager.dispatch(req), /DISPATCH_RECONCILING/);
    assert.deepEqual(calls, ['dispatch']);
    assert.equal(session.state, 'RECONCILING');
    assert.equal(session.activeRequest?.requestId, requestId);
    const stages = sessionManager.requestJournals.get(requestId).map((entry) => entry.stage);
    assert.ok(stages.includes('DISPATCH_RECONCILING'));
    assert.equal(stages.includes('DISPATCH_FAILED'), false);
    assert.equal(stages.includes('CANCEL_ACKNOWLEDGED'), false);
    const reconcile = sessionManager.requestJournals.get(requestId).find((entry) => entry.stage === 'DISPATCH_RECONCILING');
    assert.equal(reconcile.reason, 'RESULT_TIMEOUT_AFTER_POSSIBLE_SUBMIT');
    assert.equal(reconcile.retryable, false);
  } finally {
    sessionManager.ensureContentContract = originals.ensureContent;
    sessionManager.cdpEvaluate = originals.cdp;
    sessionManager.timeoutForRequest = originals.timeout;
    sessionManager.reconciliationQuarantineMs = originals.quarantine;
    sessionManager.sessions.delete(profileId);
    sessionManager.activeRequests.delete(requestId);
    sessionManager.requestJournals.delete(requestId);
  }
});

test('requestState null is treated as unavailable and never cancelled', async () => {
  const profileId = 'state-unavailable';
  const requestId = 'state-unavailable-1';
  const calls = [];
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(profileId, session);
  const originals = { ensureContent: sessionManager.ensureContentContract, cdp: sessionManager.cdpEvaluate, timeout: sessionManager.timeoutForRequest, quarantine: sessionManager.reconciliationQuarantineMs };
  sessionManager.ensureContentContract = async () => {};
  sessionManager.timeoutForRequest = () => 10;
  sessionManager.reconciliationQuarantineMs = 20;
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('.dispatch(')) { calls.push('dispatch'); return new Promise(() => {}); }
    if (expression.includes('requestState(')) return null;
    if (expression.includes('.cancel(')) { calls.push('cancel'); return null; }
    return null;
  };
  const req = request({ profileId, requestId });
  try {
    await assert.rejects(sessionManager.dispatch(req), /DISPATCH_RECONCILING/);
    assert.deepEqual(calls, ['dispatch']);
    assert.equal(session.state, 'RECONCILING');
    await new Promise((resolve) => setTimeout(resolve, 50));
    const orphan = sessionManager.requestJournals.get(requestId).find((entry) => entry.stage === 'DISPATCH_ORPHANED');
    assert.equal(orphan.submissionState, 'UNKNOWN');
    assert.equal(orphan.resolutionState, 'ORPHANED');
  } finally {
    sessionManager.ensureContentContract = originals.ensureContent;
    sessionManager.cdpEvaluate = originals.cdp;
    sessionManager.timeoutForRequest = originals.timeout;
    sessionManager.reconciliationQuarantineMs = originals.quarantine;
    sessionManager.sessions.delete(profileId);
    sessionManager.activeRequests.delete(requestId);
    sessionManager.terminalRequests.delete(requestId);
    sessionManager.requestJournals.delete(requestId);
    sessionManager.requestStateCache.delete(requestId);
  }
});

test('cached POST_CREATED_LATE evidence survives null requestState without cancel', async () => {
  const profileId = 'late-post-cache';
  const requestId = 'late-post-cache-1';
  const calls = [];
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(profileId, session);
  sessionManager.requestStateCache.set(requestId, { submissionState: 'SUBMITTED', summary: { postProofObserved: true }, stage: 'POST_CREATED_LATE', entries: [{ stage: 'POST_CREATED_LATE', submissionState: 'SUBMITTED' }] });
  const originals = { ensureContent: sessionManager.ensureContentContract, cdp: sessionManager.cdpEvaluate, timeout: sessionManager.timeoutForRequest, quarantine: sessionManager.reconciliationQuarantineMs };
  sessionManager.ensureContentContract = async () => {};
  sessionManager.timeoutForRequest = () => 10;
  sessionManager.reconciliationQuarantineMs = 20;
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('.dispatch(')) { calls.push('dispatch'); return new Promise(() => {}); }
    if (expression.includes('requestState(')) return null;
    if (expression.includes('.cancel(')) { calls.push('cancel'); return null; }
    return null;
  };
  const req = request({ profileId, requestId });
  try {
    await assert.rejects(sessionManager.dispatch(req), /DISPATCH_RECONCILING/);
    assert.deepEqual(calls, ['dispatch']);
    await new Promise((resolve) => setTimeout(resolve, 50));
    const orphan = sessionManager.requestJournals.get(requestId).find((entry) => entry.stage === 'DISPATCH_ORPHANED');
    assert.equal(orphan.submissionState, 'SUBMITTED');
    assert.equal(orphan.reason, 'RESULT_RECONCILE_EXHAUSTED');
  } finally {
    sessionManager.ensureContentContract = originals.ensureContent;
    sessionManager.cdpEvaluate = originals.cdp;
    sessionManager.timeoutForRequest = originals.timeout;
    sessionManager.reconciliationQuarantineMs = originals.quarantine;
    sessionManager.sessions.delete(profileId);
    sessionManager.activeRequests.delete(requestId);
    sessionManager.terminalRequests.delete(requestId);
    sessionManager.requestJournals.delete(requestId);
    sessionManager.requestStateCache.delete(requestId);
  }
});

test('sticky sidecar POST_CREATED_LATE journal survives unavailable content state', async () => {
  const profileId = 'late-post-journal';
  const requestId = 'late-post-journal-1';
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  const req = request({ profileId, requestId });
  sessionManager.sessions.set(profileId, session);
  const originals = { ensureContent: sessionManager.ensureContentContract, cdp: sessionManager.cdpEvaluate, timeout: sessionManager.timeoutForRequest, quarantine: sessionManager.reconciliationQuarantineMs };
  sessionManager.ensureContentContract = async () => {};
  sessionManager.timeoutForRequest = () => 10;
  sessionManager.reconciliationQuarantineMs = 20;
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('.dispatch(')) {
      sessionManager.journalStage(req, 'POST_CREATED_LATE', { submissionState: 'SUBMITTED', postIdHash: 'POST_HASH', proofType: 'URL_POST_ID' });
      return new Promise(() => {});
    }
    if (expression.includes('requestState(')) return null;
    return null;
  };
  try {
    await assert.rejects(sessionManager.dispatch(req), /DISPATCH_RECONCILING/);
    await new Promise((resolve) => setTimeout(resolve, 50));
    const orphan = sessionManager.requestJournals.get(requestId).find((entry) => entry.stage === 'DISPATCH_ORPHANED');
    assert.equal(orphan.submissionState, 'SUBMITTED');
    assert.equal(orphan.reason, 'RESULT_RECONCILE_EXHAUSTED');
  } finally {
    sessionManager.ensureContentContract = originals.ensureContent;
    sessionManager.cdpEvaluate = originals.cdp;
    sessionManager.timeoutForRequest = originals.timeout;
    sessionManager.reconciliationQuarantineMs = originals.quarantine;
    sessionManager.sessions.delete(profileId);
    sessionManager.activeRequests.delete(requestId);
    sessionManager.terminalRequests.delete(requestId);
    sessionManager.requestJournals.delete(requestId);
    sessionManager.requestStateCache.delete(requestId);
  }
});

test('sticky submit milestone marks side effect possible after diagnostic tail eviction', () => {
  assert.equal(sessionManager.hasPossibleSubmission({
    submissionState: 'UNKNOWN',
    summary: { submissionState: 'UNKNOWN', sideEffectPossible: true, submitIntentObserved: true },
    entries: [],
  }), true);
});

test('submitted timeout is quarantined, then orphaned without DISPATCH_FAILED or retry', async () => {
  const profileId = 'submitted-orphan';
  const requestId = 'submitted-orphan-1';
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(profileId, session);
  const originals = {
    ensureContent: sessionManager.ensureContentContract,
    cdp: sessionManager.cdpEvaluate,
    timeout: sessionManager.timeoutForRequest,
    quarantine: sessionManager.reconciliationQuarantineMs,
  };
  sessionManager.ensureContentContract = async () => {};
  sessionManager.timeoutForRequest = () => 10;
  sessionManager.reconciliationQuarantineMs = 30;
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('requestState(')) return { submissionState: 'SUBMITTED', stage: 'RESULT_SCAN', entries: [{ stage: 'RESULT_SCAN', submissionState: 'SUBMITTED', baselineCount: 2, newCandidateCount: 0, newContainerCount: 0 }] };
    if (expression.includes('.dispatch(')) return new Promise(() => {});
    return null;
  };
  const req = request({ profileId, requestId, jobId: 'submitted-orphan-job' });
  try {
    await assert.rejects(sessionManager.dispatch(req), /DISPATCH_RECONCILING/);
    assert.equal(session.activeRequest?.requestId, requestId);
    assert.equal(sessionManager.activeRequests.has(requestId), true);
    const before = sessionManager.requestJournals.get(requestId).map((entry) => entry.stage);
    assert.ok(before.includes('DISPATCH_RECONCILING'));
    assert.equal(before.includes('DISPATCH_FAILED'), false);
    await new Promise((resolve) => setTimeout(resolve, 80));
    const journal = sessionManager.requestJournals.get(requestId);
    const orphan = journal.find((entry) => entry.stage === 'DISPATCH_ORPHANED');
    assert.ok(orphan);
    assert.equal(orphan.submissionState, 'SUBMITTED');
    assert.equal(orphan.retryable, false);
    assert.equal(orphan.duplicateDispatchAllowed, false);
    assert.equal(session.activeRequest, null);
    assert.equal(sessionManager.activeRequests.has(requestId), false);
    await assert.rejects(sessionManager.dispatch(req), /DUPLICATE_REQUEST/);
  } finally {
    sessionManager.ensureContentContract = originals.ensureContent;
    sessionManager.cdpEvaluate = originals.cdp;
    sessionManager.timeoutForRequest = originals.timeout;
    sessionManager.reconciliationQuarantineMs = originals.quarantine;
    sessionManager.sessions.delete(profileId);
    sessionManager.activeRequests.delete(requestId);
    sessionManager.terminalRequests.delete(requestId);
    sessionManager.requestJournals.delete(requestId);
  }
});

test('active cancel waits for dispatch settlement and uses the authoritative entry', async () => {
  let rejectDispatch;
  const session = { profileId: 'success-cancel', worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  const originalEnsureContent = sessionManager.ensureContentContract;
  const originalCdp = sessionManager.cdpEvaluate;
  sessionManager.ensureContentContract = async () => {};
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('.cancel(')) { const payload = JSON.parse(expression.slice(expression.indexOf('.cancel(') + 8, -1)); rejectDispatch(new Error('cancelled')); return cancelAck(payload); }
    return new Promise((_, reject) => { rejectDispatch = reject; });
  };
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
    sessionManager.ensureContentContract = originalEnsureContent;
    sessionManager.cdpEvaluate = originalCdp;
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
  const session = { profileId: 'late-settle', worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(session.profileId, session);
  const originalWorker = sessionManager.ensureWorker;
  const originalEnsureContent = sessionManager.ensureContentContract;
  const originalCdp = sessionManager.cdpEvaluate;
  const originalTimeout = sessionManager.timeoutForRequest;
  const originalSettle = sessionManager.cancelSettleTimeoutMs;
  sessionManager.timeoutForRequest = () => 10;
  sessionManager.cancelSettleTimeoutMs = 15;
  sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => payload.method === 'production.task.cancel' ? cancelAck(payload) : new Promise((resolve) => { resolveDispatch = resolve; }) });
  sessionManager.ensureContentContract = async () => {};
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('requestState(')) return { submissionState: 'NOT_SUBMITTED', stage: 'PREPARING', entries: [] };
    if (expression.includes('.cancel(')) { const payload = JSON.parse(expression.slice(expression.indexOf('.cancel(') + 8, -1)); return cancelAck(payload); }
    return new Promise((resolve) => { resolveDispatch = resolve; });
  };
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
    sessionManager.ensureContentContract = originalEnsureContent;
    sessionManager.cdpEvaluate = originalCdp;
    sessionManager.timeoutForRequest = originalTimeout;
    sessionManager.cancelSettleTimeoutMs = originalSettle;
    sessionManager.sessions.delete(session.profileId);
    sessionManager.activeRequests.clear();
  }
});

test('cancel acknowledgement with result.cancelled=false is not success', async () => {
  const session = { profileId: 'cancel-false', worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(session.profileId, session);
  const original = sessionManager.ensureWorker;
  const originalEnsureContent = sessionManager.ensureContentContract;
  const originalCdp = sessionManager.cdpEvaluate;
  sessionManager.ensureContentContract = async () => {};
  let rejectDispatch;
  sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => payload.method === 'production.task.cancel' ? { ok: true, result: { cancelled: false } } : new Promise((_, reject) => { rejectDispatch = reject; }) });
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('.cancel(')) return { ok: true, result: { cancelled: false } };
    return new Promise((_, reject) => { rejectDispatch = reject; });
  };
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
    sessionManager.ensureContentContract = originalEnsureContent;
    sessionManager.cdpEvaluate = originalCdp;
    sessionManager.sessions.delete(session.profileId);
    sessionManager.activeRequests.clear();
  }
});

test('cancel acknowledgement requires a cancelled result object', async () => {
  for (const response of [{ ok: true }, { ok: true, result: {} }, { ok: true, result: { cancelled: false } }]) {
    const session = { profileId: `missing-result-${Math.random()}`, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
    sessionManager.sessions.set(session.profileId, session);
    const original = sessionManager.ensureWorker;
    const originalEnsureContent = sessionManager.ensureContentContract;
    const originalCdp = sessionManager.cdpEvaluate;
    sessionManager.ensureContentContract = async () => {};
    let rejectDispatch;
    sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => payload.method === 'production.task.cancel' ? response : new Promise((_, reject) => { rejectDispatch = reject; }) });
    sessionManager.cdpEvaluate = async (_session, expression) => {
      if (expression.includes('.cancel(')) return response;
      return new Promise((_, reject) => { rejectDispatch = reject; });
    };
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
      sessionManager.ensureContentContract = originalEnsureContent;
      sessionManager.cdpEvaluate = originalCdp;
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
    const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
    sessionManager.sessions.set(profileId, session);
    const original = sessionManager.ensureWorker;
    const originalEnsureContent = sessionManager.ensureContentContract;
    const originalCdp = sessionManager.cdpEvaluate;
    sessionManager.ensureContentContract = async () => {};
    let rejectDispatch;
    sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => payload.method === 'production.task.cancel' ? mutate(cancelAck(payload)) : new Promise((_, reject) => { rejectDispatch = reject; }) });
    sessionManager.cdpEvaluate = async (_session, expression) => {
      if (expression.includes('.cancel(')) { const payload = JSON.parse(expression.slice(expression.indexOf('.cancel(') + 8, -1)); return mutate(cancelAck(payload)); }
      return new Promise((_, reject) => { rejectDispatch = reject; });
    };
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
      sessionManager.ensureContentContract = originalEnsureContent;
      sessionManager.cdpEvaluate = originalCdp;
      sessionManager.sessions.delete(profileId);
      sessionManager.activeRequests.clear();
    }
  }
});

test('complete cancel acknowledgement is accepted', async () => {
  const profileId = 'cancel-contract-valid';
  const session = { profileId, worker: {}, context: { serviceWorkers: () => [] }, cdpSession: {}, state: 'READY', activeRequest: null, requestStateSupported: true };
  sessionManager.sessions.set(profileId, session);
  const original = sessionManager.ensureWorker;
  const originalEnsureContent = sessionManager.ensureContentContract;
  const originalCdp = sessionManager.cdpEvaluate;
  sessionManager.ensureContentContract = async () => {};
  let rejectDispatch;
  sessionManager.ensureWorker = async () => ({ evaluate: async (_fn, payload) => {
    if (payload.method === 'production.task.cancel') { rejectDispatch(new Error('cancelled')); return cancelAck(payload); }
    return new Promise((_, reject) => { rejectDispatch = reject; });
  } });
  sessionManager.cdpEvaluate = async (_session, expression) => {
    if (expression.includes('.cancel(')) { const payload = JSON.parse(expression.slice(expression.indexOf('.cancel(') + 8, -1)); rejectDispatch(new Error('cancelled')); return cancelAck(payload); }
    return new Promise((_, reject) => { rejectDispatch = reject; });
  };
  try {
    const dispatch = sessionManager.dispatch(request({ profileId, requestId: 'contract-valid' }));
    await new Promise((resolve) => setImmediate(resolve));
    const result = await sessionManager.cancel('job-unit', 'contract-valid');
    assert.equal(result.cancelled, true);
    rejectDispatch(new Error('cancelled'));
    await assert.rejects(dispatch);
  } finally {
    sessionManager.ensureWorker = original;
    sessionManager.ensureContentContract = originalEnsureContent;
    sessionManager.cdpEvaluate = originalCdp;
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
