'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { test } = require('node:test');
const { SessionManager } = require('../src/session-manager');

const ID = { profileId: 'p1', cdpEndpoint: 'http://127.0.0.1:41000', browserPid: 41, launchGeneration: 7, browserEngine: 'CHROME_FOR_TESTING' };

function fakeStart(manager) {
  manager.startProfile = async (profileId, options) => {
    const s = { profileId, cdpEndpoint: options.cdpEndpoint, browserPid: options.browserPid, launchGeneration: options.launchGeneration, browserEngine: options.browserEngine, managedGrokTabId: options.grokTargetId, grokPage: {}, state: 'EXTENSION_READY', browser: { disconnect: async () => {} } };
    s.sessionKey = manager.sessionKey(profileId, s.launchGeneration, s.managedGrokTabId);
    manager.sessions.set(s.sessionKey, s);
    return { profileId, grokTargetId: s.managedGrokTabId, state: s.state };
  };
}

test('same profile supports two exact target sessions without overwrite', async () => {
  const manager = new SessionManager(); fakeStart(manager);
  await manager.ensureProfile('p1', { ...ID, grokTargetId: 'target-a' });
  await manager.ensureProfile('p1', { ...ID, grokTargetId: 'target-b' });
  assert.equal(manager.sessions.size, 2);
  assert.equal(manager.resolveSession('p1', 'target-a').managedGrokTabId, 'target-a');
  assert.equal(manager.resolveSession('p1', 'target-b').managedGrokTabId, 'target-b');
});

test('profile without target is ambiguous when multiple sessions exist', () => {
  const manager = new SessionManager();
  manager.sessions.set('a', { profileId: 'p1', managedGrokTabId: 'target-a' });
  manager.sessions.set('b', { profileId: 'p1', managedGrokTabId: 'target-b' });
  assert.throws(() => manager.resolveSession('p1'), /AMBIGUOUS_MANAGED_SESSION/);
});

test('disposing one target session does not remove another', async () => {
  const manager = new SessionManager();
  const disconnected = [];
  for (const target of ['target-a', 'target-b']) {
    const s = { profileId: 'p1', launchGeneration: 7, managedGrokTabId: target, sessionKey: manager.sessionKey('p1', 7, target), browser: { disconnect: async () => disconnected.push(target) } };
    manager.sessions.set(s.sessionKey, s);
  }
  await manager.stop('p1', 'target-a');
  assert.deepEqual(disconnected, ['target-a']);
  assert.ok(manager.resolveSession('p1', 'target-b'));
});

test('source remains attach-only and never opens or launches pages', async () => {
  const source = await fs.readFile(path.join(__dirname, '..', 'src', 'session-manager.js'), 'utf8');
  assert.doesNotMatch(source, /chromium\.launch|newPage\s*\(/);
});

