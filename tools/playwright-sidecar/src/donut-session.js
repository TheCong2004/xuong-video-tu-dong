const { URL } = require('node:url');

function normalizeEndpoint(value) {
  let parsed;
  try { parsed = new URL(String(value)); } catch (_) { throw new Error('CDP_IDENTITY_REQUIRED: invalid cdpEndpoint'); }
  if (parsed.protocol !== 'http:' || !['127.0.0.1', 'localhost'].includes(parsed.hostname) || !parsed.port || parsed.pathname !== '/' || parsed.search || parsed.hash) {
    throw new Error('CDP_IDENTITY_REQUIRED: cdpEndpoint must be loopback http');
  }
  return `http://${parsed.hostname}:${parsed.port}`;
}

function withoutHumanize(options) {
  if (!options) return undefined;
  const { humanize: _humanize, ...playwrightOptions } = options;
  return playwrightOptions;
}

class DonutLocator {
  constructor(locator) { this.raw = locator; }
  locator(selector) { return new DonutLocator(this.raw.locator(selector)); }
  getByRole(role, options) { return new DonutLocator(this.raw.getByRole(role, options)); }
  getByText(text, options) { return new DonutLocator(this.raw.getByText(text, options)); }
  getByLabel(text, options) { return new DonutLocator(this.raw.getByLabel(text, options)); }
  getByPlaceholder(text, options) { return new DonutLocator(this.raw.getByPlaceholder(text, options)); }
  first() { return new DonutLocator(this.raw.first()); }
  last() { return new DonutLocator(this.raw.last()); }
  nth(index) { return new DonutLocator(this.raw.nth(index)); }
  filter(options) { return new DonutLocator(this.raw.filter(options)); }
  click(options) { return this.raw.click(withoutHumanize(options)); }
  fill(value, options) { return this.raw.fill(value, withoutHumanize(options)); }
  press(key, options) { return this.raw.press(key, withoutHumanize(options)); }
  pressSequentially(text, options) { return this.raw.pressSequentially(text, withoutHumanize(options)); }
  hover(options) { return this.raw.hover(withoutHumanize(options)); }
  check(options) { return this.raw.check(withoutHumanize(options)); }
  uncheck(options) { return this.raw.uncheck(withoutHumanize(options)); }
  setInputFiles(files, options) { return this.raw.setInputFiles(files, withoutHumanize(options)); }
  textContent(options) { return this.raw.textContent(options); }
  innerText(options) { return this.raw.innerText(options); }
  getAttribute(name, options) { return this.raw.getAttribute(name, options); }
  inputValue(options) { return this.raw.inputValue(options); }
  isVisible(options) { return this.raw.isVisible(options); }
  isEnabled(options) { return this.raw.isEnabled(options); }
  count() { return this.raw.count(); }
  waitFor(options) { return this.raw.waitFor(options); }
  boundingBox() { return this.raw.boundingBox(); }
  screenshot(options) { return this.raw.screenshot(options); }
}

class DonutPage {
  constructor(page) { this.raw = page; }
  get keyboard() { return this.raw.keyboard; }
  get mouse() { return this.raw.mouse; }
  url() { return this.raw.url(); }
  title() { return this.raw.title(); }
  content() { return this.raw.content(); }
  goto(url, options) { return this.raw.goto(url, options); }
  reload(options) { return this.raw.reload(options); }
  goBack(options) { return this.raw.goBack(options); }
  goForward(options) { return this.raw.goForward(options); }
  waitForLoadState(state, options) { return this.raw.waitForLoadState(state, options); }
  waitForTimeout(timeout) { return this.raw.waitForTimeout(timeout); }
  screenshot(options) { return this.raw.screenshot(options); }
  bringToFront() { return this.raw.bringToFront(); }
  isClosed() { return this.raw.isClosed(); }
  click(selector, options) { return this.raw.click(selector, withoutHumanize(options)); }
  fill(selector, value, options) { return this.raw.fill(selector, value, withoutHumanize(options)); }
  setInputFiles(selector, files, options) { return this.raw.locator(selector).setInputFiles(files, withoutHumanize(options)); }
  locator(selector) { return new DonutLocator(this.raw.locator(selector)); }
  getByRole(role, options) { return new DonutLocator(this.raw.getByRole(role, options)); }
  getByText(text, options) { return new DonutLocator(this.raw.getByText(text, options)); }
  getByLabel(text, options) { return new DonutLocator(this.raw.getByLabel(text, options)); }
  getByPlaceholder(text, options) { return new DonutLocator(this.raw.getByPlaceholder(text, options)); }
  evaluate(expression, ...args) { return this.raw.evaluate(expression, ...args); }
  on(event, handler) { this.raw.on(event, handler); return this; }
  off(event, handler) { this.raw.off(event, handler); return this; }
}

class DonutTabs {
  constructor(session) { this.session = session; }
  async list() {
    const pages = await Promise.all(this.session.context.pages().map(async (page) => ({
      targetId: await this.session.targetId(page),
      url: page.url(),
      title: await page.title().catch(() => ''),
      managed: page === this.session.rawPage,
    })));
    return { pages };
  }
  async activate(targetId) {
    const page = await this.session.pageForTarget(targetId);
    if (!page) throw new Error('TARGET_NOT_FOUND: requested target is not open');
    await page.bringToFront();
    return { targetId };
  }
  async open() { throw new Error('TAB_CREATION_FORBIDDEN: Donut owns browser tabs'); }
  async close() { throw new Error('TAB_CLOSE_FORBIDDEN: Donut owns browser tabs'); }
}

class DonutCookies {
  constructor(session) { this.session = session; }
  list(urls) { return this.session.context.cookies(urls); }
  add(cookies) { return this.session.context.addCookies(cookies); }
  clear() { return this.session.context.clearCookies(); }
  getStorageState() { return this.session.context.storageState(); }
  setStorageState(state) { return this.session.context.setStorageState(state); }
  async exportNetscape(urls) {
    const cookies = await this.list(urls);
    const lines = ['# Netscape HTTP Cookie File'];
    for (const cookie of cookies) {
      const domain = cookie.domain || '';
      if (!domain) continue;
      lines.push([domain, domain.startsWith('.') ? 'TRUE' : 'FALSE', cookie.path || '/', cookie.secure ? 'TRUE' : 'FALSE', cookie.expires > 0 ? Math.floor(cookie.expires) : 0, cookie.name, cookie.value].join('\t'));
    }
    return `${lines.join('\n')}\n`;
  }
}

class DonutSession {
  constructor(options) {
    if (!Number.isSafeInteger(options.browserPid) || options.browserPid <= 0 || !Number.isSafeInteger(options.launchGeneration) || options.launchGeneration <= 0) {
      throw new Error('CDP_IDENTITY_REQUIRED: browserPid and launchGeneration are required');
    }
    this.profileId = options.profileId;
    this.cdpEndpoint = normalizeEndpoint(options.cdpEndpoint);
    this.browserPid = options.browserPid;
    this.launchGeneration = options.launchGeneration;
    this.browser = options.browser;
    this.context = options.context;
    this.rawPage = options.page;
    this.tabs = new DonutTabs(this);
    this.cookies = new DonutCookies(this);
    this._page = new DonutPage(this.rawPage);
  }

  static async connect(options) {
    const cdpEndpoint = normalizeEndpoint(options.cdpEndpoint);
    if (!Number.isSafeInteger(options.browserPid) || options.browserPid <= 0 || !Number.isSafeInteger(options.launchGeneration) || options.launchGeneration <= 0) {
      throw new Error('CDP_IDENTITY_REQUIRED: browserPid and launchGeneration are required');
    }
    if (!options.browser && typeof options.chromium?.connectOverCDP !== 'function') throw new Error('CDP_CONNECTOR_REQUIRED: provide chromium.connectOverCDP');
    const browser = options.browser || await options.chromium.connectOverCDP(cdpEndpoint, { timeout: options.timeoutMs || 15000 });
    const context = options.context || browser.contexts?.()[0];
    if (!context) throw new Error('CDP_CONTEXT_NOT_FOUND: Donut browser exposed no browser context');
    let page = options.page;
    if (options.grokTargetId) {
      // The managed target id is authoritative. Even when a caller supplies a
      // page object, verify that it still maps to that exact CDP target rather
      // than silently falling back to another tab.
      page = undefined;
      for (const candidate of context.pages?.() || []) {
        const cdp = await context.newCDPSession(candidate);
        try {
          const info = await cdp.send('Target.getTargetInfo');
          if (info?.targetInfo?.targetId === options.grokTargetId) { page = candidate; break; }
        } finally { await cdp.detach?.().catch(() => {}); }
      }
      if (!page) throw new Error('GROK_MANAGED_TARGET_STALE: requested target is not open');
    }
    page ||= context.pages?.()[0];
    if (!page) throw new Error('CDP_PAGE_NOT_FOUND: Donut browser exposed no page target');
    if (options.grokTargetId && !/^https:\/\/(www\.)?grok\.com\//i.test(page.url())) throw new Error('GROK_MANAGED_TARGET_STALE: requested target is not a Grok page');
    return new DonutSession({ ...options, cdpEndpoint, browser, context, page });
  }

  get connected() { return Boolean(this.browser?.isConnected?.() ?? true); }
  get page() { return this._page; }
  async targetId(page) {
    const cdp = await this.context.newCDPSession(page);
    try { return (await cdp.send('Target.getTargetInfo')).targetInfo?.targetId || null; }
    finally { await cdp.detach?.().catch(() => {}); }
  }
  async pageForTarget(targetId) {
    for (const page of this.context.pages()) if (await this.targetId(page) === targetId) return page;
    return null;
  }
  async disconnect() {
    if (typeof this.browser?.disconnect === 'function') return this.browser.disconnect();
    // Browser ownership remains with Donut. A Playwright Browser connected via
    // CDP may expose only close(), which is intentionally never called here.
    return undefined;
  }
}

module.exports = { DonutSession, DonutPage, DonutLocator, DonutTabs, DonutCookies };
