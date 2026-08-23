const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

class SessionManager {
  constructor() { this.sessions = new Map(); this.startFlights = new Map(); this.jobs = new Map(); }
  profileDir(id) {
    if (!id || !/^[a-z0-9][a-z0-9-]{0,127}$/i.test(id)) throw new Error('INVALID_PROFILE: profileId is required');
    const root = path.resolve(process.env.FLOWORD_PLAYWRIGHT_PROFILE_ROOT || path.join(process.env.LOCALAPPDATA || process.cwd(), 'Floword', 'playwright-profiles'));
    const dir = path.resolve(root, id); if (!dir.startsWith(root + path.sep)) throw new Error('INVALID_PROFILE: unsafe profile path');
    fs.mkdirSync(dir, { recursive: true }); return dir;
  }
  extensionDir(value) {
    const dir = path.resolve(value || process.env.FLOWORD_CHROMEX_EXTENSION_PATH || '');
    if (!dir || !fs.existsSync(path.join(dir, 'manifest.json'))) throw new Error(`EXTENSION_NOT_LOADED: manifest.json not found at ${dir}`);
    return dir;
  }
  async ensureProfile(id, options = {}) {
    if (this.sessions.has(id)) { const s = this.sessions.get(id); return this.describe(s, await this.ensureGrokPage(s, options.url)); }
    if (this.startFlights.has(id)) return this.startFlights.get(id);
    const flight = this.startProfile(id, options).finally(() => this.startFlights.delete(id)); this.startFlights.set(id, flight); return flight;
  }
  async startProfile(id, options) {
    const userDataDir = this.profileDir(id); const extensionPath = this.extensionDir(options.extensionPath);
    const context = await chromium.launchPersistentContext(userDataDir, { channel: 'chromium', headless: false, args: [`--disable-extensions-except=${extensionPath}`, `--load-extension=${extensionPath}`] });
    const s = { profileId: id, userDataDir, extensionPath, context, worker: null, grokPage: null, activeJobId: null, lastHeartbeat: Date.now() }; this.sessions.set(id, s);
    try { s.worker = await this.waitForServiceWorker(context, options.timeoutMs || 15000); return this.describe(s, await this.ensureGrokPage(s, options.url)); }
    catch (e) { await context.close().catch(() => {}); this.sessions.delete(id); throw e; }
  }
  async waitForServiceWorker(context, timeout) {
    const found = context.serviceWorkers().find((w) => w.url().startsWith('chrome-extension://')); if (found) return found;
    return context.waitForEvent('serviceworker', { timeout, predicate: (w) => w.url().startsWith('chrome-extension://') });
  }
  async ensureGrokPage(s, url = 'https://grok.com/imagine') {
    let page = s.context.pages().find((p) => /^https:\/\/(www\.)?grok\.com\//i.test(p.url()));
    if (!page) page = await s.context.newPage(); if (!/^https:\/\/(www\.)?grok\.com\//i.test(page.url())) await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
    s.grokPage = page; s.lastHeartbeat = Date.now(); return page;
  }
  describe(s, page) { return { profileId: s.profileId, userDataDir: s.userDataDir, extensionId: s.worker?.url().match(/^chrome-extension:\/\/([^/]+)/)?.[1] || null, serviceWorkerUrl: s.worker?.url() || null, grokUrl: page?.url() || null, browserOpen: true, activeJobId: s.activeJobId }; }
  async health(id) { const s = this.sessions.get(id); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started'); const result = await s.worker.evaluate((profileId) => globalThis.__flowordProduction?.health(profileId), id); s.lastHeartbeat = Date.now(); return result; }
  async dispatch(request) {
    const { profileId, requestId, jobId } = request; if (!profileId || !requestId || !jobId) throw new Error('INVALID_REQUEST: profileId, requestId and jobId are required');
    if (this.jobs.has(requestId)) return this.jobs.get(requestId); const s = this.sessions.get(profileId); if (!s) throw new Error('PLAYWRIGHT_PROFILE_OFFLINE: profile is not started');
    if (s.activeJobId && s.activeJobId !== jobId) throw new Error('JOB_ALREADY_RUNNING: profile has an active job'); s.activeJobId = jobId;
    const promise = s.worker.evaluate((payload) => globalThis.__flowordProduction?.dispatch(payload), request).finally(() => { if (s.activeJobId === jobId) s.activeJobId = null; this.jobs.delete(requestId); }); this.jobs.set(requestId, promise); return promise;
  }
  async cancel(jobId) { const s = [...this.sessions.values()].find((x) => x.activeJobId === jobId); if (!s) return { cancelled: false, jobId }; s.activeJobId = null; return { cancelled: true, jobId }; }
  async stop(id) { const s = this.sessions.get(id); if (!s) return { stopped: false, profileId: id }; await s.context.close(); this.sessions.delete(id); return { stopped: true, profileId: id }; }
  async getPages(id) { const s = this.sessions.get(id); if (!s) return []; return Promise.all(s.context.pages().map(async (p, index) => ({ index, url: p.url(), title: await p.title().catch(() => '') }))); }
  async disconnect() { await Promise.all([...this.sessions.keys()].map((id) => this.stop(id))); return { success: true }; }
}
module.exports = new SessionManager();
