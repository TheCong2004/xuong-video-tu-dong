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
