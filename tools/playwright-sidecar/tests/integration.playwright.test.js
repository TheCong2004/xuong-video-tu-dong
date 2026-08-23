const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const sessionManager = require('../src/session-manager');

test('persistent Chromium loads MV3 worker and reuses profile', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'floword-playwright-'));
  process.env.FLOWORD_PLAYWRIGHT_PROFILE_ROOT = root;
  const fixture = path.join(__dirname, 'fixtures');
  const first = await sessionManager.ensureProfile('fixture-profile', { extensionPath: fixture, url: 'about:blank' });
  assert.match(first.serviceWorkerUrl, /^chrome-extension:\/\//);
  const health = await sessionManager.health('fixture-profile');
  assert.equal(health.profileId, 'fixture-profile');
  const second = await sessionManager.ensureProfile('fixture-profile', { extensionPath: fixture });
  assert.equal(second.serviceWorkerUrl, first.serviceWorkerUrl);
  await sessionManager.stop('fixture-profile');
});
