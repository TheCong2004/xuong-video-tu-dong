const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { DonutSession } = require('../src/donut-session');
const sessionManager = require('../src/session-manager');

function fakePage(url, title, targetId, counters) {
  const locator = {
    click: async () => ({ ok: true }),
    fill: async () => ({ ok: true }),
    setInputFiles: async () => ({ ok: true }),
  };
  return {
    url: () => url,
    title: async () => title,
    locator: () => locator,
    getByRole: () => locator,
    getByText: () => locator,
    getByLabel: () => locator,
    getByPlaceholder: () => locator,
    isClosed: () => false,
    bringToFront: async () => {},
    goto: async () => { counters.goto += 1; throw new Error('goto must not be called'); },
    reload: async () => { counters.reload += 1; throw new Error('reload must not be called'); },
    _targetId: targetId,
  };
}

function fakeBrowser(pages) {
  const counters = { connect: 0, newPage: 0, goto: 0, reload: 0, close: 0, disconnect: 0, cdp: 0 };
  const context = {
    pages: () => pages,
    newPage: async () => { counters.newPage += 1; throw new Error('newPage must not be called'); },
    newCDPSession: async (page) => {
      counters.cdp += 1;
      return {
        send: async (method) => {
          assert.equal(method, 'Target.getTargetInfo');
          return { targetInfo: { targetId: page._targetId } };
        },
        detach: async () => {},
      };
    },
  };
  const browser = {
    contexts: () => [context],
    isConnected: () => true,
    close: async () => { counters.close += 1; },
    disconnect: async () => { counters.disconnect += 1; },
  };
  const chromium = { connectOverCDP: async () => { counters.connect += 1; return browser; } };
  return { browser, context, chromium, counters };
}

const identity = { profileId: 'profile-1', cdpEndpoint: 'http://127.0.0.1:9222', browserPid: 42, launchGeneration: 7 };

test('attaches exact managed Grok target among multiple tabs without navigation', async () => {
  const counters = { goto: 0, reload: 0 };
  const grok = fakePage('https://grok.com/imagine', 'Grok', 'target-grok', counters);
  const oldGrok = fakePage('https://grok.com/imagine/post/old', 'Old result', 'target-old', counters);
  const fake = fakeBrowser([oldGrok, grok]);
  const session = await DonutSession.connect({ ...identity, grokTargetId: 'target-grok', browser: fake.browser, context: fake.context });
  assert.equal(session.rawPage, grok);
  assert.equal(await session.targetId(session.rawPage), 'target-grok');
  assert.deepEqual((await session.tabs.list()).pages.map((p) => ({ targetId: p.targetId, managed: p.managed })), [
    { targetId: 'target-old', managed: false },
    { targetId: 'target-grok', managed: true },
  ]);
  assert.equal(counters.goto, 0);
  assert.equal(counters.reload, 0);
  assert.equal(fake.counters.newPage, 0);
});

test('stale managed target fails closed without falling back to another tab', async () => {
  const counters = { goto: 0, reload: 0 };
  const grok = fakePage('https://grok.com/imagine', 'Grok', 'target-live', counters);
  const fake = fakeBrowser([grok]);
  await assert.rejects(DonutSession.connect({ ...identity, grokTargetId: 'target-stale', browser: fake.browser, context: fake.context }), /GROK_MANAGED_TARGET_STALE/);
  assert.equal(counters.goto, 0);
  assert.equal(fake.counters.newPage, 0);
});

test('non-Grok managed target fails closed', async () => {
  const counters = { goto: 0, reload: 0 };
  const login = fakePage('https://accounts.google.com/signin', 'Login', 'target-login', counters);
  const fake = fakeBrowser([login]);
  await assert.rejects(DonutSession.connect({ ...identity, grokTargetId: 'target-login', browser: fake.browser, context: fake.context }), /GROK_MANAGED_TARGET_STALE/);
});

test('facade preserves identity and never owns browser lifecycle', async () => {
  const counters = { goto: 0, reload: 0 };
  const grok = fakePage('https://grok.com/imagine', 'Grok', 'target-grok', counters);
  const fake = fakeBrowser([grok]);
  const session = await DonutSession.connect({ ...identity, grokTargetId: 'target-grok', browser: fake.browser, context: fake.context });
  assert.deepEqual({ profileId: session.profileId, cdpEndpoint: session.cdpEndpoint, browserPid: session.browserPid, launchGeneration: session.launchGeneration }, {
    profileId: 'profile-1', cdpEndpoint: 'http://127.0.0.1:9222', browserPid: 42, launchGeneration: 7,
  });
  await assert.rejects(session.tabs.open(), /TAB_CREATION_FORBIDDEN/);
  await assert.rejects(session.tabs.close(), /TAB_CLOSE_FORBIDDEN/);
  await session.disconnect();
  assert.equal(fake.counters.close, 0);
  assert.equal(fake.counters.disconnect, 1);
  assert.equal(counters.goto, 0);
  assert.equal(counters.reload, 0);
});

test('identity is mandatory and facade remains compatible with CDP binding fields', async () => {
  const fake = fakeBrowser([]);
  await assert.rejects(DonutSession.connect({ ...identity, browserPid: null, browser: fake.browser, context: fake.context }), /CDP_IDENTITY_REQUIRED/);
  await assert.rejects(DonutSession.connect({ ...identity, launchGeneration: null, browser: fake.browser, context: fake.context }), /CDP_IDENTITY_REQUIRED/);
  const state = { cdpSession: { protocol: 'floword-production' }, contentContextId: 9, contentFrameId: 'main', contentOrigin: 'chrome-extension://floword' };
  assert.equal(state.cdpSession.protocol, 'floword-production');
  assert.equal(state.contentContextId, 9);
});

test('session facade rebind is idempotent and preserves production CDP fields', async () => {
  const counters = { goto: 0, reload: 0 };
  const grok = fakePage('https://grok.com/imagine', 'Grok', 'target-grok', counters);
  const fake = fakeBrowser([grok]);
  const s = {
    profileId: 'profile-rebind',
    cdpEndpoint: 'http://127.0.0.1:9222',
    browserPid: 51,
    launchGeneration: 11,
    browserEngine: 'CHROME_FOR_TESTING',
    browser: fake.browser,
    context: fake.context,
    cdpSession: { protocol: 'floword-production' },
    contentContextId: 12,
    contentFrameId: 'main-frame',
    contentOrigin: 'chrome-extension://floword',
  };
  const first = await sessionManager.ensureSessionFacade(s, grok);
  const second = await sessionManager.ensureSessionFacade(s, grok);
  assert.strictEqual(first, second);
  assert.equal(s.cdpSession.protocol, 'floword-production');
  assert.equal(s.contentContextId, 12);
  assert.equal(s.contentFrameId, 'main-frame');
  assert.equal(s.browserPid, 51);
  assert.equal(s.launchGeneration, 11);
  assert.equal(fake.counters.connect, 0);
});

test('managed-target errors do not echo cookie or token material', async () => {
  const counters = { goto: 0, reload: 0 };
  const grok = fakePage('https://grok.com/imagine', 'Grok', 'target-live', counters);
  const fake = fakeBrowser([grok]);
  const secret = 'Bearer super-secret-token';
  await assert.rejects(
    DonutSession.connect({ ...identity, grokTargetId: secret, browser: fake.browser, context: fake.context }),
    (error) => !String(error.message).includes(secret) && !String(error.message).includes('super-secret-token'),
  );
});

test('sidecar start/dispose source has no Donut lifecycle calls', () => {
  const source = fs.readFileSync(path.join(__dirname, '..', 'src', 'session-manager.js'), 'utf8');
  const facade = fs.readFileSync(path.join(__dirname, '..', 'src', 'donut-session.js'), 'utf8');
  assert.doesNotMatch(source, /DonutRestClient|rest\.(run|stop)\s*\(/);
  assert.doesNotMatch(facade, /DonutRestClient|static\s+(run|stop)\s*\(/);
});

test('locator facade strips non-Playwright humanize metadata', async () => {
  const counters = { goto: 0, reload: 0 };
  const grok = fakePage('https://grok.com/imagine', 'Grok', 'target-grok', counters);
  const fake = fakeBrowser([grok]);
  const session = await DonutSession.connect({ ...identity, browser: fake.browser, context: fake.context });
  await session.page.getByRole('button', { name: 'Submit' }).click({ humanize: true, timeout: 1000 });
  await session.page.locator('#prompt').fill('safe text', { humanize: true });
});
