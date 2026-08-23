const express = require('express');
const sessionManager = require('./session-manager');
const actionRunner = require('./action-runner');

const app = express();
const PORT = Number(process.env.PLAYWRIGHT_SIDECAR_PORT || 9223);
const token = process.env.FLOWORD_SIDECAR_TOKEN;
app.use(express.json({ limit: '2mb' }));
app.use((req, res, next) => {
  if (token && req.get('authorization') !== `Bearer ${token}`) return res.status(401).json({ error: 'UNAUTHORIZED' });
  next();
});
const route = (fn) => async (req, res) => { try { res.json(await fn(req, res)); } catch (e) { res.status(500).json({ error: { code: String(e.message).split(':', 1)[0], message: e.message } }); } };

app.get('/health', (_req, res) => res.json({ status: 'ok', service: 'floword-playwright-runtime', protocolVersion: 1, pid: process.pid }));
app.post('/v1/profiles/:profileId/start', route((req) => sessionManager.ensureProfile(req.params.profileId, req.body || {})));
app.post('/v1/profiles/:profileId/stop', route((req) => sessionManager.stop(req.params.profileId)));
app.get('/v1/profiles/:profileId/status', route((req) => sessionManager.health(req.params.profileId)));
app.get('/v1/profiles/:profileId/pages', route((req) => sessionManager.getPages(req.params.profileId)));
app.post('/v1/profiles/:profileId/dispatch', route((req) => sessionManager.dispatch({ ...(req.body || {}), profileId: req.params.profileId })));
app.post('/v1/jobs/:jobId/cancel', route((req) => sessionManager.cancel(req.params.jobId)));

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
app.post('/cancel', route((req) => sessionManager.cancel(req.body.jobId)));
app.post('/disconnect', route(() => sessionManager.disconnect()));

const server = app.listen(PORT, '127.0.0.1', () => console.log(`Floword Playwright runtime listening on 127.0.0.1:${PORT}`));
const shutdown = async () => { await sessionManager.disconnect(); server.close(() => process.exit(0)); };
process.on('SIGINT', shutdown); process.on('SIGTERM', shutdown);
