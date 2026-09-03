'use strict';

const http = require('node:http');
const https = require('node:https');
const { URL } = require('node:url');
const { RuntimeError } = require('./errors');

function assertLoopbackEndpoint(endpoint) {
  const value = new URL(endpoint);
  if (value.protocol !== 'http:' || !['127.0.0.1', 'localhost', '::1'].includes(value.hostname.toLowerCase())) {
    throw new RuntimeError('CDP_ENDPOINT_NOT_LOOPBACK', 'CDP endpoint must be loopback HTTP', 400);
  }
  return value;
}

function requestJson(endpoint, { method = 'GET', path = '/', timeoutMs = 3000, requestImpl, parseJson = true } = {}) {
  const base = assertLoopbackEndpoint(endpoint);
  const transport = base.protocol === 'https:' ? https : http;
  return new Promise((resolve, reject) => {
    const options = { method, hostname: base.hostname, port: base.port, path, timeout: timeoutMs, headers: { accept: 'application/json' } };
    const request = (requestImpl || transport.request)(options, (response) => {
      let data = '';
      response.setEncoding('utf8');
      response.on('data', (chunk) => { if (data.length < 1024 * 1024) data += chunk; });
      response.on('end', () => {
        if (response.statusCode < 200 || response.statusCode >= 300) return reject(new RuntimeError('BROWSER_CDP_REQUEST_FAILED', 'CDP request failed', 503));
        if (!parseJson) return resolve(data);
        try { resolve(JSON.parse(data)); } catch { reject(new RuntimeError('BROWSER_CDP_INVALID_JSON', 'CDP returned invalid JSON', 503)); }
      });
    });
    request.on('timeout', () => request.destroy(new RuntimeError('BROWSER_CDP_TIMEOUT', 'CDP request timed out', 503, {}, true)));
    request.on('error', reject);
    request.end();
  });
}

class CdpHttpAdapter {
  constructor(endpoint, options = {}) {
    assertLoopbackEndpoint(endpoint);
    this.endpoint = endpoint.replace(/\/$/, '');
    this.requestJson = options.requestJson || ((request) => requestJson(this.endpoint, request));
  }

  async version() { return this.requestJson({ path: '/json/version' }); }
  async targets() { return this.requestJson({ path: '/json/list' }); }
  async createTarget(url) {
    const value = new URL(url);
    const encoded = encodeURIComponent(value.toString());
    return this.requestJson({ method: 'PUT', path: `/json/new?${encoded}` });
  }
  async closeTarget(targetId) {
    if (typeof targetId !== 'string' || !targetId.trim()) throw new RuntimeError('PAGE_TARGET_ID_REQUIRED', 'targetId is required', 400);
    return this.requestJson({ path: `/json/close/${encodeURIComponent(targetId)}`, parseJson: false });
  }
}

module.exports = { CdpHttpAdapter, assertLoopbackEndpoint, requestJson };
