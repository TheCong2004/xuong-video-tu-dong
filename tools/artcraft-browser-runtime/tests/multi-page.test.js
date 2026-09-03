'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { test } = require('node:test');
const { LocalBrowserRuntime } = require('../src/runtime-manager');
const { MemoryReceiptStore } = require('../src/receipt-store');
const { normalizeRunRequest, normalizePageRequest } = require('../src/validation');
const { RuntimeService } = require('../src/server');

class Process {
  constructor() { this.pid = 9000; this.alive = new Set(); this.spawnCount = 0; this.terminateCount = 0; }
  async spawn(spec) { this.spawnCount += 1; const pid = this.pid++; this.alive.add(pid); return { pid, executable: spec.executable, arguments: spec.arguments }; }
  async inspect(receipt) { return this.alive.has(receipt.browserPid || receipt.pid); }
  async terminate(receipt) { this.terminateCount += 1; this.alive.delete(receipt.browserPid || receipt.pid); }
}

class Cdp {
  constructor() { this.targetList = []; this.created = []; this.closed = []; this.versionCalls = 0; }
  async version() { this.versionCalls += 1; return { Browser: 'CFT' }; }
  async targets() { return this.targetList.map((target) => ({ ...target })); }
  async createTarget(url) { const target = { id: `t-${this.created.length + 1}`, type: 'page', url, title: '' }; this.created.push(target); this.targetList.push(target); return target; }
  async closeTarget(id) { this.closed.push(id); this.targetList = this.targetList.filter((target) => target.id !== id); return { id, closed: true }; }
}

async function fixture() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'artcraft-multi-page-'));
  const resourceRoot = path.join(root, 'resources');
  const executable = path.join(resourceRoot, 'playwright', 'chrome-win64', 'chrome.exe');
  await fs.mkdir(path.dirname(executable), { recursive: true });
  await fs.writeFile(executable, 'cft');
  await fs.mkdir(path.join(resourceRoot, 'chromex-extension'), { recursive: true });
  await fs.writeFile(path.join(resourceRoot, 'chromex-extension', 'manifest.json'), '{}');
  const process = new Process(); const cdp = new Cdp();
  const runtime = new LocalBrowserRuntime({ resourceRoot, executable, profileRoot: path.join(root, 'profiles'), receiptStore: new MemoryReceiptStore(), processAdapter: process, cdpFactory: () => cdp, allocatePort: async () => 42000, nonce: () => 'fixed', clock: (() => { let now = 1700000000000; return () => ++now; })() });
  return { runtime, process, cdp };
}

const RUN = normalizeRunRequest({ url: 'https://grok.com/imagine', ensure_page: true });

test('legacy single target migrates into pages map', async () => {
  const x = await fixture();
  const out = await x.runtime.run('p1', RUN);
  const receipt = await x.runtime.receipts.read('p1');
  assert.equal(receipt.pages[out.grok_target_id].purpose, 'GROK_AUTOMATION');
});

test('browser-only local run does not create a page', async () => {
  const x = await fixture();
  const out = await x.runtime.run('p1', { ...RUN, ensurePage: false });
  assert.equal(out.grok_target_id, null);
  assert.equal(x.cdp.created.length, 0);
  assert.equal(x.process.spawnCount, 1);
});

test('page API creates and lists multiple exact targets in one browser', async () => {
  const x = await fixture();
  await x.runtime.run('p1', { ...RUN, ensurePage: false });
  const a = await x.runtime.createPage('p1', normalizePageRequest({ url: 'https://grok.com/imagine', purpose: 'GROK_AUTOMATION' }));
  const b = await x.runtime.createPage('p1', normalizePageRequest({ url: 'https://grok.com/imagine/post/1', purpose: 'RESULT', reuseExisting: false }));
  const list = await x.runtime.listPages('p1');
  assert.equal(x.process.spawnCount, 1);
  assert.notEqual(a.page.targetId, b.page.targetId);
  assert.equal(list.pages.length, 2);
  assert.deepEqual(list.pages.map((p) => p.targetId).sort(), [a.page.targetId, b.page.targetId].sort());
});

test('same purpose reuses managed page', async () => {
  const x = await fixture(); await x.runtime.run('p1', { ...RUN, ensurePage: false });
  const a = await x.runtime.createPage('p1', normalizePageRequest({ purpose: 'GROK_AUTOMATION' }));
  const b = await x.runtime.createPage('p1', normalizePageRequest({ purpose: 'GROK_AUTOMATION' }));
  assert.equal(a.page.targetId, b.page.targetId); assert.equal(b.created, false); assert.equal(x.cdp.created.length, 1);
});

test('delete closes only managed page and preserves another page', async () => {
  const x = await fixture(); await x.runtime.run('p1', { ...RUN, ensurePage: false });
  const a = await x.runtime.createPage('p1', normalizePageRequest({ purpose: 'GROK_AUTOMATION' }));
  const b = await x.runtime.createPage('p1', normalizePageRequest({ purpose: 'RESULT', reuseExisting: false }));
  await x.runtime.deletePage('p1', a.page.targetId);
  const list = await x.runtime.listPages('p1');
  assert.deepEqual(x.cdp.closed, [a.page.targetId]); assert.equal(list.pages.length, 1); assert.equal(list.pages[0].targetId, b.page.targetId);
});

test('user page cannot be claimed or deleted', async () => {
  const x = await fixture(); await x.runtime.run('p1', { ...RUN, ensurePage: false });
  x.cdp.targetList.push({ id: 'user', type: 'page', url: 'https://grok.com/imagine', title: 'user' });
  const list = await x.runtime.listPages('p1'); const user = list.pages.find((p) => p.targetId === 'user');
  assert.equal(user.managed, false); await assert.rejects(() => x.runtime.deletePage('p1', 'user'), (e) => e.code === 'PAGE_NOT_OWNED');
});

test('local HTTP pages routes are idempotent and browser-only run is separate', async () => {
  const x = await fixture(); const service = new RuntimeService(x.runtime);
  const run = await service.handle({ method: 'POST', pathname: '/v1/local/browser/profiles/p1/run', body: { url: 'https://grok.com/imagine' } });
  assert.equal(run.status, 200); assert.equal(run.body.grok_target_id, null);
  const first = await service.handle({ method: 'POST', pathname: '/v1/local/browser/profiles/p1/pages', body: { purpose: 'GROK_AUTOMATION' } });
  const second = await service.handle({ method: 'POST', pathname: '/v1/local/browser/profiles/p1/pages', body: { purpose: 'GROK_AUTOMATION' } });
  assert.equal(first.body.page.targetId, second.body.page.targetId); assert.equal(second.body.created, false);
});
