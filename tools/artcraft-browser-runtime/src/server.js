'use strict';

const http = require('node:http');
const { LocalBrowserRuntime } = require('./runtime-manager');
const { RuntimeError, asRuntimeError, envelope } = require('./errors');
const { assertProfileId, normalizeRunRequest, normalizePageRequest, parseStopRequest, assertLoopbackHost } = require('./validation');

class RuntimeService {
  constructor(runtime = new LocalBrowserRuntime()) { this.runtime = runtime; }

  async handle({ method, pathname, body, headers = {} }) {
    try {
      if (method === 'GET' && pathname === '/health') return { status: 200, body: this.runtime.health() };
      const localRun = pathname.match(/^\/v1\/local\/browser\/profiles\/([^/]+)\/run$/);
      const localPages = pathname.match(/^\/v1\/local\/browser\/profiles\/([^/]+)\/pages(?:\/([^/]+))?$/);
      if (localRun || localPages) {
        const profileId = decodeURIComponent((localRun || localPages)[1]);
        assertProfileId(profileId);
        if (headers['content-type'] && !headers['content-type'].toLowerCase().includes('application/json') && method !== 'GET') throw new RuntimeError('INVALID_REQUEST', 'Content-Type must be application/json', 400);
        if (localRun) {
          if (method !== 'POST') throw new RuntimeError('INVALID_REQUEST', 'Method not allowed', 405);
          const request = normalizeRunRequest(body); request.ensurePage = false;
          return { status: 200, body: await this.runtime.run(profileId, request) };
        }
        if (method === 'GET' && !localPages[2]) return { status: 200, body: await this.runtime.listPages(profileId) };
        if (method === 'POST' && !localPages[2]) return { status: 200, body: await this.runtime.createPage(profileId, normalizePageRequest(body)) };
        if (method === 'DELETE' && localPages[2]) return { status: 200, body: await this.runtime.deletePage(profileId, decodeURIComponent(localPages[2])) };
        throw new RuntimeError('INVALID_REQUEST', 'Method not allowed', 405);
      }
      const match = pathname.match(/^\/v1\/profiles\/([^/]+)\/(run|stop)$/);
      if (!match) throw new RuntimeError('INVALID_REQUEST', 'Route not found', 404);
      const profileId = decodeURIComponent(match[1]);
      assertProfileId(profileId);
      if (headers['content-type'] && !headers['content-type'].toLowerCase().includes('application/json')) {
        throw new RuntimeError('INVALID_REQUEST', 'Content-Type must be application/json', 400);
      }
      if (method === 'POST' && match[2] === 'run') return { status: 200, body: await this.runtime.run(profileId, normalizeRunRequest(body)) };
      if (method === 'POST' && match[2] === 'stop') return { status: 200, body: await this.runtime.stop(profileId, parseStopRequest(body)) };
      throw new RuntimeError('INVALID_REQUEST', 'Method not allowed', 405);
    } catch (error) {
      const value = asRuntimeError(error);
      return { status: value.status || 500, body: envelope(value) };
    }
  }
}

function createServer(options = {}) {
  const host = options.host || process.env.ARTCRAFT_BROWSER_RUNTIME_HOST || '127.0.0.1';
  assertLoopbackHost(host);
  const port = Number(options.port || process.env.ARTCRAFT_BROWSER_RUNTIME_PORT || 10108);
  const service = options.service || new RuntimeService(options.runtime);
  const server = http.createServer(async (request, response) => {
    const chunks = [];
    let size = 0;
    request.on('data', (chunk) => { size += chunk.length; if (size <= 1024 * 1024) chunks.push(chunk); });
    request.on('end', async () => {
      const result = await service.handle({ method: request.method, pathname: new URL(request.url, `http://${host}`).pathname, body: Buffer.concat(chunks), headers: request.headers });
      const payload = JSON.stringify(result.body);
      response.writeHead(result.status, { 'content-type': 'application/json; charset=utf-8', 'content-length': Buffer.byteLength(payload) });
      response.end(payload);
    });
  });
  return { server, service, host, port };
}

if (require.main === module) {
  const { server, host, port } = createServer();
  server.listen(port, host, () => process.stdout.write(`ArtCraft Local Browser Runtime READY ${host}:${port}\n`));
  const shutdown = () => server.close(() => process.exit(0));
  process.once('SIGINT', shutdown);
  process.once('SIGTERM', shutdown);
}

module.exports = { RuntimeService, createServer };
