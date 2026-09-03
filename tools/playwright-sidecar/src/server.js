const express = require('express');
const sessionManager = require('./session-manager');
const actionRunner = require('./action-runner');

const app = express();
const PORT = Number(process.env.PLAYWRIGHT_SIDECAR_PORT || 9223);
const token = process.env.FLOWORD_SIDECAR_TOKEN;
// Image payloads are base64 encoded by the Floword pipeline; keep a bounded
// but practical limit above the common 2 MB source-image case.
app.use(express.json({ limit: process.env.FLOWORD_SIDECAR_BODY_LIMIT || '32mb' }));
app.use((req, res, next) => {
  if (token && req.get('authorization') !== `Bearer ${token}`) return res.status(401).json({ error: 'UNAUTHORIZED' });
  next();
});
const statusFor = (code) => ({ INVALID_REQUEST: 400, INVALID_PROFILE: 400, PROTOCOL_MISMATCH: 400, PAGE_TARGET_ID_REQUIRED: 400, UNAUTHORIZED: 401, PROFILE_NOT_FOUND: 404, GROK_TAB_NOT_FOUND: 404, PAGE_NOT_FOUND: 404, GROK_MANAGED_TARGET_STALE: 409, AMBIGUOUS_MANAGED_SESSION: 409, PAGE_NOT_OWNED: 409, JOB_ALREADY_RUNNING: 409, CORRELATION_CONFLICT: 409, PLAYWRIGHT_PROFILE_LOCKED: 409, WORKER_BUSY: 409, INVALID_LEASE: 409, CAPABILITY_UNAVAILABLE: 422, GROK_AUTH_REQUIRED: 422, GROK_NOT_LOGGED_IN: 422, PLAYWRIGHT_RUNTIME_OFFLINE: 503, PLAYWRIGHT_PROFILE_OFFLINE: 503, EXTENSION_NOT_LOADED: 503, EXTENSION_NOT_READY: 503, EXTENSION_CONTENT_CONTEXT_NOT_FOUND: 503, EXTENSION_PRODUCTION_CONTENT_CONTRACT_MISSING: 503, CDP_AUTOMATION_UNAVAILABLE: 503, CDP_IDENTITY_REQUIRED: 503, CDP_SESSION_STALE: 503, CDP_CONTEXT_NOT_FOUND: 503, EXTENSION_PRODUCTION_WORKER_NOT_READY: 503, CONTENT_SCRIPT_BIND_TIMEOUT: 503, EXTENSION_PRODUCTION_BRIDGE_NOT_FOUND: 503, CONTENT_SCRIPT_NOT_READY: 503, RESULT_TIMEOUT: 504 }[code] || 500);
const route = (fn) => async (req, res) => {
  try {
    res.json(await fn(req, res));
  } catch (e) {
    const message = String(e.message || e);
    const code = message.split(':', 1)[0];
    const status = statusFor(code);
    console.warn(`[sidecar] ${req.method} ${req.path} ${code}: ${message.replace(/Bearer\s+[^\s]+/gi, 'Bearer [redacted]')}`);
    res.status(status).json({ error: { code, message, details: e.details || {}, retryable: [409, 503, 504].includes(status) } });
  }
};

app.get('/health', (_req, res) => res.json({ status: 'ok', service: 'floword-playwright-runtime', protocol: 'floword-playwright', protocolVersion: 1, pid: process.pid, playwrightVersion: require('playwright/package.json').version, extensionPath: process.env.FLOWORD_CHROMEX_EXTENSION_PATH || null }));
app.post('/v1/profiles/:profileId/start', route((req) => sessionManager.ensureProfile(req.params.profileId, req.body || {})));
app.post('/v1/profiles/:profileId/stop', route((req) => sessionManager.stop(req.params.profileId)));
app.get('/v1/profiles/:profileId/status', route((req) => sessionManager.health(req.params.profileId, req.query.targetId || null)));
app.get('/v1/profiles/:profileId/pages', route((req) => sessionManager.getPages(req.params.profileId, req.query.targetId || null)));
app.post('/v1/profiles/:profileId/artifacts/fetch', route((req) => sessionManager.fetchArtifact(req.params.profileId, req.body?.locator)));
app.post('/v1/profiles/:profileId/dispatch', route((req) => sessionManager.dispatch({ ...(req.body || {}), profileId: req.params.profileId })));
app.post('/v1/profiles/:profileId/trace/start', route((req) => sessionManager.startTrace(req.params.profileId)));
app.post('/v1/profiles/:profileId/trace/stop', route((req) => sessionManager.stopTrace(req.params.profileId, req.body?.outputPath)));
app.post('/v1/jobs/:jobId/cancel', route((req) => sessionManager.cancel(req.params.jobId, req.body?.targetRequestId)));

// Backward-compatible endpoints used by the existing developer tooling.
app.post('/connect', route((req) => sessionManager.ensureProfile(req.body.profileId, req.body)));
app.get('/pages', route(() => sessionManager.getPages([...sessionManager.sessions.keys()][0])));
app.post('/navigate', route((req) => actionRunner.navigate(req.body.url)));
app.post('/click', route((req) => actionRunner.click(req.body.selector)));
app.post('/fill', route((req) => actionRunner.fill(req.body.selector, req.body.value)));
app.post('/upload', route((req) => actionRunner.upload(req.body.selector, req.body.filePaths)));
app.post('/screenshot', route((req) => actionRunner.screenshot(req.body.outputPath)));
app.post('/trace/start', route(() => actionRunner.startTrace()));
app.post('/trace/stop', route((req) => actionRunner.stopTrace(req.body.outputPath)));
app.post('/cancel', route((req) => {
  if (!req.body?.targetRequestId) throw new Error('INVALID_REQUEST: targetRequestId is required');
  return sessionManager.cancel(req.body.jobId, req.body.targetRequestId);
}));
app.post('/disconnect', route(() => sessionManager.disconnect()));

// Express' JSON parser raises a body-parser `entity.too.large` error before a
// route is entered. Keep that failure machine-readable so ArtCraft can reject
// the request without parsing an HTML error page.
app.use((err, _req, res, next) => {
  if (err?.type === 'entity.too.large' || err?.status === 413) {
    return res.status(413).json({
      error: {
        code: 'PAYLOAD_TOO_LARGE',
        message: 'Request payload exceeds the configured sidecar limit',
        details: {},
        retryable: false,
      },
    });
  }
  return next(err);
});

const server = app.listen(PORT, '127.0.0.1', () => console.log(`Floword Playwright runtime listening on 127.0.0.1:${PORT}`));
const shutdown = async () => { await sessionManager.disconnect(); server.close(() => process.exit(0)); };
process.on('SIGINT', shutdown); process.on('SIGTERM', shutdown);
