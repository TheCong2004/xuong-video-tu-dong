'use strict';

const fs = require('node:fs/promises');
const path = require('node:path');
const net = require('node:net');
const crypto = require('node:crypto');
const { RuntimeError } = require('./errors');
const { DEFAULT_URL } = require('./validation');
const { FileReceiptStore } = require('./receipt-store');
const { RealProcessAdapter } = require('./process-adapter');
const { CdpHttpAdapter } = require('./cdp-adapter');

function fingerprint(value) {
  return crypto.createHash('sha256').update(String(value)).digest('hex');
}

function isGrokUrl(value) {
  try {
    const url = new URL(value);
    const host = url.hostname.toLowerCase();
    return url.protocol === 'https:' && (host === 'grok.com' || host.endsWith('.grok.com'));
  } catch { return false; }
}

function validTarget(target) {
  return Boolean(target && target.type === 'page' && isGrokUrl(target.url || ''));
}

const PAGE_PURPOSES = new Set(['GROK_AUTOMATION', 'LOGIN', 'RESULT', 'USER', 'UNKNOWN']);

function pageReceipt(target, base, purpose = 'GROK_AUTOMATION', managed = true, state = 'LIVE') {
  return {
    targetId: target.id,
    type: 'page',
    url: target.url || '',
    title: target.title || '',
    hostname: (() => { try { return new URL(target.url || '').hostname.toLowerCase(); } catch { return ''; } })(),
    purpose,
    managed,
    state,
    browserPid: base.browserPid,
    launchGeneration: base.launchGeneration,
    createdAt: base.createdAt,
    updatedAt: new Date().toISOString(),
  };
}

function normalizedReceipt(receipt) {
  if (!receipt) return null;
  const pages = receipt.pages && typeof receipt.pages === 'object' && !Array.isArray(receipt.pages) ? { ...receipt.pages } : {};
  if (receipt.managedTargetId && !pages[receipt.managedTargetId]) {
    pages[receipt.managedTargetId] = {
      targetId: receipt.managedTargetId,
      type: receipt.managedTargetType || 'page',
      url: receipt.managedTargetUrl || '',
      title: '',
      hostname: (() => { try { return new URL(receipt.managedTargetUrl || '').hostname.toLowerCase(); } catch { return ''; } })(),
      purpose: 'GROK_AUTOMATION',
      managed: true,
      state: 'LIVE',
      browserPid: receipt.browserPid,
      launchGeneration: receipt.launchGeneration,
      createdAt: receipt.createdAt,
      updatedAt: receipt.updatedAt,
    };
  }
  return { ...receipt, pages };
}

async function allocateLoopbackPort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      server.close(() => resolve(port));
    });
  });
}

class LocalBrowserRuntime {
  constructor(options = {}) {
    this.config = {
      resourceRoot: options.resourceRoot || process.env.ARTCRAFT_RUNTIME_RESOURCE_ROOT || path.resolve(__dirname, '..', '..', '..', 'resources'),
      profileRoot: options.profileRoot || process.env.ARTCRAFT_PROFILE_ROOT || path.join(process.env.LOCALAPPDATA || process.cwd(), 'ArtCraft', 'profiles'),
      executable: options.executable || process.env.FLOWORD_CHROMIUM_EXECUTABLE || null,
      browserEngine: 'CHROME_FOR_TESTING',
      host: options.host || '127.0.0.1',
      port: Number(options.port || process.env.ARTCRAFT_BROWSER_RUNTIME_PORT || 10108),
      startupTimeoutMs: options.startupTimeoutMs || 15000,
    };
    this.fs = options.fs || fs;
    this.clock = options.clock || (() => Date.now());
    this.nonce = options.nonce || (() => crypto.randomBytes(16).toString('hex'));
    this.allocatePort = options.allocatePort || allocateLoopbackPort;
    this.inspectPort = options.inspectPort || (async () => false);
    this.receipts = options.receiptStore || new FileReceiptStore(path.join(this.config.profileRoot, '.receipts'));
    this.processAdapter = options.processAdapter || new RealProcessAdapter();
    this.cdpFactory = options.cdpFactory || ((endpoint) => new CdpHttpAdapter(endpoint));
    this.locks = new Map();
    this.instanceId = options.instanceId || `runtime-${this.nonce()}`;
    this.ready = true;
  }

  health() {
    return {
      protocolVersion: 1,
      runtimeKind: 'artcraft-local-browser-runtime',
      service: 'ARTCRAFT_LOCAL_BROWSER_RUNTIME',
      status: this.ready ? 'READY' : 'STARTING',
      pid: process.pid,
      instanceId: this.instanceId,
      nonce: fingerprint(this.instanceId).slice(0, 24),
      browserOwner: 'ARTCRAFT_LOCAL_RUNTIME',
    };
  }

  async run(profileId, request) {
    const previous = this.locks.get(profileId) || Promise.resolve();
    const operation = previous.then(() => this._run(profileId, request));
    const gate = operation.catch(() => undefined);
    this.locks.set(profileId, gate);
    try { return await operation; } finally { if (this.locks.get(profileId) === gate) this.locks.delete(profileId); }
  }

  async _run(profileId, request) {
    const profileDirectory = path.join(this.config.profileRoot, profileId);
    await this.fs.mkdir(profileDirectory, { recursive: true });
    let receipt = normalizedReceipt(await this.receipts.read(profileId));
    if (receipt) {
      const alive = await this.processAdapter.inspect(receipt);
      if (alive) {
        const cdp = this.cdpFactory(receipt.cdpEndpoint);
        const targets = await cdp.targets();
        receipt = await this.reconcilePages(receipt, targets);
        const managed = Object.values(receipt.pages).find((page) => page.managed && page.purpose === 'GROK_AUTOMATION' && page.state === 'LIVE');
        if (!managed && request.ensurePage !== false) receipt = await this.reconcileTarget(receipt, cdp, request.url);
        await this.receipts.write(profileId, receipt);
        return this.identityFromReceipt(receipt, true, request.ensurePage !== false);
      }
      await this.receipts.clear(profileId);
      receipt = null;
    }

    const executable = this.resolveExecutable();
    try {
      const stat = await this.fs.stat(executable);
      if (!stat.isFile()) throw new Error('not a file');
    } catch {
      throw new RuntimeError('CFT_EXECUTABLE_NOT_FOUND', 'Chrome for Testing executable is not available', 503, {}, true);
    }
    const port = await this.allocatePort();
    if (await this.inspectPort(port)) throw new RuntimeError('FOREIGN_PORT_OCCUPIED', 'Allocated CDP port is occupied', 409);
    const endpoint = `http://${this.config.host}:${port}`;
    const extensionPath = path.join(this.config.resourceRoot, 'chromex-extension');
    try {
      const extensionManifest = await this.fs.stat(path.join(extensionPath, 'manifest.json'));
      if (!extensionManifest.isFile()) throw new Error('manifest is not a file');
    } catch {
      throw new RuntimeError('EXTENSION_ARTIFACT_NOT_STAGED', 'Chromex unpacked extension is not available', 503, {}, true);
    }
    const args = [
      '--remote-debugging-address=127.0.0.1',
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${profileDirectory}`,
      `--load-extension=${extensionPath}`,
      `--disable-extensions-except=${extensionPath}`,
      '--no-first-run',
      '--no-default-browser-check',
    ];
    const processReceipt = await this.processAdapter.spawn({ executable, arguments: args, workingDirectory: path.dirname(executable) });
    const baseReceipt = {
      schemaVersion: 1,
      profileId,
      browserPid: processReceipt.pid,
      executableFingerprint: fingerprint(executable),
      argumentFingerprint: fingerprint(args.join('\0')),
      profileDirectoryFingerprint: fingerprint(profileDirectory),
      remoteDebuggingPort: port,
      cdpEndpoint: endpoint,
      launchGeneration: this.clock(),
      browserEngine: 'CHROME_FOR_TESTING',
      runtimeInstanceId: this.instanceId,
      ownershipNonce: this.nonce(),
      createdAt: new Date(this.clock()).toISOString(),
      updatedAt: new Date(this.clock()).toISOString(),
      processReceipt: { pid: processReceipt.pid },
    };
    try {
      const cdp = this.cdpFactory(endpoint);
      await this.waitForCdp(cdp);
      receipt = { ...baseReceipt, pages: {} };
      if (request.ensurePage !== false) {
        const target = await this.createManagedTarget(cdp, request.url || DEFAULT_URL);
        const page = pageReceipt(target, receipt, 'GROK_AUTOMATION', true, 'LIVE');
        receipt = { ...receipt, pages: { [page.targetId]: page }, managedTargetId: page.targetId, managedTargetUrl: page.url, managedTargetType: 'page' };
      }
      await this.receipts.write(profileId, receipt);
      return this.identityFromReceipt(receipt, false, request.ensurePage !== false);
    } catch (error) {
      await this.processAdapter.terminate(baseReceipt).catch(() => undefined);
      throw error;
    }
  }

  resolveExecutable() {
    const candidate = this.config.executable || path.join(this.config.resourceRoot, 'playwright', 'chrome-win64', 'chrome.exe');
    const normalized = path.resolve(candidate);
    const root = path.resolve(this.config.resourceRoot);
    if (!normalized.toLowerCase().startsWith(`${root.toLowerCase()}${path.sep}`)) {
      throw new RuntimeError('CFT_EXECUTABLE_NOT_FOUND', 'Configured Chrome for Testing executable is outside ArtCraft resources', 503, {}, true);
    }
    if (normalized.toLowerCase().includes('program files\\google\\chrome')) {
      throw new RuntimeError('CFT_EXECUTABLE_NOT_FOUND', 'Branded browser executable is not allowed', 503);
    }
    return normalized;
  }

  async waitForCdp(cdp) {
    const deadline = this.clock() + this.config.startupTimeoutMs;
    let lastError;
    while (this.clock() <= deadline) {
      try { await cdp.version(); await cdp.targets(); return; } catch (error) { lastError = error; }
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new RuntimeError('BROWSER_CDP_NOT_READY', 'Browser CDP did not become ready', 503, {}, true, lastError);
  }

  async createManagedTarget(cdp, url) {
    const target = await cdp.createTarget(url);
    if (!validTarget({ ...target, type: target.type || 'page', url: target.url || url })) {
      throw new RuntimeError('GROK_TARGET_CREATE_FAILED', 'Created target is not a valid Grok page', 503, {}, true);
    }
    return { ...target, type: 'page', url: target.url || url };
  }

  async reconcilePages(receipt, targets) {
    const next = {};
    const targetMap = new Map((targets || []).filter((target) => target && target.type === 'page' && target.id).map((target) => [target.id, target]));
    for (const [targetId, page] of Object.entries(receipt.pages || {})) {
      const live = targetMap.get(targetId);
      next[targetId] = live
        ? { ...page, targetId, type: 'page', url: live.url || page.url || '', title: live.title || page.title || '', hostname: (() => { try { return new URL(live.url || page.url || '').hostname.toLowerCase(); } catch { return ''; } })(), state: 'LIVE', updatedAt: new Date().toISOString() }
        : { ...page, targetId, state: 'STALE', updatedAt: new Date().toISOString() };
    }
    for (const [targetId, target] of targetMap) {
      if (!next[targetId]) next[targetId] = pageReceipt(target, receipt, 'USER', false, 'LIVE');
    }
    return { ...receipt, pages: next };
  }

  async reconcileTarget(receipt, cdp, url) {
    const target = await this.createManagedTarget(cdp, url || receipt.managedTargetUrl || DEFAULT_URL);
    const page = pageReceipt(target, receipt, 'GROK_AUTOMATION', true, 'LIVE');
    return { ...receipt, pages: { ...(receipt.pages || {}), [page.targetId]: page }, managedTargetId: target.id, managedTargetUrl: target.url, managedTargetType: 'page', updatedAt: new Date(this.clock()).toISOString() };
  }

  identityFromReceipt(receipt, reused, includePage = true) {
    const managed = Object.values(receipt.pages || {}).find((page) => page.managed && page.purpose === 'GROK_AUTOMATION' && page.state === 'LIVE');
    return {
      profile_id: receipt.profileId,
      browser_pid: receipt.browserPid,
      remote_debugging_port: receipt.remoteDebuggingPort,
      cdp_endpoint: receipt.cdpEndpoint,
      launch_generation: receipt.launchGeneration,
      browser_engine: receipt.browserEngine,
      grok_target_id: includePage ? (managed?.targetId || receipt.managedTargetId || null) : null,
      grok_page_url: includePage ? (managed?.url || receipt.managedTargetUrl || null) : null,
      reused,
    };
  }

  async assertBrowserReceipt(profileId) {
    const receipt = normalizedReceipt(await this.receipts.read(profileId));
    if (!receipt) throw new RuntimeError('PROFILE_NOT_FOUND', 'Browser profile is not running', 404);
    if (!(await this.processAdapter.inspect(receipt))) throw new RuntimeError('PLAYWRIGHT_PROFILE_OFFLINE', 'Browser profile is not running', 503, {}, true);
    const cdp = this.cdpFactory(receipt.cdpEndpoint);
    const targets = await cdp.targets();
    const reconciled = await this.reconcilePages(receipt, targets);
    await this.receipts.write(profileId, reconciled);
    return { receipt: reconciled, cdp, targets };
  }

  async listPages(profileId) {
    const { receipt } = await this.assertBrowserReceipt(profileId);
    return {
      profileId,
      browserPid: receipt.browserPid,
      remoteDebuggingPort: receipt.remoteDebuggingPort,
      cdpEndpoint: receipt.cdpEndpoint,
      launchGeneration: receipt.launchGeneration,
      pages: Object.values(receipt.pages || {}).map(({ targetId, type, url, title, hostname, purpose, managed, state, browserPid, launchGeneration, createdAt, updatedAt }) => ({ targetId, type, url: String(url || '').split(/[?#]/, 1)[0], title, hostname, purpose, managed, state, browserPid, launchGeneration, createdAt, updatedAt })),
    };
  }

  async createPage(profileId, request) {
    const previous = this.locks.get(profileId) || Promise.resolve();
    const operation = previous.then(async () => {
      const { receipt, cdp } = await this.assertBrowserReceipt(profileId);
      const existing = Object.values(receipt.pages || {}).find((page) => page.managed && page.purpose === request.purpose && page.state === 'LIVE' && request.reuseExisting);
      if (existing) return { profileId, browserPid: receipt.browserPid, remoteDebuggingPort: receipt.remoteDebuggingPort, cdpEndpoint: receipt.cdpEndpoint, launchGeneration: receipt.launchGeneration, page: existing, created: false };
      const target = await this.createManagedTarget(cdp, request.url);
      const page = pageReceipt(target, receipt, request.purpose, true, 'LIVE');
      const next = { ...receipt, pages: { ...(receipt.pages || {}), [page.targetId]: page }, updatedAt: new Date(this.clock()).toISOString() };
      await this.receipts.write(profileId, next);
      return { profileId, browserPid: receipt.browserPid, remoteDebuggingPort: receipt.remoteDebuggingPort, cdpEndpoint: receipt.cdpEndpoint, launchGeneration: receipt.launchGeneration, page, created: true };
    });
    const gate = operation.catch(() => undefined); this.locks.set(profileId, gate);
    try { return await operation; } finally { if (this.locks.get(profileId) === gate) this.locks.delete(profileId); }
  }

  async deletePage(profileId, targetId) {
    if (typeof targetId !== 'string' || !targetId.trim()) throw new RuntimeError('PAGE_TARGET_ID_REQUIRED', 'targetId is required', 400);
    const { receipt, cdp, targets } = await this.assertBrowserReceipt(profileId);
    const page = receipt.pages?.[targetId];
    if (!page || page.managed !== true) throw new RuntimeError('PAGE_NOT_OWNED', 'Page is not owned by ArtCraft runtime', 409);
    if (!targets.some((target) => target.id === targetId)) throw new RuntimeError('PAGE_NOT_FOUND', 'Managed page is not present in CDP', 404);
    await cdp.closeTarget(targetId);
    const nextPages = { ...(receipt.pages || {}) }; delete nextPages[targetId];
    await this.receipts.write(profileId, { ...receipt, pages: nextPages, updatedAt: new Date(this.clock()).toISOString() });
    return { profileId, targetId, closed: true };
  }

  async stop(profileId, input = {}) {
    const receipt = await this.receipts.read(profileId);
    if (!receipt) return { profile_id: profileId, status: 'STOPPED', stopped: false };
    if (input.browser_pid !== undefined && Number(input.browser_pid) !== receipt.browserPid) throw new RuntimeError('FOREIGN_PROCESS_DETECTED', 'Process identity mismatch', 409);
    if (input.remote_debugging_port !== undefined && Number(input.remote_debugging_port) !== receipt.remoteDebuggingPort) throw new RuntimeError('FOREIGN_PROCESS_DETECTED', 'CDP identity mismatch', 409);
    if (input.launch_generation !== undefined && Number(input.launch_generation) !== receipt.launchGeneration) throw new RuntimeError('FOREIGN_PROCESS_DETECTED', 'Launch identity mismatch', 409);
    if (input.ownership_nonce !== undefined && input.ownership_nonce !== receipt.ownershipNonce) throw new RuntimeError('FOREIGN_PROCESS_DETECTED', 'Ownership identity mismatch', 409);
    if (await this.processAdapter.inspect(receipt)) await this.processAdapter.terminate(receipt);
    await this.receipts.clear(profileId);
    return { profile_id: profileId, status: 'STOPPED', stopped: true };
  }
}

module.exports = { LocalBrowserRuntime, fingerprint, isGrokUrl, validTarget };
