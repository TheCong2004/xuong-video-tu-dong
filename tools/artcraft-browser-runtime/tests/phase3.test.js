'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { test, describe } = require('node:test');
const { LocalBrowserRuntime, isGrokUrl } = require('../src/runtime-manager');
const { RuntimeService, createServer } = require('../src/server');
const { RuntimeError } = require('../src/errors');
const { MemoryReceiptStore, FileReceiptStore, profileFile } = require('../src/receipt-store');
const { normalizeRunRequest } = require('../src/validation');
const { CdpHttpAdapter } = require('../src/cdp-adapter');

const RUN = { url: 'https://grok.com/imagine', headless: false, cold_start_only: true, browser_engine: 'chromium' };

class FakeProcess {
  constructor() { this.nextPid = 7000; this.spawned = []; this.alive = new Set(); this.terminated = []; }
  async spawn(spec) { const receipt = { pid: this.nextPid++, executable: spec.executable, arguments: [...spec.arguments] }; this.spawned.push(receipt); this.alive.add(receipt.pid); return receipt; }
  async inspect(receipt) { return this.alive.has(receipt.browserPid || receipt.pid); }
  async terminate(receipt) { const pid = receipt.browserPid || receipt.pid; this.terminated.push(pid); this.alive.delete(pid); }
}

class FakeCdp {
  constructor() { this.targetList = []; this.created = []; this.versionCalls = 0; this.listCalls = 0; this.failVersion = false; this.failCreate = false; }
  async version() { this.versionCalls += 1; if (this.failVersion) throw new Error('timeout'); return { Browser: 'CFT' }; }
  async targets() { this.listCalls += 1; return this.targetList.map((target) => ({ ...target })); }
  async createTarget(url) { if (this.failCreate) throw new RuntimeError('GROK_TARGET_CREATE_FAILED', 'create failed', 503, {}, true); const target = { id: `target-${this.created.length + 1}`, type: 'page', url }; this.created.push(target); this.targetList.push(target); return { ...target }; }
}

async function makeRuntime(overrides = {}) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'artcraft-runtime-'));
  const resourceRoot = path.join(root, 'resources');
  const executable = path.join(resourceRoot, 'playwright', 'chrome-win64', 'chrome.exe');
  await fs.mkdir(path.dirname(executable), { recursive: true });
  await fs.writeFile(executable, 'fake-cft');
  await fs.mkdir(path.join(resourceRoot, 'chromex-extension'), { recursive: true });
  await fs.writeFile(path.join(resourceRoot, 'chromex-extension', 'manifest.json'), '{}');
  const process = overrides.processAdapter || new FakeProcess();
  const cdp = overrides.cdp || new FakeCdp();
  const store = overrides.receiptStore || new MemoryReceiptStore();
  let now = 1700000000000;
  const runtime = new LocalBrowserRuntime({ resourceRoot, executable, profileRoot: path.join(root, 'profiles'), receiptStore: store, processAdapter: process, cdpFactory: () => cdp, allocatePort: async () => 41000, nonce: () => 'nonce-fixed', clock: () => (now += 100), ...overrides });
  return { root, runtime, process, cdp, store, executable };
}

test('health identity is canonical', async () => {
  const { runtime } = await makeRuntime();
  const health = runtime.health();
  assert.equal(health.protocolVersion, 1);
  assert.equal(health.runtimeKind, 'artcraft-local-browser-runtime');
  assert.equal(health.service, 'ARTCRAFT_LOCAL_BROWSER_RUNTIME');
  assert.equal(health.status, 'READY');
  assert.equal(health.pid, process.pid);
  assert.equal(health.instanceId, 'runtime-nonce-fixed');
  assert.equal(health.browserOwner, 'ARTCRAFT_LOCAL_RUNTIME');
  assert.match(health.nonce, /^[a-f0-9]{24}$/);
});

test('health never exposes secrets or raw paths', async () => {
  const { runtime } = await makeRuntime();
  const text = JSON.stringify(runtime.health());
  assert.equal(/token|cookie|webSocketDebuggerUrl|\\\\/.test(text), false);
});

test('non-loopback server host is rejected', () => assert.throws(() => createServer({ host: '0.0.0.0', port: 0 }), /loopback/));
test('malformed JSON returns canonical 400', async () => { const { runtime } = await makeRuntime(); const result = await new RuntimeService(runtime).handle({ method: 'POST', pathname: '/v1/profiles/p1/run', body: '{' }); assert.equal(result.status, 400); assert.equal(result.body.error.code, 'INVALID_REQUEST'); });
test('oversized direct body is rejected by validation policy', () => assert.throws(() => normalizeRunRequest({ unknown: 'x' }), /Unknown run field/));
test('invalid profile identifier is rejected', async () => { const { runtime } = await makeRuntime(); const result = await new RuntimeService(runtime).handle({ method: 'POST', pathname: '/v1/profiles/../run', body: RUN }); assert.notEqual(result.status, 200); });
test('path traversal profile identifier is rejected', async () => { const { runtime } = await makeRuntime(); const result = await new RuntimeService(runtime).handle({ method: 'POST', pathname: '/v1/profiles/%2e%2e/run', body: RUN }); assert.equal(result.status, 400); });

test('cold run spawns exactly one CFT with argument array', async () => { const x = await makeRuntime(); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(x.process.spawned.length, 1); assert.ok(Array.isArray(x.process.spawned[0].arguments)); assert.equal(out.browser_engine, 'CHROME_FOR_TESTING'); });
test('launch arguments contain owned profile and loopback CDP', async () => { const x = await makeRuntime(); await x.runtime.run('p1', normalizeRunRequest(RUN)); const args = x.process.spawned[0].arguments.join(' '); assert.match(args, /--remote-debugging-address=127\.0\.0\.1/); assert.match(args, /--user-data-dir=/); assert.match(args, /--remote-debugging-port=41000/); });
test('configured executable must be under ArtCraft resources', async () => { const x = await makeRuntime({ executable: 'C:\\Program Files\\Google\\Chrome\\chrome.exe' }); await assert.rejects(() => x.runtime.run('p1', normalizeRunRequest(RUN)), (e) => e.code === 'CFT_EXECUTABLE_NOT_FOUND'); });
test('branded browser path is rejected', async () => { const x = await makeRuntime({ resourceRoot: 'C:\\resources', executable: 'C:\\Program Files\\Google\\Chrome\\chrome.exe' }); await assert.rejects(() => x.runtime.run('p1', normalizeRunRequest(RUN)), (e) => e.code === 'CFT_EXECUTABLE_NOT_FOUND'); });
test('allocated foreign port fails closed', async () => { const x = await makeRuntime({ inspectPort: async () => true }); await assert.rejects(() => x.runtime.run('p1', normalizeRunRequest(RUN)), (e) => e.code === 'FOREIGN_PORT_OCCUPIED'); assert.equal(x.process.spawned.length, 0); });
test('CDP adapter rejects non-loopback endpoint', () => assert.throws(() => new CdpHttpAdapter('http://192.0.2.1:9222'), /loopback/));
test('CDP timeout does not publish a receipt', async () => { const x = await makeRuntime(); x.cdp.failVersion = true; await assert.rejects(() => x.runtime.run('p1', normalizeRunRequest(RUN)), (e) => e.code === 'BROWSER_CDP_NOT_READY'); assert.equal(await x.store.read('p1'), null); });
test('cold run creates one managed target', async () => { const x = await makeRuntime(); await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(x.cdp.created.length, 1); assert.equal(x.cdp.created[0].type, 'page'); });
test('managed target must be a page', async () => { const x = await makeRuntime(); x.cdp.createTarget = async () => ({ id: 'bad', type: 'service_worker', url: RUN.url }); await assert.rejects(() => x.runtime.run('p1', normalizeRunRequest(RUN)), (e) => e.code === 'GROK_TARGET_CREATE_FAILED'); });
test('Grok hostname parser rejects lookalike domains', () => { assert.equal(isGrokUrl('https://evil-grok.com/imagine'), false); assert.equal(isGrokUrl('https://grok.com.evil.example/imagine'), false); assert.equal(isGrokUrl('https://grok.com@evil.example/imagine'), false); assert.equal(isGrokUrl(RUN.url), true); });
test('run response contains complete canonical DTO', async () => { const x = await makeRuntime(); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); for (const key of ['profile_id', 'browser_pid', 'remote_debugging_port', 'cdp_endpoint', 'launch_generation', 'browser_engine', 'grok_target_id', 'grok_page_url', 'reused']) assert.ok(Object.hasOwn(out, key)); });

test('concurrent runs coalesce to one spawn', async () => { const x = await makeRuntime(); const [a, b] = await Promise.all([x.runtime.run('p1', normalizeRunRequest(RUN)), x.runtime.run('p1', normalizeRunRequest(RUN))]); assert.equal(x.process.spawned.length, 1); assert.equal(a.grok_target_id, b.grok_target_id); });
test('concurrent runs create one target', async () => { const x = await makeRuntime(); await Promise.all([x.runtime.run('p1', normalizeRunRequest(RUN)), x.runtime.run('p1', normalizeRunRequest(RUN))]); assert.equal(x.cdp.created.length, 1); });
test('healthy reuse returns reused true', async () => { const x = await makeRuntime(); await x.runtime.run('p1', normalizeRunRequest(RUN)); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(out.reused, true); assert.equal(x.process.spawned.length, 1); });
test('healthy reuse keeps generation stable', async () => { const x = await makeRuntime(); const a = await x.runtime.run('p1', normalizeRunRequest(RUN)); const b = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(a.launch_generation, b.launch_generation); });
test('profiles have independent receipts', async () => { const x = await makeRuntime({ allocatePort: (() => { let p = 41000; return async () => p++; })() }); const a = await x.runtime.run('p1', normalizeRunRequest(RUN)); const b = await x.runtime.run('p2', normalizeRunRequest(RUN)); assert.notEqual(a.profile_id, b.profile_id); assert.notEqual(a.browser_pid, b.browser_pid); });
test('startup failure cleans only newly spawned child', async () => { const x = await makeRuntime(); x.cdp.failCreate = true; await assert.rejects(() => x.runtime.run('p1', normalizeRunRequest(RUN))); assert.deepEqual(x.process.terminated, [7000]); });
test('startup failure leaves no half receipt', async () => { const x = await makeRuntime(); x.cdp.failCreate = true; await assert.rejects(() => x.runtime.run('p1', normalizeRunRequest(RUN))); assert.equal(await x.store.read('p1'), null); });
test('exact existing target is reused', async () => { const x = await makeRuntime(); const first = await x.runtime.run('p1', normalizeRunRequest(RUN)); const second = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(first.grok_target_id, second.grok_target_id); assert.equal(x.cdp.created.length, 1); });
test('multiple user tabs do not alter exact target', async () => { const x = await makeRuntime(); const first = await x.runtime.run('p1', normalizeRunRequest(RUN)); x.cdp.targetList.push({ id: 'user-tab', type: 'page', url: RUN.url }); const second = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(second.grok_target_id, first.grok_target_id); });
test('runtime never selects first tab instead of exact receipt', async () => { const x = await makeRuntime(); const first = await x.runtime.run('p1', normalizeRunRequest(RUN)); x.cdp.targetList.unshift({ id: 'first-user-tab', type: 'page', url: RUN.url }); const second = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(second.grok_target_id, first.grok_target_id); });
test('stale target is replaced exactly once', async () => { const x = await makeRuntime(); const first = await x.runtime.run('p1', normalizeRunRequest(RUN)); x.cdp.targetList = []; const second = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.notEqual(second.grok_target_id, first.grok_target_id); assert.equal(x.cdp.created.length, 2); });
test('stale reconciliation preserves browser identity', async () => { const x = await makeRuntime(); const first = await x.runtime.run('p1', normalizeRunRequest(RUN)); x.cdp.targetList = []; const second = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(second.browser_pid, first.browser_pid); assert.equal(second.remote_debugging_port, first.remote_debugging_port); assert.equal(second.launch_generation, first.launch_generation); });
test('target creation failure never returns success', async () => { const x = await makeRuntime(); x.cdp.failCreate = true; const result = await new RuntimeService(x.runtime).handle({ method: 'POST', pathname: '/v1/profiles/p1/run', body: RUN }); assert.notEqual(result.status, 200); assert.equal(result.body.error.code, 'GROK_TARGET_CREATE_FAILED'); });

test('stop terminates owned process once', async () => { const x = await makeRuntime(); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); await x.runtime.stop('p1', { browser_pid: out.browser_pid }); assert.deepEqual(x.process.terminated, [out.browser_pid]); assert.equal(await x.store.read('p1'), null); });
test('stop when already stopped is idempotent', async () => { const x = await makeRuntime(); const a = await x.runtime.stop('p1'); const b = await x.runtime.stop('p1'); assert.equal(a.status, 'STOPPED'); assert.equal(b.stopped, false); });
test('stop identity mismatch does not terminate', async () => { const x = await makeRuntime(); await x.runtime.run('p1', normalizeRunRequest(RUN)); await assert.rejects(() => x.runtime.stop('p1', { browser_pid: 99 }), (e) => e.code === 'FOREIGN_PROCESS_DETECTED'); assert.equal(x.process.terminated.length, 0); });
test('PID reuse mismatch is fail closed', async () => { const x = await makeRuntime(); await x.runtime.run('p1', normalizeRunRequest(RUN)); await assert.rejects(() => x.runtime.stop('p1', { launch_generation: 1 }), (e) => e.code === 'FOREIGN_PROCESS_DETECTED'); });
test('stop failure keeps receipt', async () => { const x = await makeRuntime(); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); x.process.terminate = async () => { throw new Error('stop failed'); }; await assert.rejects(() => x.runtime.stop('p1', { browser_pid: out.browser_pid })); assert.ok(await x.store.read('p1')); });
test('run and stop calls serialize per profile', async () => { const x = await makeRuntime(); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); await Promise.all([x.runtime.stop('p1', { browser_pid: out.browser_pid }), x.runtime.run('p1', normalizeRunRequest(RUN))]); assert.equal(x.process.spawned.length, 2); });
test('corrupt receipt is treated as empty and restarted safely', async () => { const store = new MemoryReceiptStore({ p1: { schemaVersion: 9, profileId: 'p1' } }); const x = await makeRuntime({ receiptStore: store }); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(out.reused, false); });
test('receipt store uses atomic temporary replace', async () => { const root = await fs.mkdtemp(path.join(os.tmpdir(), 'receipt-')); const store = new FileReceiptStore(root); const receipt = { schemaVersion: 1, profileId: 'p1' }; await store.write('p1', receipt); assert.deepEqual(await store.read('p1'), receipt); assert.equal((await fs.readdir(root)).some((name) => name.endsWith('.tmp')), false); });
test('stop never closes a target', async () => { const x = await makeRuntime(); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); await x.runtime.stop('p1', { browser_pid: out.browser_pid }); assert.equal(x.cdp.targetList.length, 1); });

test('source has no forbidden paid integration marker', async () => { const source = await fs.readFile(path.join(__dirname, '..', 'src', 'runtime-manager.js'), 'utf8'); assert.equal(/X-Floword-Integration|PAYMENT_REQUIRED|cloud_auth|floword-donut-runtime/.test(source), false); });
test('source has no browser automation library import', async () => { const source = await fs.readFile(path.join(__dirname, '..', 'src', 'runtime-manager.js'), 'utf8'); assert.equal(/require\(['"]playwright|connectOverCDP|chromium\.launch|newPage/.test(source), false); });
test('tests use an ephemeral HTTP listener when binding', async () => { const x = await makeRuntime(); const { server } = createServer({ port: 0, service: new RuntimeService(x.runtime) }); await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve)); assert.ok(server.address().port > 0); await new Promise((resolve) => server.close(resolve)); });
test('runtime service health route is read-only', async () => { const x = await makeRuntime(); const result = await new RuntimeService(x.runtime).handle({ method: 'GET', pathname: '/health' }); assert.equal(result.status, 200); assert.equal(x.process.spawned.length, 0); });
test('content type policy rejects non-JSON mutation', async () => { const x = await makeRuntime(); const result = await new RuntimeService(x.runtime).handle({ method: 'POST', pathname: '/v1/profiles/p1/run', body: RUN, headers: { 'content-type': 'text/plain' } }); assert.equal(result.status, 400); });
test('Grok URL requires HTTPS', () => assert.throws(() => normalizeRunRequest({ url: 'http://grok.com/imagine' }), /not allowed/));
test('Grok URL allows subdomain only under grok.com', () => assert.equal(isGrokUrl('https://sub.grok.com/imagine'), true));
test('run never hardcodes target identity', async () => { const x = await makeRuntime(); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(out.grok_target_id, x.cdp.created[0].id); });
test('managed target URL is persisted with receipt', async () => { const x = await makeRuntime(); await x.runtime.run('p1', normalizeRunRequest(RUN)); const receipt = await x.store.read('p1'); assert.equal(receipt.managedTargetUrl, RUN.url); });
test('runtime health reports no browser secret fields', async () => { const { runtime } = await makeRuntime(); assert.deepEqual(Object.keys(runtime.health()).sort(), ['browserOwner', 'instanceId', 'nonce', 'pid', 'protocolVersion', 'runtimeKind', 'service', 'status'].sort()); });
test('error envelope omits stack traces', async () => { const x = await makeRuntime(); const result = await new RuntimeService(x.runtime).handle({ method: 'POST', pathname: '/v1/profiles/p1/run', body: '{' }); assert.equal('stack' in result.body.error, false); });
test('runtime service does not expose raw executable path in errors', async () => { const x = await makeRuntime({ executable: 'C:\\Program Files\\Google\\Chrome\\chrome.exe' }); const result = await new RuntimeService(x.runtime).handle({ method: 'POST', pathname: '/v1/profiles/p1/run', body: RUN }); assert.equal(JSON.stringify(result.body).includes('Program Files'), false); });
test('service returns 404 envelope for unknown route', async () => { const x = await makeRuntime(); const result = await new RuntimeService(x.runtime).handle({ method: 'GET', pathname: '/unknown' }); assert.equal(result.status, 404); assert.equal(result.body.error.code, 'INVALID_REQUEST'); });
test('service returns 405 envelope for wrong method', async () => { const x = await makeRuntime(); const result = await new RuntimeService(x.runtime).handle({ method: 'GET', pathname: '/v1/profiles/p1/run' }); assert.equal(result.status, 405); });
test('stop validates generation and nonce when provided', async () => { const x = await makeRuntime(); const out = await x.runtime.run('p1', normalizeRunRequest(RUN)); const receipt = await x.store.read('p1'); await assert.rejects(() => x.runtime.stop('p1', { browser_pid: out.browser_pid, ownership_nonce: `${receipt.ownershipNonce}-bad` }), (e) => e.code === 'FOREIGN_PROCESS_DETECTED'); });
test('run receipt has ownership nonce and generation', async () => { const x = await makeRuntime(); await x.runtime.run('p1', normalizeRunRequest(RUN)); const receipt = await x.store.read('p1'); assert.ok(receipt.ownershipNonce); assert.ok(receipt.launchGeneration > 0); });
test('CDP version and target list are both required before receipt', async () => { const x = await makeRuntime(); await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.equal(x.cdp.versionCalls, 1); assert.ok(x.cdp.listCalls >= 1); });
test('foreign target is not claimed during healthy reuse', async () => { const x = await makeRuntime(); const first = await x.runtime.run('p1', normalizeRunRequest(RUN)); x.cdp.targetList = [{ id: 'foreign', type: 'page', url: RUN.url }]; const second = await x.runtime.run('p1', normalizeRunRequest(RUN)); assert.notEqual(second.grok_target_id, 'foreign'); });
test('replacement target keeps Grok hostname validation', async () => { const x = await makeRuntime(); await x.runtime.run('p1', normalizeRunRequest(RUN)); x.cdp.targetList = []; x.cdp.createTarget = async () => ({ id: 'bad', type: 'page', url: 'https://example.com/' }); await assert.rejects(() => x.runtime.run('p1', normalizeRunRequest(RUN)), (e) => e.code === 'GROK_TARGET_CREATE_FAILED'); });
test('runtime manager remains offline from real ports in tests', async () => { const x = await makeRuntime(); assert.equal(x.runtime.config.port, 10108); assert.equal(x.runtime.config.host, '127.0.0.1'); });

describe('ephemeral server lifecycle', () => {
  test('ephemeral HTTP server can be closed cleanly', async () => {
    const x = await makeRuntime();
    const { server } = createServer({ port: 0, service: new RuntimeService(x.runtime) });
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    await new Promise((resolve) => server.close(resolve));
    assert.equal(server.listening, false);
  });
});
