/* Passive CDP navigation diagnostic. It never creates, closes, reloads, or
 * navigates a browser/tab. Usage:
 *   node scripts/cdp-navigation-diagnostic.cjs <cdpEndpoint> [profileId]
 */
const { chromium } = require('playwright');
const crypto = require('crypto');

const endpoint = process.argv[2] || process.env.FLOWORD_CDP_ENDPOINT;
if (!endpoint) throw new Error('CDP endpoint is required');
const profileId = process.argv[3] || null;
const grokUrl = /^https:\/\/(www\.)?grok\.com\//i;
const safeUrl = (value) => {
  try { const u = new URL(String(value)); return `${u.origin}${u.pathname}`; } catch (_) { return null; }
};
const now = () => new Date().toISOString();
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function main() {
  const browser = await chromium.connectOverCDP(endpoint, { timeout: 15000 });
  const context = browser.contexts()[0];
  if (!context) throw new Error('CDP context not found');
  const pages = context.pages().filter((p) => grokUrl.test(p.url()));
  if (pages.length !== 1) throw new Error(`Expected exactly one Grok page, found ${pages.length}`);
  const page = pages[0];
  const cdp = await context.newCDPSession(page);
  const trace = [];
  let phase = 'A';
  const push = (event, data = {}) => {
    trace.push({ timestamp: now(), phase, event, pageUrl: safeUrl(page.url()), ...data });
    process.stdout.write(`${JSON.stringify(trace[trace.length - 1])}\n`);
  };
  const requestIds = new Set();
  const documentRequests = new Set();
  const listen = (name, handler) => cdp.on(name, handler);
  listen('Page.frameStartedLoading', ({ frameId }) => push('Page.frameStartedLoading', { frameId }));
  listen('Page.frameNavigated', ({ frame }) => push('Page.frameNavigated', { frameId: frame.id, loaderId: frame.loaderId, url: safeUrl(frame.url) }));
  listen('Page.lifecycleEvent', ({ frameId, loaderId, name }) => push('Page.lifecycleEvent', { frameId, loaderId, name }));
  listen('Runtime.executionContextCreated', ({ context: value }) => push('Runtime.executionContextCreated', { contextId: value.id, frameId: value.auxData?.frameId || null }));
  listen('Runtime.executionContextDestroyed', ({ executionContextId }) => push('Runtime.executionContextDestroyed', { contextId: executionContextId }));
  listen('Runtime.executionContextsCleared', () => push('Runtime.executionContextsCleared'));
  listen('Network.requestWillBeSent', (event) => {
    if (event.type !== 'Document') return;
    requestIds.add(event.requestId); documentRequests.add(event.requestId);
    push('Network.requestWillBeSent', { requestId: event.requestId, frameId: event.frameId, loaderId: event.loaderId, url: safeUrl(event.request.url), redirectSource: safeUrl(event.redirectResponse?.url) });
  });
  listen('Network.responseReceived', (event) => {
    if (event.type !== 'Document' || !documentRequests.has(event.requestId)) return;
    push('Network.responseReceived', { requestId: event.requestId, frameId: event.frameId, loaderId: event.loaderId, url: safeUrl(event.response.url), status: event.response.status });
  });
  listen('Network.loadingFailed', (event) => {
    if (!requestIds.has(event.requestId)) return;
    push('Network.loadingFailed', { requestId: event.requestId, errorText: event.errorText, canceled: event.canceled });
  });

  await cdp.send('Page.enable');
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Page.setLifecycleEventsEnabled', { enabled: true });
  push('diagnostic.phase.start', { durationMs: 10000 });
  await sleep(10000);

  phase = 'B';
  const wakeId = `wake_${crypto.randomUUID()}`;
  const beforeUrl = safeUrl(page.url());
  push('wake-attempt', { wakeId, attempt: 1, beforeUrl });
  try {
    await page.evaluate(async (requestWakeId) => new Promise((resolve) => {
      let done = false;
      const finish = (value) => { if (done) return; done = true; window.removeEventListener('floword.runtime.wake.result', onResult); resolve(value); };
      const onResult = (event) => finish(event instanceof CustomEvent ? event.detail : null);
      window.addEventListener('floword.runtime.wake.result', onResult, { once: true });
      window.setTimeout(() => finish(null), 5000);
      window.dispatchEvent(new CustomEvent('floword.runtime.wake', { detail: { protocol: 'floword-production', protocolVersion: 1, wakeId: requestWakeId } }));
    }), wakeId);
    push('wake-result', { wakeId, afterUrl: safeUrl(page.url()) });
  } catch (error) {
    push('wake-error', { wakeId, afterUrl: safeUrl(page.url()), error: String(error?.message || error).split('\n')[0] });
  }
  await sleep(10000);
  push('diagnostic.phase.end', { durationMs: 10000 });
  const nav = trace.filter((e) => e.event === 'Page.frameNavigated');
  const contexts = trace.filter((e) => e.event.startsWith('Runtime.executionContext'));
  const responses = trace.filter((e) => e.event === 'Network.responseReceived');
  const failed = trace.filter((e) => e.event === 'Network.loadingFailed');
  process.stdout.write(JSON.stringify({
    conclusion: nav.length > 1 ? 'document_or_frame_navigation_churn' : contexts.length > 1 ? 'renderer_context_reset_without_proven_navigation' : failed.length ? 'proxy_or_network_failure' : 'no_navigation_churn_observed',
    frameNavigations: nav.length, contextEvents: contexts.length, documentResponses: responses.length, failedRequests: failed.length,
    trace,
  }, null, 2) + '\n');
  await cdp.detach().catch(() => {});
}

main().catch((error) => { console.error(error.stack || error); process.exitCode = 1; });
