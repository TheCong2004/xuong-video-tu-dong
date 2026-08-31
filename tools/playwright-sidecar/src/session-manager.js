const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

function validateArtifactLocator(value) {
  let parsed;
  try { parsed = new URL(String(value)); } catch (_) { throw new Error('ARTIFACT_LOCATOR_INVALID: locator must be an absolute URL'); }
  if (parsed.protocol !== 'https:' || !['grok.com', 'assets.grok.com'].includes(parsed.hostname.toLowerCase())) {
    throw new Error('ARTIFACT_LOCATOR_FORBIDDEN: locator origin is not an allowed Grok artifact host');
  }
  return parsed;
}

const MAX_ARTIFACT_BYTES = 16 * 1024 * 1024;

function sha256DataUrl(value) {
  if (typeof value !== 'string') return null;
  const comma = value.indexOf(',');
  if (comma < 0) return null;
  try { return crypto.createHash('sha256').update(Buffer.from(value.slice(comma + 1), 'base64')).digest('hex').toUpperCase(); } catch (_) { return null; }
}

function canonical(value) { if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`; if (value && typeof value === 'object') return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`; return JSON.stringify(value); }

class SessionManager {
  constructor() { this.sessions = new Map(); this.startFlights = new Map(); this.activeRequests = new Map(); this.completedRequests = new Map(); this.terminalRequests = new Map(); this.requestJournals = new Map(); this.requestStateCache = new Map(); this.completedTtlMs = 30 * 60 * 1000; this.maxCompletedRequests = 500; this.cancelSettleTimeoutMs = 15000; this.reconciliationQuarantineMs = 30 * 1000; }
  journalStage(request, stage, details = {}) {
    const entry = { timestamp: new Date().toISOString(), requestId: request.requestId, jobId: request.jobId, stepId: request.stepId, attempt: request.attemptId, stage, submissionState: details.submissionState || 'NOT_SUBMITTED', ...details };
    const list = this.requestJournals.get(request.requestId) || [];
    list.push(entry); this.requestJournals.set(request.requestId, list.slice(-64));
    console.info(`[FlowordJournal] ${JSON.stringify(entry)}`);
    return entry;
  }
  async readRawContentRequestState(session, requestId) {
    if (!session?.cdpSession || !session.requestStateSupported || !requestId) return null;
    try {
      const value = await Promise.race([
        this.cdpEvaluate(session, `globalThis.__flowordProductionContent.requestState(${JSON.stringify(requestId)})`),
        new Promise((resolve) => setTimeout(() => resolve(null), 2000)),
      ]);
      return value && typeof value === 'object' ? value : null;
    } catch (_) { return null; }
  }
  async readContentRequestState(session, requestId) {
    const value = await this.readRawContentRequestState(session, requestId);
    if (value && typeof value === 'object') {
      this.requestStateCache.set(requestId, JSON.parse(JSON.stringify(value)));
      return value;
    }
    const cached = this.requestStateCache.get(requestId);
    return cached ? JSON.parse(JSON.stringify(cached)) : null;
  }
  appendInnerJournal(request, state) {
    const entries = Array.isArray(state?.entries) ? state.entries : [];
    if (!entries.length) return state;
    const seen = new Set((this.requestJournals.get(request.requestId) || []).map((entry) => `${entry.stage}|${entry.timestamp}`));
    for (const entry of entries) {
      const key = `${entry.stage}|${entry.timestamp}`;
      if (!seen.has(key)) { this.journalStage(request, entry.stage, { submissionState: entry.submissionState, baselineCount: entry.baselineCount, newCandidateCount: entry.newCandidateCount, newContainerCount: entry.newContainerCount, newMediaFingerprintCount: entry.newMediaFingerprintCount, baselineMatchCount: entry.baselineMatchCount, totalMediaCount: entry.totalMediaCount, visibleMediaCount: entry.visibleMediaCount, rejectedUnknownCount: entry.rejectedUnknownCount, postIdMismatchCount: entry.postIdMismatchCount, unsupportedMediaTypeCount: entry.unsupportedMediaTypeCount, selectedFingerprint: entry.selectedFingerprint, postIdHash: entry.postIdHash, postIdentityHash: entry.postIdentityHash, proofType: entry.proofType, postContainerFingerprint: entry.postContainerFingerprint, pageIdentityHash: entry.pageIdentityHash, postRootResolution: entry.postRootResolution, spaTransitionObserved: entry.spaTransitionObserved, currentPostContainerCount: entry.currentPostContainerCount, matchedPostContainerCount: entry.matchedPostContainerCount, correlationConflictCount: entry.correlationConflictCount, errorCode: entry.errorCode }); seen.add(key); }
    }
    return state;
  }
  hasPossibleSubmission(state) {
    if (!state || typeof state !== 'object') return false;
    const summary = state.summary && typeof state.summary === 'object' ? state.summary : {};
    if (summary.sideEffectPossible === true || summary.submitIntentObserved === true || summary.submitClickedObserved === true || summary.submitClicked === true || summary.submitAcknowledgedObserved === true || summary.submitAcknowledged === true || summary.postProofObserved === true || summary.postCreated === true || summary.generationProofObserved === true || summary.generationAccepted === true) return true;
    if (state.submissionState === 'UNKNOWN' || state.submissionState === 'SUBMITTED' || state.submissionState === 'COMPLETED') return true;
    return Array.isArray(state.entries) && state.entries.some((entry) => ['UNKNOWN', 'SUBMITTED'].includes(entry?.submissionState) || ['SUBMIT_INTENT', 'SUBMIT_CLICKED', 'SUBMIT_ACKNOWLEDGED', 'POST_CREATED', 'GENERATION_ACCEPTED'].includes(entry?.stage));
  }
  hasSubmittedProof(state) {
    if (!state || typeof state !== 'object') return false;
    const summary = state.summary && typeof state.summary === 'object' ? state.summary : {};
    return state.submissionState === 'SUBMITTED' || state.submissionState === 'COMPLETED' || summary.postProofObserved === true || summary.generationProofObserved === true || summary.submissionState === 'SUBMITTED' || summary.submissionState === 'COMPLETED';
  }
  localJournalState(requestId) {
    const entries = this.requestJournals.get(requestId) || [];
    if (!entries.length) return null;
    const last = entries[entries.length - 1];
    return { submissionState: last.submissionState, stage: last.stage, entries };
  }
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
  validateAndNormalizeIdentity(profileId, options = {}) {
    if (!profileId || !/^[a-z0-9][a-z0-9-]{0,127}$/i.test(profileId)) throw new Error('INVALID_PROFILE: profileId is required');
    const endpoint = String(options.cdpEndpoint || (options.cdpPort ? `http://127.0.0.1:${options.cdpPort}` : ''));
    let parsed;
    try { parsed = new URL(endpoint); } catch (_) { throw new Error('CDP_IDENTITY_REQUIRED: invalid cdpEndpoint'); }
    if (parsed.protocol !== 'http:' || !['127.0.0.1', 'localhost'].includes(parsed.hostname) || parsed.pathname !== '/' || parsed.search || parsed.hash) throw new Error('CDP_IDENTITY_REQUIRED: cdpEndpoint must be loopback http');
    if (!parsed.port) throw new Error('CDP_IDENTITY_REQUIRED: explicit cdpEndpoint port is required');
    const port = Number(parsed.port);
    if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('CDP_IDENTITY_REQUIRED: invalid cdpEndpoint port');
    const browserPid = options.browserPid;
    const launchGeneration = options.launchGeneration;
    if (!Number.isSafeInteger(browserPid) || browserPid <= 0 || !Number.isSafeInteger(launchGeneration) || launchGeneration <= 0) throw new Error('CDP_IDENTITY_REQUIRED: browserPid and launchGeneration are required');
    const normalizedEndpoint = `http://${parsed.hostname}:${port}`;
    return { profileId, cdpEndpoint: normalizedEndpoint, browserPid, launchGeneration, fingerprint: `${profileId}|${normalizedEndpoint}|${browserPid}|${launchGeneration}` };
  }
  async ensureProfile(id, options = {}) {
    const identity = this.validateAndNormalizeIdentity(id, options);
    // A STARTING session is not authoritative yet. Always join the existing
    // attach flight before consulting the published session map.
    const flight = this.startFlights.get(id);
    if (flight) {
      if (flight.fingerprint !== identity.fingerprint) throw new Error('CDP_SESSION_STALE: another attach is already starting for this profile');
      return flight.promise;
    }
    if (this.sessions.has(id)) {
      const s = this.sessions.get(id);
      const sessionFingerprint = `${id}|${s.cdpEndpoint}|${s.browserPid}|${s.launchGeneration}`;
      if (sessionFingerprint !== identity.fingerprint) {
        if (s.activeRequest) throw new Error('CDP_SESSION_STALE: browser endpoint changed while a request was active');
        await s.browser.disconnect?.().catch(() => {});
        this.sessions.delete(id);
      } else {
        return this.describe(s, await this.ensureGrokPage(s, options.url, options.grokTargetId));
      }
    }
    const promise = this.startProfile(id, { ...options, ...identity }).finally(() => {
      const current = this.startFlights.get(id);
      if (current?.promise === promise) this.startFlights.delete(id);
    });
    this.startFlights.set(id, { fingerprint: identity.fingerprint, promise });
    return promise;
  }
  async startProfile(id, options = {}) {
    const identity = this.validateAndNormalizeIdentity(id, options);
    const browser = await chromium.connectOverCDP(identity.cdpEndpoint, { timeout: options.timeoutMs || 15000 });
    const context = browser.contexts()[0];
    if (!context) { throw new Error('CDP_CONTEXT_NOT_FOUND: Donut browser exposed no browser context'); }
    const s = { profileId: id, cdpEndpoint: identity.cdpEndpoint, browserPid: identity.browserPid, launchGeneration: identity.launchGeneration, browserEngine: options.browserEngine || 'WAYFERN', browser, userDataDir: null, extensionPath: null, context, worker: null, cdpSession: null, contentContextId: null, contentFrameId: null, contentOrigin: null, grokPage: null, managedGrokTabId: null, activeRequest: null, state: 'STARTING', isTracing: false, lastHeartbeat: Date.now(), navigationDiagnostics: [] };
    try {
      const page = await this.ensureGrokPage(s, options.url, options.grokTargetId);
      this.attachNavigationDiagnostics(s, page);
      await this.preflightCdpAutomation(page, s);
      s.state = 'BROWSER_READY';
      await this.bindContentContract(s, page, options.timeoutMs || 15000);
      await this.bindProfile(s);
      s.state = 'EXTENSION_READY';
      this.sessions.set(id, s);
      return this.describe(s, page);
    // This browser is owned by Donut.  A failed attach/wake must never close
    // the user's Donut browser or its profile.
    } catch (e) {
      if (String(e?.message || e).startsWith('GROK_PAGE_NAVIGATION_UNSTABLE')) {
        e.details = { ...(e.details || {}), navigation: s.navigationDiagnostics };
      }
      throw e;
    }
  }
  async attachContentCdp(s, page) {
    if (s.cdpSession) return s.cdpSession;
    const cdp = await s.context.newCDPSession(page);
    s.executionContexts = new Map();
    cdp.on('Runtime.executionContextCreated', ({ context }) => s.executionContexts.set(context.id, context));
    await cdp.send('Page.enable');
    await cdp.send('Runtime.enable');
    s.cdpSession = cdp;
    cdp.on('Runtime.executionContextDestroyed', ({ executionContextId }) => {
      if (s.contentContextId === executionContextId) { s.contentContextId = null; s.state = 'RECONCILING'; }
    });
    cdp.on('Runtime.executionContextsCleared', () => { s.contentContextId = null; s.state = 'RECONCILING'; });
    cdp.on('Page.frameNavigated', () => { s.contentContextId = null; if (!s.activeRequest) s.state = 'RECONCILING'; });
    return cdp;
  }
  async findContentContext(s, page, deadline) {
    const cdp = await this.attachContentCdp(s, page);
    const tree = await cdp.send('Page.getFrameTree');
    const mainFrameId = tree?.frameTree?.frame?.id;
    let extensionId = null;
    try { const marker = await this.readBootstrapMarker(page); extensionId = marker ? JSON.parse(marker)?.extensionId || null : null; } catch (_) { extensionId = null; }
    const contexts = s.executionContexts || new Map();
    const onCreated = (event) => contexts.set(event.context.id, event.context);
    cdp.on('Runtime.executionContextCreated', onCreated);
    const probes = [];
    try {
      // Runtime.enable emits existing contexts and future isolated worlds. Probe
      // every candidate: the first isolated context may belong to another
      // content script and must never be treated as the production contract.
      while (Date.now() < deadline) {
        const candidates = [...contexts.values()].filter((context) => {
          const aux = context.auxData || {};
          const origin = String(context.origin || '');
          return aux.frameId === mainFrameId && aux.isDefault === false &&
            (aux.type === 'isolated' || origin.startsWith('chrome-extension://'));
        });
        const matches = [];
        for (const context of candidates) {
          let contract = null;
          let probeError = null;
          try {
            const evaluated = await cdp.send('Runtime.evaluate', {
              contextId: context.id,
              expression: '(() => { const c = globalThis.__flowordProductionContent; return { has: Boolean(c), keys: c ? Object.getOwnPropertyNames(c) : [], bind: typeof c?.bind, health: typeof c?.health, dispatch: typeof c?.dispatch, cancel: typeof c?.cancel, protocol: c?.protocol ?? null, protocolVersion: c?.protocolVersion ?? null }; })()',
              awaitPromise: true,
              returnByValue: true,
            });
            contract = evaluated?.result?.value || null;
          } catch (error) { probeError = String(error?.message || error); }
          probes.push({ contextId: context.id, origin: String(context.origin || ''), name: context.name || '', frameId: context.auxData?.frameId || null, isDefault: context.auxData?.isDefault ?? null, type: context.auxData?.type || null, contract, error: probeError });
          if (contract?.bind === 'function' && contract?.health === 'function' && contract?.dispatch === 'function' && contract?.cancel === 'function') matches.push(context);
        }
        if (matches.length === 1) {
          const context = matches[0];
          const origin = String(context.origin || '');
          s.contentContextId = context.id; s.contentFrameId = mainFrameId; s.contentOrigin = origin;
          return context;
        }
        if (matches.length > 1) {
          const preferred = extensionId ? matches.filter((context) => String(context.origin || '').startsWith(`chrome-extension://${extensionId}`)) : [];
          if (preferred.length === 1) {
            const context = preferred[0];
            s.contentContextId = context.id; s.contentFrameId = mainFrameId; s.contentOrigin = String(context.origin || '');
            return context;
          }
          const error = new Error('EXTENSION_PRODUCTION_CONTEXT_AMBIGUOUS: multiple contexts expose the production contract');
          error.details = { probes };
          throw error;
        }
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
      const error = new Error('EXTENSION_CONTENT_CONTEXT_NOT_FOUND: no Grok context exposed a complete production contract');
      error.details = { probes };
      throw error;
    } finally { cdp.off?.('Runtime.executionContextCreated', onCreated); }
  }
  async cdpEvaluate(s, expression, args = []) {
    if (!s.cdpSession || !s.contentContextId) throw new Error('EXTENSION_CONTENT_CONTEXT_NOT_FOUND: content context is stale');
    const result = await s.cdpSession.send('Runtime.evaluate', { expression, contextId: s.contentContextId, awaitPromise: true, returnByValue: true, userGesture: true });
    if (result?.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || 'EXTENSION_CONTENT_CONTRACT_ERROR');
    return result?.result?.value;
  }
  async bindContentContract(s, page, timeoutMs) {
    const deadline = Date.now() + Math.min(Math.max(Number(timeoutMs) || 15000, 1000), 15000);
    await this.findContentContext(s, page, deadline);
    let contract;
    try { contract = await this.cdpEvaluate(s, '(() => { const c = globalThis.__flowordProductionContent; return { protocol: c?.protocol, protocolVersion: c?.protocolVersion, bind: typeof c?.bind === "function", health: typeof c?.health === "function", dispatch: typeof c?.dispatch === "function", cancel: typeof c?.cancel === "function", requestState: typeof c?.requestState === "function" }; })()'); }
    catch (error) {
      const wrapped = new Error(`EXTENSION_PRODUCTION_CONTENT_CONTRACT_MISSING: ${error.message}`);
      wrapped.details = {
        ...(error?.details || {}),
        bootstrapMarker: await this.readBootstrapMarker(page),
        browserPid: s.browserPid,
        launchGeneration: s.launchGeneration,
        grokPageUrl: this.navigationUrl(page?.url?.()),
      };
      throw wrapped;
    }
    if (contract?.protocol !== 'floword-production' || contract.protocolVersion !== 1 || !contract.bind || !contract.health || !contract.dispatch || !contract.cancel) throw new Error('EXTENSION_PRODUCTION_CONTENT_CONTRACT_MISSING: required bind/health/dispatch/cancel functions are missing');
    s.requestStateSupported = contract.requestState === true;
    s.state = 'BROWSER_READY';
  }
  async preflightCdpAutomation(page, session) {
    try {
      const value = await page.evaluate(() => 1 + 1);
      if (value !== 2) throw new Error(`Runtime.evaluate returned ${String(value)}`);
    } catch (cause) {
      const message = String(cause?.message || cause);
      const error = new Error(`CDP_AUTOMATION_UNAVAILABLE: Runtime.evaluate is unavailable (${message})`);
      error.details = { browserEngine: session?.browserEngine || 'WAYFERN', cdpEndpoint: session?.cdpEndpoint, browserPid: session?.browserPid, launchGeneration: session?.launchGeneration };
      throw error;
    }
  }
  navigationUrl(value) {
    try { const parsed = new URL(String(value)); return `${parsed.origin}${parsed.pathname}`; } catch (_) { return null; }
  }
  isGrokAuthUrl(value) {
    try {
      const url = new URL(String(value));
      const host = url.hostname.toLowerCase();
      const pathName = url.pathname.toLowerCase();
      return host === 'accounts.google.com' ||
        (host === 'grok.com' && pathName.startsWith('/login')) ||
        (host === 'x.com' && pathName.startsWith('/i/flow/login')) ||
        (host === 'twitter.com' && pathName.startsWith('/i/flow/login'));
    } catch (_) { return false; }
  }
  grokAuthRequired(s, page, currentUrl) {
    s.state = 'AUTH_REQUIRED';
    const error = new Error('GROK_AUTH_REQUIRED: Sign in to Grok in the Donut browser window');
    error.details = { profileId: s.profileId, grokTargetId: s.managedGrokTabId || null, currentUrl: this.navigationUrl(currentUrl), browserPid: s.browserPid, launchGeneration: s.launchGeneration, retryable: true, requiresUserAction: true };
    return error;
  }
  recordNavigationEvent(s, event, page, extra = {}) {
    if (!s?.navigationDiagnostics) return;
    const item = { timestamp: new Date().toISOString(), event, url: this.navigationUrl(page?.url?.()), pageUrl: this.navigationUrl(page?.url?.()), browserPid: s.browserPid, launchGeneration: s.launchGeneration, ...extra };
    s.navigationDiagnostics.push(item);
    if (s.navigationDiagnostics.length > 30) s.navigationDiagnostics.splice(0, s.navigationDiagnostics.length - 30);
  }
  attachNavigationDiagnostics(s, page) {
    if (!page || page.__flowordNavigationDiagnosticsAttached) return;
    page.__flowordNavigationDiagnosticsAttached = true;
    const mainFrame = page.mainFrame?.();
    page.on?.('framenavigated', (frame) => { if (!mainFrame || frame === mainFrame) this.recordNavigationEvent(s, 'framenavigated', page); });
    page.on?.('request', (request) => { if (request.resourceType?.() === 'document') this.recordNavigationEvent(s, 'request:document', page, { url: this.navigationUrl(request.url?.()) }); });
    page.on?.('response', (response) => {
      const request = response.request?.();
      if (request?.resourceType?.() !== 'document') return;
      const redirect = request.redirectedFrom?.();
      this.recordNavigationEvent(s, 'response:document', page, { status: response.status?.(), redirectSource: this.navigationUrl(redirect?.url?.()) });
    });
    page.on?.('requestfailed', (request) => { if (request.resourceType?.() === 'document') this.recordNavigationEvent(s, 'requestfailed:document', page, { failure: request.failure?.()?.errorText || null, url: this.navigationUrl(request.url?.()) }); });
    page.on?.('crash', () => this.recordNavigationEvent(s, 'crash', page));
    page.on?.('close', () => this.recordNavigationEvent(s, 'close', page));
  }
  async waitForServiceWorker() { throw new Error('EXTENSION_CONTENT_CONTEXT_NOT_FOUND: service-worker readiness is not used'); }
  isTransientNavigationError(error) {
    const message = String(error?.message || error);
    return message.includes('Execution context was destroyed') || message.includes('Cannot find context with specified id');
  }
  async waitForStableGrokPage(page, deadline) {
    if (!page || page.isClosed?.()) throw new Error('GROK_PAGE_NAVIGATION_UNSTABLE: Grok page is closed');
    let url;
    try { url = page.url(); } catch (error) { if (this.isTransientNavigationError(error)) throw error; throw error; }
    if (!/^https:\/\/(www\.)?grok\.com\//i.test(url)) throw new Error('GROK_PAGE_NAVIGATION_UNSTABLE: managed page left grok.com');
    const remaining = Math.max(1, deadline - Date.now());
    try {
      await page.waitForLoadState('domcontentloaded', { timeout: Math.min(remaining, 2000) });
    } catch (error) {
      if (!this.isTransientNavigationError(error) && Date.now() < deadline) throw error;
      throw error;
    }
    if (page.isClosed?.()) throw new Error('GROK_PAGE_NAVIGATION_UNSTABLE: Grok page is closed');
    return page;
  }
  async pollFlowordWorker() { return null; }
  async readBootstrapMarker(page) {
    if (!page || page.isClosed?.()) return null;
    try {
      return await page.evaluate(() => document.documentElement?.getAttribute('data-floword-production-bootstrap') || null);
    } catch (_) { return null; }
  }
  async wakeServiceWorker() { throw new Error('EXTENSION_CONTENT_CONTEXT_NOT_FOUND: service-worker wake is not used'); }
  async targetIdForPage(s, page) {
    if (!s?.context?.newCDPSession) return null;
    const cdp = await s.context.newCDPSession(page);
    try {
      const info = await cdp.send('Target.getTargetInfo');
      return info?.targetInfo?.targetId || null;
    } finally { await cdp.detach?.().catch(() => {}); }
  }
  async ensureGrokPage(s, url = 'https://grok.com/imagine', requestedTargetId = null) {
    let page = null;
    const pages = s.context.pages();
    if (requestedTargetId) {
      for (const candidate of pages) {
        if (candidate.isClosed?.()) continue;
        try {
          if (await this.targetIdForPage(s, candidate) === requestedTargetId) { page = candidate; break; }
        } catch (_) { /* target may have closed between enumeration and attach */ }
      }
      if (!page) {
        const error = new Error('GROK_MANAGED_TARGET_STALE: Donut managed target is not present in CDP target list');
        error.details = { requestedTargetId, grokCandidateCount: pages.filter((candidate) => /^https:\/\/(www\.)?grok\.com\//i.test(candidate.url())).length, browserPid: s.browserPid, launchGeneration: s.launchGeneration };
        throw error;
      }
    } else {
      page = s.grokPage && !s.grokPage.isClosed() ? s.grokPage : null;
      const grokPages = pages.filter((candidate) => /^https:\/\/(www\.)?grok\.com\//i.test(candidate.url()));
      if (!page && grokPages.length > 1) throw new Error('AMBIGUOUS_GROK_TAB: multiple Grok tabs exist without a managed mapping');
      if (!page) page = grokPages[0];
    }
    if (!page) {
      const authPage = s.context.pages().find((candidate) => this.isGrokAuthUrl(candidate.url()));
      if (authPage) throw this.grokAuthRequired(s, authPage, authPage.url());
      throw new Error('GROK_TAB_NOT_FOUND: Donut did not expose a managed Grok tab');
    }
    if (this.isGrokAuthUrl(page.url())) throw this.grokAuthRequired(s, page, page.url());
    if (!/^https:\/\/(www\.)?grok\.com\//i.test(page.url())) throw new Error('GROK_MANAGED_TARGET_STALE: Donut managed target is not a Grok page');
    await page.bringToFront().catch(() => {}); await page.waitForLoadState('domcontentloaded').catch(() => {});
    s.grokPage = page;
    s.managedGrokTabId = requestedTargetId || await this.targetIdForPage(s, page);
    s.lastHeartbeat = Date.now();
    return page;
  }
  async bindProfile(s) {
    const identity = JSON.stringify({ profileId: s.profileId, browserPid: s.browserPid, launchGeneration: s.launchGeneration, grokTargetId: s.managedGrokTabId });
    const result = await this.cdpEvaluate(s, `globalThis.__flowordProductionContent.bind(${identity})`);
    if (!result?.ok) throw new Error(`${result?.error?.code || 'CONTENT_SCRIPT_BIND_TIMEOUT'}: ${result?.error?.message || 'Profile binding failed'}`);
    return result;
  }
  timeoutForRequest(request) {
    const requested = Number(request?.params?.timeoutMs);
    const fallback = request?.method === 'grok.video.generate' ? 300000 : 180000;
    if (!Number.isFinite(requested)) return fallback;
    return Math.min(Math.max(Math.trunc(requested), 5000), 900000);
  }
  async ensureContentContract(s) {
    if (!s.cdpSession || !s.contentContextId) {
      await this.bindContentContract(s, s.grokPage, 15000);
      await this.bindProfile(s);
    }
    return s.cdpSession;
  }
  async ensureWorker(s) { return this.ensureContentContract(s); }
  describe(s, page) { return { profileId: s.profileId, userDataDir: s.userDataDir, extensionId: s.contentOrigin?.match(/^chrome-extension:\/\/([^/]+)/)?.[1] || null, serviceWorkerUrl: null, grokTargetId: s.managedGrokTabId || null, grokPageUrl: this.navigationUrl(page?.url?.()), grokUrl: page?.url() || null, browserOpen: true, state: s.state, activeRequest: s.activeRequest }; }
  async health(id) {
    const s = this.sessions.get(id); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started');
    const managedPage = s.grokPage && !s.grokPage.isClosed?.() ? s.grokPage : s.context.pages().find((candidate) => this.isGrokAuthUrl(candidate.url()));
    if (managedPage && this.isGrokAuthUrl(managedPage.url())) throw this.grokAuthRequired(s, managedPage, managedPage.url());
    await this.ensureContentContract(s); const result = await this.cdpEvaluate(s, 'globalThis.__flowordProductionContent.health()');
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
    try { await this.ensureContentContract(s); } catch (error) {
      this.journalStage(active, 'DISPATCH_FAILED', { submissionState: 'NOT_SUBMITTED', errorCode: String(error?.message || error).split(':')[0], retryable: false });
      throw error;
    }
    const cancelRequest = { protocol: 'floword-production', protocolVersion: 1, requestId: `CANCEL_${crypto.randomUUID()}`, jobId: active.jobId, stepId: active.stepId, attemptId: active.attemptId, leaseId: active.leaseId, profileId: active.profileId, method: 'production.task.cancel', params: { targetRequestId: active.requestId }, createdAt: new Date().toISOString() };
    const cancelRequestId = cancelRequest.requestId;
    const cancelPromise = this.cdpEvaluate(s, `globalThis.__flowordProductionContent.cancel(${JSON.stringify(cancelRequest)})`);
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
  postSubmitErrorCode(error) {
    const code = String(error?.code || error?.message || error || '').split(':')[0];
    return ['POST_ROOT_AMBIGUOUS', 'CURRENT_DOM_NOT_CORRELATED', 'POST_CORRELATION_CONFLICT', 'GROK_RESULT_AMBIGUOUS', 'GROK_GENERATION_TIMEOUT', 'RESULT_SCANNER_ZERO_CANDIDATES', 'RESULT_HASH_UNVERIFIED', 'SOURCE_ARTIFACT_ECHO'].includes(code);
  }
  errorCode(error) { return String(error?.code || error?.message || error || '').split(':')[0]; }
  decoratePostSubmitError(error, request, recoveredState, errorCode) {
    const details = {
      ...(error?.details || {}),
      requestId: request.requestId,
      jobId: request.jobId,
      stepId: request.stepId,
      attemptId: request.attemptId,
      leaseId: request.leaseId,
      profileId: request.profileId,
      submissionState: 'SUBMITTED',
      resultState: 'UNKNOWN',
      resolutionState: 'RECONCILING',
      terminal: false,
      retryable: false,
      ownershipHeld: true,
      errorCode,
      lastStage: recoveredState?.stage || null,
    };
    error.details = details;
    return error;
  }
  scheduleOrphanReconciliation(s, requestId, dispatchPromise, request, fingerprint) {
    const entry = this.activeRequests.get(requestId);
    if (!entry || entry.session !== s || entry.dispatchPromise !== dispatchPromise) return;
    clearTimeout(entry.reconciliationTimer);
    const quarantineMs = Math.max(10, Number(this.reconciliationQuarantineMs) || 30000);
    entry.reconciliationTimer = setTimeout(() => this.orphanReconciliation(s, requestId, dispatchPromise, request, fingerprint), quarantineMs);
    entry.reconciliationTimer.unref?.();
  }
  async dispatch(request) {
    const { profileId, requestId, jobId } = request; if (!profileId || !requestId || !jobId) throw new Error('INVALID_REQUEST: profileId, requestId and jobId are required');
    this.journalStage(request, 'DISPATCH_RECEIVED');
    this.pruneCompleted(); const fingerprint = crypto.createHash('sha256').update(canonical({ requestId, jobId, stepId: request.stepId, attemptId: request.attemptId, leaseId: request.leaseId, profileId, method: request.method, params: request.params })).digest('hex');
    const active = this.activeRequests.get(requestId); if (active) { if (active.fingerprint !== fingerprint) throw new Error('CORRELATION_CONFLICT: requestId was reused with different payload'); this.journalStage(request, 'DISPATCH_DUPLICATE', { submissionState: 'UNKNOWN', retryable: false }); return active.promise; }
    const completed = this.completedRequests.get(requestId); if (completed) { if (completed.fingerprint !== fingerprint) throw new Error('CORRELATION_CONFLICT: requestId was reused with different payload'); this.journalStage(request, 'DISPATCH_DUPLICATE', { submissionState: 'COMPLETED', retryable: false }); return completed.result; }
    const terminal = this.terminalRequests.get(requestId); if (terminal) { if (terminal.fingerprint !== fingerprint) throw new Error('CORRELATION_CONFLICT: requestId was reused with different payload'); this.journalStage(request, 'DISPATCH_DUPLICATE', { submissionState: terminal.submissionState || 'SUBMITTED', retryable: false, resolutionState: terminal.resolutionState }); throw new Error(`DUPLICATE_REQUEST: requestId is terminal (${terminal.resolutionState || 'ORPHANED'}) and cannot be dispatched again`); }
    const s = this.sessions.get(profileId); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started');
    const currentPage = s.grokPage && !s.grokPage.isClosed?.() ? s.grokPage : null;
    if (s.state === 'AUTH_REQUIRED' || (currentPage && this.isGrokAuthUrl(currentPage.url()))) throw this.grokAuthRequired(s, currentPage, currentPage?.url() || '');
    try { await this.ensureContentContract(s); } catch (error) {
      this.journalStage(request, 'DISPATCH_FAILED', { submissionState: 'NOT_SUBMITTED', errorCode: String(error?.message || error).split(':')[0], retryable: false });
      throw error;
    }
    if (s.state === 'RECONCILING') throw new Error('WORKER_RECONCILING: profile is recovering after an unconfirmed cancellation');
    if (s.activeRequest) throw new Error('JOB_ALREADY_RUNNING: profile has an active request');
    const activeRequest = { ...request };
    s.activeRequest = activeRequest; s.state = 'BUSY';
    const timeoutMs = this.timeoutForRequest(request);
    let timeoutHandle;
    const dispatchPromise = this.cdpEvaluate(s, `globalThis.__flowordProductionContent.dispatch(${JSON.stringify(request)})`);
    const timeoutPromise = new Promise((_, reject) => { timeoutHandle = setTimeout(() => reject(new Error('RESULT_TIMEOUT: dispatch timed out')), timeoutMs); });
    // Keep a last valid sticky state while the content dispatch is running.
    // A later CDP read may transiently return null during navigation or worker
    // churn; that must not erase submit evidence.
    const stateObserver = setInterval(() => { this.readRawContentRequestState(s, requestId).then((state) => { if (state && typeof state === 'object') this.requestStateCache.set(requestId, JSON.parse(JSON.stringify(state))); }).catch(() => {}); }, 500);
    stateObserver.unref?.();
    let retainOwnership = false;
    let enteredReconciliation = false;
    let dispatchSettled = false;
    const promise = (async () => {
      try {
        const result = await Promise.race([dispatchPromise, timeoutPromise]);
        dispatchSettled = true;
        if (!result || typeof result !== 'object') throw new Error('INVALID_EXTENSION_RESPONSE: dispatch returned no object');
        if (result.protocol !== 'floword-production' || result.protocolVersion !== 1 || result.requestId !== request.requestId || result.jobId !== request.jobId || result.stepId !== request.stepId || result.attemptId !== request.attemptId || result.leaseId !== request.leaseId || result.profileId !== request.profileId) throw new Error('CORRELATION_MISMATCH: extension response did not echo request identity');
        if (result.ok !== true) {
          const failure = result.error || {};
          const code = String(failure.code || result.code || 'EXTENSION_DISPATCH_FAILED');
          const error = new Error(`${code}: ${failure.message || result.message || 'Extension reported dispatch failure'}`);
          error.code = code;
          error.details = { ...(failure.details || result.details || {}), ...(failure.retryable === undefined ? {} : { retryable: failure.retryable }) };
          throw error;
        }
        if (request.method === 'grok.image.edit' && request.params?.sourceArtifact?.dataUrl) {
          const sourceSha256 = request.params.sourceArtifact.sha256 || sha256DataUrl(request.params.sourceArtifact.dataUrl);
          const contentSha256 = result.result?.contentSha256;
          if (!sourceSha256 || typeof contentSha256 !== 'string') throw new Error('RESULT_HASH_UNVERIFIED: generated result did not include content SHA-256');
          if (sourceSha256.toLowerCase() === contentSha256.toLowerCase()) {
            this.journalStage(request, 'SOURCE_ARTIFACT_ECHO', { submissionState: 'SUBMITTED', sourceEchoCount: 1, resultCandidateCount: 0, rejectionReason: 'SOURCE_ARTIFACT_ECHO', errorCode: 'SOURCE_ARTIFACT_ECHO', retryable: false });
            throw new Error('SOURCE_ARTIFACT_ECHO: generated artifact matches input source');
          }
        }
        const requestState = await this.readContentRequestState(s, requestId);
        this.appendInnerJournal(request, requestState);
        this.completedRequests.set(requestId, { fingerprint, result, expiresAt: Date.now() + this.completedTtlMs });
        this.journalStage(request, 'DISPATCH_COMPLETED', { submissionState: 'COMPLETED' });
        while (this.completedRequests.size > this.maxCompletedRequests) this.completedRequests.delete(this.completedRequests.keys().next().value);
        return result;
      } catch (error) {
          const requestState = await this.readContentRequestState(s, requestId);
          this.appendInnerJournal(request, requestState);
          const recoveredState = requestState || this.localJournalState(requestId);
          const rootErrorCode = this.errorCode(error);
          const postSubmitEvidence = this.hasPossibleSubmission(recoveredState) || this.hasPossibleSubmission(error?.details) || this.postSubmitErrorCode(error);
          // Preserve the established timeout envelope (DISPATCH_RECONCILING)
          // below; content/root failures must retain their original code.
          if (postSubmitEvidence && rootErrorCode !== 'RESULT_TIMEOUT' && !String(error?.message || '').startsWith('TASK_CANCELLED')) {
            retainOwnership = true;
            enteredReconciliation = true;
            s.state = 'RECONCILING';
            const retainedState = recoveredState && typeof recoveredState === 'object'
              ? { ...JSON.parse(JSON.stringify(recoveredState)), submissionState: 'SUBMITTED', summary: { ...(recoveredState.summary || {}), postProofObserved: true } }
              : { submissionState: 'SUBMITTED', stage: rootErrorCode, summary: { postProofObserved: true } };
            this.requestStateCache.set(requestId, retainedState);
            this.journalStage(request, 'DISPATCH_RECONCILING', {
              submissionState: 'SUBMITTED', resultState: 'UNKNOWN', resolutionState: 'RECONCILING',
              terminal: false, retryable: false, ownershipHeld: true, errorCode: rootErrorCode,
              reason: 'POST_SUBMIT_RECONCILIATION_REQUIRED', lastStage: recoveredState?.stage || null,
              baselineCount: recoveredState?.entries?.at?.(-1)?.baselineCount,
              newCandidateCount: recoveredState?.entries?.at?.(-1)?.newCandidateCount,
              newContainerCount: recoveredState?.entries?.at?.(-1)?.newContainerCount,
            });
            const decorated = this.decoratePostSubmitError(error, request, recoveredState, rootErrorCode);
            if (dispatchSettled) this.scheduleOrphanReconciliation(s, requestId, dispatchPromise, request, fingerprint);
            else this.watchLateSettlement(s, requestId, dispatchPromise, request, fingerprint);
            throw decorated;
          }
          if (String(error?.message || error).startsWith('RESULT_TIMEOUT:')) {
          const stateAvailable = !!requestState && typeof requestState === 'object';
          const sideEffectPossible = this.hasPossibleSubmission(recoveredState);
          // State unavailable is UNKNOWN, not NOT_SUBMITTED. Never invoke the
          // Grok cancellation contract when the authoritative state cannot be
          // be read; quarantine and reconcile instead.
          if (sideEffectPossible || !stateAvailable) {
            retainOwnership = true;
            enteredReconciliation = true;
            s.state = 'RECONCILING';
            // Preserve the best recovered sticky state for the quarantine
            // reaper.  A later content-context read may be unavailable, but
            // post-submit evidence must still resolve to ORPHANED rather than
            // being downgraded to an unproven cancellation/UNKNOWN state.
            if (recoveredState && typeof recoveredState === 'object') {
              this.requestStateCache.set(requestId, JSON.parse(JSON.stringify(recoveredState)));
            }
            const generationAccepted = recoveredState?.summary?.generationAccepted === true
              || recoveredState?.submissionState === 'SUBMITTED'
              || recoveredState?.entries?.some((entry) => entry?.submissionState === 'SUBMITTED' && entry?.stage === 'GENERATION_ACCEPTED');
            this.journalStage(request, 'DISPATCH_RECONCILING', {
              submissionState: generationAccepted ? 'SUBMITTED' : 'UNKNOWN',
              resultState: 'WAITING',
              resolutionState: 'RECONCILING',
              terminal: false,
              retryable: false,
              errorCode: 'RESULT_TIMEOUT',
              reason: stateAvailable ? 'RESULT_TIMEOUT_AFTER_POSSIBLE_SUBMIT' : 'RESULT_TIMEOUT_STATE_UNAVAILABLE',
              lastStage: recoveredState?.stage || 'RESULT_SCAN',
              baselineCount: recoveredState?.entries?.at?.(-1)?.baselineCount,
              newCandidateCount: recoveredState?.entries?.at?.(-1)?.newCandidateCount,
              newContainerCount: recoveredState?.entries?.at?.(-1)?.newContainerCount,
            });
            this.watchLateSettlement(s, requestId, dispatchPromise, request, fingerprint);
            throw new Error('DISPATCH_RECONCILING: result wait timed out after submission');
          }
          try {
            await this.cancelRequest(s, activeRequest, 10000);
            const settled = await Promise.race([dispatchPromise.then(() => true).catch(() => true), new Promise((resolve) => setTimeout(() => resolve(false), this.cancelSettleTimeoutMs))]);
            if (!settled) { retainOwnership = true; s.state = 'RECONCILING'; this.watchLateSettlement(s, requestId, dispatchPromise); throw new Error('CANCEL_UNCONFIRMED: dispatch did not settle after cancellation'); }
            s.state = 'RECONCILING';
          } catch (cancelError) {
            // An incomplete/invalid cancellation acknowledgement is not proof
            // that the dispatch stopped. Keep ownership quarantined until the
            // original promise settles or the orphan reaper takes it terminal.
            retainOwnership = true;
            enteredReconciliation = true;
            s.state = 'RECONCILING';
            this.journalStage(request, 'DISPATCH_RECONCILING', {
              submissionState: 'UNKNOWN',
              resultState: 'WAITING',
              resolutionState: 'RECONCILING',
              terminal: false,
              retryable: false,
              errorCode: 'CANCEL_UNCONFIRMED',
              reason: 'CANCEL_ACK_UNCONFIRMED',
            });
            this.watchLateSettlement(s, requestId, dispatchPromise, request, fingerprint);
            throw new Error(`CANCEL_UNCONFIRMED: ${cancelError.message}`);
          }
        }
        if (!enteredReconciliation) this.journalStage(request, 'DISPATCH_FAILED', { submissionState: requestState?.submissionState || (String(error?.message || '').includes('SUBMIT') ? 'NOT_SUBMITTED' : 'UNKNOWN'), errorCode: requestState?.errorCode || String(error?.message || '').split(':')[0], retryable: requestState?.retryable ?? false, lastStage: requestState?.stage || null, baselineCount: requestState?.entries?.at?.(-1)?.baselineCount, newCandidateCount: requestState?.entries?.at?.(-1)?.newCandidateCount, newContainerCount: requestState?.entries?.at?.(-1)?.newContainerCount });
        throw error;
      } finally {
        clearTimeout(timeoutHandle);
        clearInterval(stateObserver);
        if (!retainOwnership) {
          if (s.activeRequest?.requestId === requestId) s.activeRequest = null;
          if (s.state === 'BUSY') s.state = 'RECONCILING';
          this.activeRequests.delete(requestId);
        }
      }
    })();
    this.activeRequests.set(requestId, { fingerprint, promise, dispatchPromise, session: s, request: activeRequest }); return promise;
  }
  watchLateSettlement(s, requestId, dispatchPromise, request = null, fingerprint = null) {
    const entry = this.activeRequests.get(requestId);
    if (!entry || entry.session !== s || entry.dispatchPromise !== dispatchPromise) return;
    const quarantineMs = Math.max(10, Number(this.reconciliationQuarantineMs) || 30000);
    entry.reconciliationTimer = setTimeout(() => this.orphanReconciliation(s, requestId, dispatchPromise, request, fingerprint), quarantineMs);
    entry.reconciliationTimer.unref?.();
    dispatchPromise.then((result) => this.reapLateSettlement(s, requestId, dispatchPromise, result, request, fingerprint), (error) => this.reapLateSettlement(s, requestId, dispatchPromise, null, request, fingerprint, error));
  }
  reapLateSettlement(s, requestId, dispatchPromise, result = null, request = null, fingerprint = null, error = null) {
    const entry = this.activeRequests.get(requestId);
    if (!entry || entry.session !== s || entry.dispatchPromise !== dispatchPromise) return;
    clearTimeout(entry.reconciliationTimer);
    if (result && request && this.isCorrelatedDispatchResult(result, request)) {
      this.completedRequests.set(requestId, { fingerprint: fingerprint || entry.fingerprint, result, expiresAt: Date.now() + this.completedTtlMs });
      this.journalStage(request, 'DISPATCH_COMPLETED', { submissionState: 'COMPLETED', resolutionState: 'COMPLETED', lateSettlement: true });
    } else if (request) {
      this.orphanReconciliation(s, requestId, dispatchPromise, request, fingerprint, error || new Error('late dispatch settlement did not produce a correlated result'));
      return;
    }
    this.activeRequests.delete(requestId);
    if (s.activeRequest?.requestId === requestId) s.activeRequest = null;
    s.state = 'RECONCILING';
  }
  isCorrelatedDispatchResult(result, request) {
    return result?.protocol === 'floword-production' && result?.protocolVersion === 1 &&
      result?.requestId === request.requestId && result?.jobId === request.jobId &&
      result?.stepId === request.stepId && result?.attemptId === request.attemptId &&
      result?.leaseId === request.leaseId && result?.profileId === request.profileId && result?.ok === true;
  }
  orphanReconciliation(s, requestId, dispatchPromise, request = null, fingerprint = null, error = null) {
    const entry = this.activeRequests.get(requestId);
    if (!entry || entry.session !== s || entry.dispatchPromise !== dispatchPromise) return;
    clearTimeout(entry.reconciliationTimer);
    this.activeRequests.delete(requestId);
    if (s.activeRequest?.requestId === requestId) s.activeRequest = null;
    s.state = 'RECONCILING';
    if (request) {
      const recoveredState = this.requestStateCache.get(requestId);
      const submissionState = this.hasSubmittedProof(recoveredState) ? 'SUBMITTED' : 'UNKNOWN';
      this.terminalRequests.set(requestId, { fingerprint: fingerprint || entry.fingerprint, submissionState, resolutionState: 'ORPHANED', expiresAt: Date.now() + this.completedTtlMs });
      this.journalStage(request, 'DISPATCH_ORPHANED', { submissionState, resultState: 'UNKNOWN', resolutionState: 'ORPHANED', terminal: true, retryable: false, reason: 'RESULT_RECONCILE_EXHAUSTED', artifactPersisted: false, ownershipHeld: false, leaseAlreadyReleased: false, duplicateDispatchAllowed: false, errorCode: error ? String(error.message || error).split(':')[0] : undefined });
    }
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
  pruneCompleted() { const now = Date.now(); for (const [id, entry] of this.completedRequests) if (entry.expiresAt <= now) this.completedRequests.delete(id); for (const [id, entry] of this.terminalRequests) if (entry.expiresAt <= now) this.terminalRequests.delete(id); }
  async stop(id) { const s = this.sessions.get(id); if (!s) return { stopped: false, profileId: id }; await s.browser.disconnect?.().catch(() => {}); this.sessions.delete(id); return { stopped: true, profileId: id, browserOwnedBy: 'donut' }; }
  async getPages(id) { const s = this.sessions.get(id); if (!s) return []; return Promise.all(s.context.pages().map(async (p, index) => ({ index, url: p.url(), title: await p.title().catch(() => ''), managed: p === s.grokPage }))); }
  async fetchArtifact(id, locator) {
    const s = this.sessions.get(id);
    if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started');
    let parsed = validateArtifactLocator(locator);
    for (let redirect = 0; redirect <= 3; redirect += 1) {
      const response = await s.context.request.get(parsed.toString(), { timeout: 30000, maxRedirects: 0 });
      if (response.status() >= 300 && response.status() < 400) {
        const location = response.headers()?.location;
        await response.dispose?.().catch(() => {});
        if (!location || redirect === 3) throw new Error('ARTIFACT_FETCH_FORBIDDEN: redirect chain exceeded policy');
        try { parsed = validateArtifactLocator(new URL(location, parsed).toString()); } catch (_) { throw new Error('ARTIFACT_FETCH_FORBIDDEN: redirect left allowed Grok origin'); }
        continue;
      }
      if (!response.ok()) {
        const error = new Error(`ARTIFACT_FETCH_FORBIDDEN: browser-context fetch returned HTTP ${response.status()}`);
        error.details = { profileId: id, host: parsed.hostname, status: response.status() };
        await response.dispose?.().catch(() => {});
        throw error;
      }
      const headers = response.headers() || {};
      const contentLength = Number(headers['content-length']);
      if (Number.isFinite(contentLength) && contentLength > MAX_ARTIFACT_BYTES) { await response.dispose?.().catch(() => {}); throw new Error('ARTIFACT_FETCH_TOO_LARGE: artifact exceeds size limit'); }
      const mimeType = String(headers['content-type'] || '').split(';')[0].toLowerCase();
      if (!mimeType.startsWith('image/')) { await response.dispose?.().catch(() => {}); throw new Error('ARTIFACT_FETCH_INVALID_MIME: upstream is not an image'); }
      const body = await response.body();
      await response.dispose?.().catch(() => {});
      if (!body?.length) throw new Error('ARTIFACT_FETCH_EMPTY: browser-context fetch returned no bytes');
      if (body.length > MAX_ARTIFACT_BYTES) throw new Error('ARTIFACT_FETCH_TOO_LARGE: artifact exceeds size limit');
      return { profileId: id, mimeType, dataBase64: body.toString('base64'), sizeBytes: body.length };
    }
    throw new Error('ARTIFACT_FETCH_FORBIDDEN: redirect policy failed');
  }
  async startTrace(id) { const s = this.sessions.get(id); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started'); await s.context.tracing.start({ screenshots: true, snapshots: true }); s.isTracing = true; return { profileId: id, tracing: true }; }
  async stopTrace(id, outputPath) { const s = this.sessions.get(id); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started'); await s.context.tracing.stop({ path: outputPath }); s.isTracing = false; return { profileId: id, tracing: false, outputPath }; }
  async disconnect() { await Promise.all([...this.sessions.keys()].map((id) => this.stop(id))); return { success: true }; }
}
module.exports = new SessionManager();
