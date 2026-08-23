const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

function canonical(value) { if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`; if (value && typeof value === 'object') return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`; return JSON.stringify(value); }

class SessionManager {
  constructor() { this.sessions = new Map(); this.startFlights = new Map(); this.activeRequests = new Map(); this.completedRequests = new Map(); this.completedTtlMs = 30 * 60 * 1000; this.maxCompletedRequests = 500; this.cancelSettleTimeoutMs = 15000; }
  // Retained as validation helpers for callers/tests. CDP mode never creates
  // or opens this path; the browser profile remains owned by Donut.
  profileDir(id) {
    if (!id || !/^[a-z0-9][a-z0-9-]{0,127}$/i.test(id)) throw new Error('INVALID_PROFILE: profileId is required');
    const root = path.resolve(process.env.FLOWORD_PLAYWRIGHT_PROFILE_ROOT || path.join(process.env.LOCALAPPDATA || process.cwd(), 'Floword', 'playwright-profiles'));
    return path.resolve(root, id);
  }
  extensionDir(value) {
    const dir = path.resolve(value || process.env.FLOWORD_CHROMEX_EXTENSION_PATH || '');
    if (!dir || !fs.existsSync(path.join(dir, 'manifest.json'))) throw new Error(`EXTENSION_NOT_LOADED: manifest.json not found at ${dir}`);
    return dir;
  }
  async ensureProfile(id, options = {}) {
    if (this.sessions.has(id)) {
      const s = this.sessions.get(id);
      if ((options.cdpEndpoint && s.cdpEndpoint !== options.cdpEndpoint) ||
        (options.browserPid && s.browserPid && s.browserPid !== options.browserPid) ||
        (options.launchGeneration && s.launchGeneration && s.launchGeneration !== options.launchGeneration)) {
        if (s.activeRequest) throw new Error('CDP_SESSION_STALE: browser endpoint changed while a request was active');
        await s.browser.close().catch(() => {});
        this.sessions.delete(id);
      } else {
        return this.describe(s, await this.ensureGrokPage(s, options.url));
      }
    }
    if (this.startFlights.has(id)) return this.startFlights.get(id);
    const flight = this.startProfile(id, options).finally(() => this.startFlights.delete(id)); this.startFlights.set(id, flight); return flight;
  }
  async startProfile(id, options = {}) {
    if (!id || !/^[a-z0-9][a-z0-9-]{0,127}$/i.test(id)) throw new Error('INVALID_PROFILE: profileId is required');
    const cdpEndpoint = options.cdpEndpoint || (options.cdpPort ? `http://127.0.0.1:${options.cdpPort}` : null);
    if (!cdpEndpoint) throw new Error('CDP_ENDPOINT_REQUIRED: Donut must provide the owned browser CDP endpoint');
    const browser = await chromium.connectOverCDP(cdpEndpoint, { timeout: options.timeoutMs || 15000 });
    const context = browser.contexts()[0];
    if (!context) { await browser.close().catch(() => {}); throw new Error('CDP_CONTEXT_NOT_FOUND: Donut browser exposed no browser context'); }
    const s = { profileId: id, cdpEndpoint, browserPid: options.browserPid || null, launchGeneration: options.launchGeneration || null, browser, userDataDir: null, extensionPath: null, context, worker: null, grokPage: null, managedGrokTabId: null, activeRequest: null, state: 'STARTING', isTracing: false, lastHeartbeat: Date.now() }; this.sessions.set(id, s);
    try { s.worker = await this.waitForServiceWorker(context, options.timeoutMs || 15000); s.state = 'BROWSER_READY'; const page = await this.ensureGrokPage(s, options.url); await this.bindProfile(s); s.state = 'EXTENSION_READY'; return this.describe(s, page); }
    catch (e) { await browser.close().catch(() => {}); this.sessions.delete(id); throw e; }
  }
  async waitForServiceWorker(context, timeout) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      for (const worker of context.serviceWorkers().filter((candidate) => candidate.url().startsWith('chrome-extension://'))) {
        try {
          const ready = await worker.evaluate(() => Boolean(globalThis.__flowordProduction && typeof globalThis.__flowordProduction.bind === 'function' && typeof globalThis.__flowordProduction.health === 'function'));
          if (ready) return worker;
        } catch (_) { /* worker may be restarting */ }
      }
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error('EXTENSION_PRODUCTION_WORKER_NOT_READY: Donut browser has no Floword production service worker');
  }
  async ensureGrokPage(s, url = 'https://grok.com/imagine') {
    let page = s.grokPage && !s.grokPage.isClosed() ? s.grokPage : null;
    if (!page && s.managedGrokTabId) page = s.context.pages().find((p) => p.url() === s.managedGrokTabId);
    if (!page) {
      const grokPages = s.context.pages().filter((candidate) => /^https:\/\/(www\.)?grok\.com\//i.test(candidate.url()));
      if (grokPages.length > 1) throw new Error('AMBIGUOUS_GROK_TAB: multiple Grok tabs exist without a managed mapping');
      page = grokPages[0];
    }
    if (!page) throw new Error('GROK_TAB_NOT_FOUND: Donut did not expose a managed Grok tab');
    if (!/^https:\/\/(www\.)?grok\.com\//i.test(page.url()) && url) await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
    await page.bringToFront().catch(() => {}); await page.waitForLoadState('domcontentloaded').catch(() => {});
    s.grokPage = page; s.managedGrokTabId = page.url(); s.lastHeartbeat = Date.now(); return page;
  }
  async bindProfile(s) { let last; for (let attempt = 0; attempt < 30; attempt += 1) { try { const result = await s.worker.evaluate((profileId) => globalThis.__flowordProduction?.bind(profileId), s.profileId); if (result?.ok) return result; last = result?.error || { code: 'CONTENT_SCRIPT_NOT_READY', message: 'Content script has not acknowledged binding' }; if (last.code === 'INVALID_PROFILE') break; } catch (error) { last = { code: 'CONTENT_SCRIPT_NOT_READY', message: error.message }; } await new Promise((resolve) => setTimeout(resolve, 300)); } throw new Error(`${last?.code || 'CONTENT_SCRIPT_BIND_TIMEOUT'}: ${last?.message || 'Profile binding timed out'}`); }
  timeoutForRequest(request) {
    const requested = Number(request?.params?.timeoutMs);
    const fallback = request?.method === 'grok.video.generate' ? 300000 : 180000;
    if (!Number.isFinite(requested)) return fallback;
    return Math.min(Math.max(Math.trunc(requested), 5000), 900000);
  }
  async ensureWorker(s) {
    const alive = s.worker && !s.worker.isClosed();
    if (!alive || !s.context.serviceWorkers().includes(s.worker)) {
      s.worker = await this.waitForServiceWorker(s.context, 15000);
      await this.bindProfile(s);
      s.state = 'EXTENSION_READY';
    }
    return s.worker;
  }
  describe(s, page) { return { profileId: s.profileId, userDataDir: s.userDataDir, extensionId: s.worker?.url().match(/^chrome-extension:\/\/([^/]+)/)?.[1] || null, serviceWorkerUrl: s.worker?.url() || null, grokUrl: page?.url() || null, browserOpen: true, state: s.state, activeRequest: s.activeRequest }; }
  async health(id) {
    const s = this.sessions.get(id); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started');
    const worker = await this.ensureWorker(s); const result = await worker.evaluate((profileId) => globalThis.__flowordProduction?.health(profileId), id);
    if (!result) throw new Error('EXTENSION_PRODUCTION_BRIDGE_NOT_FOUND: health bridge returned no result');
    if (result.protocol !== 'floword-production' || result.protocolVersion !== 1 || result.result?.profileId && result.result.profileId !== id) throw new Error('PROTOCOL_MISMATCH: health response protocol/profile is invalid');
    s.lastHeartbeat = Date.now();
    if (!result.ok) return result;
    const details = { ...(result.result || {}) };
    if (s.activeRequest) { s.state = 'BUSY'; details.status = 'BUSY'; details.workerState = 'BUSY'; return { ...result, result: details }; }
    const status = String(details.status || '').toUpperCase();
    const workerState = String(details.workerState || '').toUpperCase();
    if (details.loggedIn === false || status === 'LOGIN_REQUIRED') s.state = 'LOGIN_REQUIRED';
    else if (status === 'BUSY' || status === 'LEASED' || workerState === 'BUSY' || workerState === 'LEASED') s.state = 'BUSY';
    else if (status === 'READY' && workerState === 'IDLE') s.state = 'READY';
    else if (s.state !== 'RECONCILING') s.state = 'EXTENSION_READY';
    if (s.state === 'RECONCILING') { details.status = 'RECONCILING'; details.workerState = 'RECONCILING'; }
    return { ...result, result: details };
  }
  async cancelRequest(s, active, timeoutMs = 10000) {
    const worker = await this.ensureWorker(s);
    const cancelRequest = { protocol: 'floword-production', protocolVersion: 1, requestId: `CANCEL_${crypto.randomUUID()}`, jobId: active.jobId, stepId: active.stepId, attemptId: active.attemptId, leaseId: active.leaseId, profileId: active.profileId, method: 'production.task.cancel', params: { targetRequestId: active.requestId }, createdAt: new Date().toISOString() };
    const cancelRequestId = cancelRequest.requestId;
    const cancelPromise = worker.evaluate((payload) => globalThis.__flowordProduction?.cancel(payload), cancelRequest);
    let cancelTimer;
    try {
      const result = await Promise.race([cancelPromise, new Promise((_, reject) => { cancelTimer = setTimeout(() => reject(new Error('CANCEL_TIMEOUT: cancellation acknowledgement timed out')), Math.min(Math.max(timeoutMs, 1000), 15000)); })]);
      const valid = result?.protocol === 'floword-production' &&
        result?.protocolVersion === 1 &&
        result?.ok === true &&
        result?.result?.cancelled === true &&
        result?.requestId === cancelRequestId &&
        result?.jobId === active.jobId &&
        result?.stepId === active.stepId &&
        result?.attemptId === active.attemptId &&
        result?.leaseId === active.leaseId &&
        result?.profileId === active.profileId;
      if (!valid) {
        const correlationMismatch = result && (result.requestId !== cancelRequestId || result.jobId !== active.jobId || result.stepId !== active.stepId || result.attemptId !== active.attemptId || result.leaseId !== active.leaseId || result.profileId !== active.profileId);
        throw new Error(`${correlationMismatch ? 'CORRELATION_MISMATCH' : 'CANCEL_UNCONFIRMED'}: cancellation acknowledgement is incomplete or invalid`);
      }
      return { cancelled: true, requestId: active.requestId, acknowledgment: result };
    } finally {
      clearTimeout(cancelTimer);
    }
  }
  async dispatch(request) {
    const { profileId, requestId, jobId } = request; if (!profileId || !requestId || !jobId) throw new Error('INVALID_REQUEST: profileId, requestId and jobId are required');
    this.pruneCompleted(); const fingerprint = crypto.createHash('sha256').update(canonical({ requestId, jobId, stepId: request.stepId, attemptId: request.attemptId, leaseId: request.leaseId, profileId, method: request.method, params: request.params })).digest('hex');
    const active = this.activeRequests.get(requestId); if (active) { if (active.fingerprint !== fingerprint) throw new Error('CORRELATION_CONFLICT: requestId was reused with different payload'); return active.promise; }
    const completed = this.completedRequests.get(requestId); if (completed) { if (completed.fingerprint !== fingerprint) throw new Error('CORRELATION_CONFLICT: requestId was reused with different payload'); return completed.result; }
    const s = this.sessions.get(profileId); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started');
    const worker = await this.ensureWorker(s);
    if (s.state === 'RECONCILING') throw new Error('WORKER_RECONCILING: profile is recovering after an unconfirmed cancellation');
    if (s.activeRequest) throw new Error('JOB_ALREADY_RUNNING: profile has an active request');
    const activeRequest = { ...request };
    s.activeRequest = activeRequest; s.state = 'BUSY';
    const timeoutMs = this.timeoutForRequest(request);
    let timeoutHandle;
    const dispatchPromise = worker.evaluate((payload) => globalThis.__flowordProduction?.dispatch(payload), request);
    const timeoutPromise = new Promise((_, reject) => { timeoutHandle = setTimeout(() => reject(new Error('RESULT_TIMEOUT: dispatch timed out')), timeoutMs); });
    let retainOwnership = false;
    const promise = (async () => {
      try {
        const result = await Promise.race([dispatchPromise, timeoutPromise]);
        if (!result || typeof result !== 'object') throw new Error('INVALID_EXTENSION_RESPONSE: dispatch returned no object');
        if (result.protocol !== 'floword-production' || result.protocolVersion !== 1 || result.requestId !== request.requestId || result.jobId !== request.jobId || result.stepId !== request.stepId || result.attemptId !== request.attemptId || result.leaseId !== request.leaseId || result.profileId !== request.profileId) throw new Error('CORRELATION_MISMATCH: extension response did not echo request identity');
        this.completedRequests.set(requestId, { fingerprint, result, expiresAt: Date.now() + this.completedTtlMs });
        while (this.completedRequests.size > this.maxCompletedRequests) this.completedRequests.delete(this.completedRequests.keys().next().value);
        return result;
      } catch (error) {
        if (String(error?.message || error).startsWith('RESULT_TIMEOUT:')) {
          try {
            await this.cancelRequest(s, activeRequest, 10000);
            const settled = await Promise.race([dispatchPromise.then(() => true).catch(() => true), new Promise((resolve) => setTimeout(() => resolve(false), this.cancelSettleTimeoutMs))]);
            if (!settled) { retainOwnership = true; s.state = 'RECONCILING'; this.watchLateSettlement(s, requestId, dispatchPromise); throw new Error('CANCEL_UNCONFIRMED: dispatch did not settle after cancellation'); }
            s.state = 'RECONCILING';
          } catch (cancelError) {
            s.state = 'RECONCILING';
            throw new Error(`CANCEL_UNCONFIRMED: ${cancelError.message}`);
          }
        }
        throw error;
      } finally {
        clearTimeout(timeoutHandle);
        if (!retainOwnership) {
          if (s.activeRequest?.requestId === requestId) s.activeRequest = null;
          if (s.state === 'BUSY') s.state = 'RECONCILING';
          this.activeRequests.delete(requestId);
        }
      }
    })();
    this.activeRequests.set(requestId, { fingerprint, promise, dispatchPromise, session: s, request: activeRequest }); return promise;
  }
  watchLateSettlement(s, requestId, dispatchPromise) {
    dispatchPromise.then(() => this.reapLateSettlement(s, requestId, dispatchPromise), () => this.reapLateSettlement(s, requestId, dispatchPromise));
  }
  reapLateSettlement(s, requestId, dispatchPromise) {
    const entry = this.activeRequests.get(requestId);
    if (entry?.session === s && entry.dispatchPromise === dispatchPromise) this.activeRequests.delete(requestId);
    if (s.activeRequest?.requestId === requestId) s.activeRequest = null;
    s.state = 'RECONCILING';
  }
  async cancel(jobId, targetRequestId) {
    const s = [...this.sessions.values()].find((x) => x.activeRequest?.jobId === jobId);
    if (!s) return { cancelled: false, jobId, requestId: targetRequestId || null, acknowledgment: { ok: false, code: 'JOB_NOT_FOUND' } };
    const active = s.activeRequest;
    if (targetRequestId && active.requestId !== targetRequestId) return { cancelled: false, jobId, requestId: targetRequestId, acknowledgment: { ok: false, code: 'CORRELATION_MISMATCH' } };
    const entry = this.activeRequests.get(active.requestId);
    if (!entry || entry.session !== s || entry.request.jobId !== jobId || entry.request.profileId !== s.profileId) { s.state = 'RECONCILING'; return { cancelled: false, jobId, requestId: active.requestId, acknowledgment: { ok: false, code: 'CANCEL_UNCONFIRMED' } }; }
    let result;
    try { result = await this.cancelRequest(s, active); }
    catch (error) { s.state = 'RECONCILING'; return { cancelled: false, jobId, requestId: active.requestId, acknowledgment: { ok: false, code: 'CANCEL_UNCONFIRMED', message: error.message } }; }
    const settled = await Promise.race([entry.dispatchPromise.then(() => true).catch(() => true), new Promise((resolve) => setTimeout(() => resolve(false), this.cancelSettleTimeoutMs))]);
    if (!settled) {
      this.watchLateSettlement(s, active.requestId, entry.dispatchPromise);
      s.state = 'RECONCILING';
      return { cancelled: false, jobId, requestId: active.requestId, acknowledgment: { ok: false, code: 'CANCEL_UNCONFIRMED' } };
    }
    if (result.requestId !== targetRequestId || result.acknowledgment?.ok !== true || result.acknowledgment?.result?.cancelled !== true) { s.state = 'RECONCILING'; return { cancelled: false, jobId, requestId: active.requestId, acknowledgment: { ok: false, code: 'CANCEL_UNCONFIRMED' } }; }
    return { cancelled: true, jobId, ...result };
  }
  pruneCompleted() { const now = Date.now(); for (const [id, entry] of this.completedRequests) if (entry.expiresAt <= now) this.completedRequests.delete(id); }
  async stop(id) { const s = this.sessions.get(id); if (!s) return { stopped: false, profileId: id }; await s.browser.close().catch(() => {}); this.sessions.delete(id); return { stopped: true, profileId: id, browserOwnedBy: 'donut' }; }
  async getPages(id) { const s = this.sessions.get(id); if (!s) return []; return Promise.all(s.context.pages().map(async (p, index) => ({ index, url: p.url(), title: await p.title().catch(() => ''), managed: p === s.grokPage }))); }
  async startTrace(id) { const s = this.sessions.get(id); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started'); await s.context.tracing.start({ screenshots: true, snapshots: true }); s.isTracing = true; return { profileId: id, tracing: true }; }
  async stopTrace(id, outputPath) { const s = this.sessions.get(id); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started'); await s.context.tracing.stop({ path: outputPath }); s.isTracing = false; return { profileId: id, tracing: false, outputPath }; }
  async disconnect() { await Promise.all([...this.sessions.keys()].map((id) => this.stop(id))); return { success: true }; }
}
module.exports = new SessionManager();
