const test = require('node:test');
const assert = require('node:assert/strict');
const sessionManager = require('../src/session-manager');

test('Donut-owned Chromium attaches through CDP and reuses profile', async (t) => {
  const cdpEndpoint = process.env.FLOWORD_CDP_ENDPOINT;
  const profileId = process.env.FLOWORD_CDP_PROFILE_ID || 'fixture-profile';
  const browserPid = Number(process.env.FLOWORD_CDP_BROWSER_PID);
  const launchGeneration = Number(process.env.FLOWORD_CDP_LAUNCH_GENERATION);
  if (!cdpEndpoint || !Number.isInteger(browserPid) || !Number.isInteger(launchGeneration)) {
    t.skip('set FLOWORD_CDP_ENDPOINT, FLOWORD_CDP_BROWSER_PID and FLOWORD_CDP_LAUNCH_GENERATION for a Donut-owned browser');
    return;
  }
  const identity = { cdpEndpoint, browserPid, launchGeneration };
  const first = await sessionManager.ensureProfile(profileId, { ...identity, url: 'https://grok.com/imagine' });
  assert.match(first.serviceWorkerUrl, /^chrome-extension:\/\//);
  const health = await sessionManager.health(profileId);
  assert.equal(health.profileId, profileId);
  const request = { protocol: 'floword-production', protocolVersion: 1, requestId: 'req-1', jobId: 'job-1', stepId: 'image', attemptId: 'attempt-1', leaseId: 'lease-1', profileId, method: 'grok.image.edit', params: {}, createdAt: new Date().toISOString() };
  const dispatched = await sessionManager.dispatch(request);
  assert.equal(dispatched.requestId, request.requestId);
  assert.deepEqual(await sessionManager.dispatch(request), dispatched);
  const cancelled = await sessionManager.cancel('missing-job');
  assert.equal(cancelled.cancelled, false);
  const second = await sessionManager.ensureProfile(profileId, identity);
  assert.equal(second.serviceWorkerUrl, first.serviceWorkerUrl);
  await sessionManager.stop(profileId);
});
