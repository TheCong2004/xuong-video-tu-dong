const test = require('node:test');
const assert = require('node:assert/strict');
const http = require('node:http');
const { spawn } = require('node:child_process');
const path = require('node:path');

test('payload above 32 MB returns a structured 413 JSON error', async () => {
  const port = 19323 + Math.floor(Math.random() * 500);
  const child = spawn(process.execPath, [path.join(__dirname, '..', 'src', 'server.js')], {
    env: { ...process.env, PLAYWRIGHT_SIDECAR_PORT: String(port), FLOWORD_SIDECAR_BODY_LIMIT: '32mb' },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  try {
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('sidecar did not start')), 10000);
      child.stdout.on('data', (chunk) => { if (String(chunk).includes('listening')) { clearTimeout(timer); resolve(); } });
      child.once('exit', (code) => reject(new Error(`sidecar exited before start: ${code}`)));
    });
    const body = JSON.stringify({ payload: 'x'.repeat(33 * 1024 * 1024) });
    const result = await new Promise((resolve, reject) => {
      const req = http.request({ hostname: '127.0.0.1', port, path: '/v1/profiles/test/dispatch', method: 'POST', headers: { 'content-type': 'application/json', 'content-length': Buffer.byteLength(body) } }, (res) => {
        let text = '';
        res.setEncoding('utf8');
        res.on('data', (chunk) => { text += chunk; });
        res.on('end', () => resolve({ status: res.statusCode, body: JSON.parse(text) }));
      });
      req.once('error', reject);
      req.end(body);
    });
    assert.equal(result.status, 413);
    assert.equal(result.body.error.code, 'PAYLOAD_TOO_LARGE');
  } finally {
    child.kill();
  }
});
