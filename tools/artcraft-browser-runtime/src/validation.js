'use strict';

const { RuntimeError } = require('./errors');

const PROFILE_RE = /^[A-Za-z0-9_-]{1,128}$/;
const ALLOWED_RUN_FIELDS = new Set(['url', 'headless', 'cold_start_only', 'browser_engine', 'ensure_page']);
const DEFAULT_URL = 'https://grok.com/imagine';

function assertProfileId(profileId) {
  if (typeof profileId !== 'string' || !PROFILE_RE.test(profileId)) {
    throw new RuntimeError('INVALID_REQUEST', 'Invalid profile identifier', 400);
  }
  return profileId;
}

function parseBody(body) {
  if (body === undefined || body === null || body === '') return {};
  if (typeof body === 'object' && !Buffer.isBuffer(body)) return body;
  if (typeof body !== 'string' && !Buffer.isBuffer(body)) {
    throw new RuntimeError('INVALID_REQUEST', 'JSON body required', 400);
  }
  try {
    const value = JSON.parse(Buffer.isBuffer(body) ? body.toString('utf8') : body);
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('object required');
    return value;
  } catch {
    throw new RuntimeError('INVALID_REQUEST', 'Malformed JSON body', 400);
  }
}

function assertGrokUrl(value) {
  const url = new URL(value || DEFAULT_URL);
  const host = (url.hostname || '').toLowerCase();
  if (url.protocol !== 'https:' || !(host === 'grok.com' || host.endsWith('.grok.com'))) {
    throw new RuntimeError('INVALID_REQUEST', 'Grok URL is not allowed', 400);
  }
  return url.toString();
}

function normalizeRunRequest(body) {
  const input = parseBody(body);
  const unknown = Object.keys(input).filter((key) => !ALLOWED_RUN_FIELDS.has(key));
  if (unknown.length) throw new RuntimeError('INVALID_REQUEST', 'Unknown run field', 400, { fields: unknown.sort() });
  if (input.headless !== undefined && typeof input.headless !== 'boolean') {
    throw new RuntimeError('INVALID_REQUEST', 'headless must be boolean', 400);
  }
  if (input.cold_start_only !== undefined && typeof input.cold_start_only !== 'boolean') {
    throw new RuntimeError('INVALID_REQUEST', 'cold_start_only must be boolean', 400);
  }
  if (input.browser_engine !== undefined && input.browser_engine !== 'CHROME_FOR_TESTING' && input.browser_engine !== 'chromium') {
    throw new RuntimeError('INVALID_REQUEST', 'Unsupported browser engine', 400);
  }
  return {
    url: assertGrokUrl(input.url),
    headless: input.headless === true,
    coldStartOnly: input.cold_start_only !== false,
    browserEngine: 'CHROME_FOR_TESTING',
    ensurePage: input.ensure_page !== false,
  };
}

const PAGE_PURPOSES = new Set(['GROK_AUTOMATION', 'LOGIN', 'RESULT', 'USER', 'UNKNOWN']);

function normalizePageRequest(body) {
  const input = parseBody(body);
  const allowed = new Set(['url', 'purpose', 'reuseExisting']);
  const unknown = Object.keys(input).filter((key) => !allowed.has(key));
  if (unknown.length) throw new RuntimeError('INVALID_REQUEST', 'Unknown page field', 400, { fields: unknown.sort() });
  const url = assertGrokUrl(input.url || DEFAULT_URL);
  const purpose = String(input.purpose || 'GROK_AUTOMATION');
  if (!PAGE_PURPOSES.has(purpose)) throw new RuntimeError('INVALID_REQUEST', 'Unsupported page purpose', 400, { purpose });
  if (input.reuseExisting !== undefined && typeof input.reuseExisting !== 'boolean') throw new RuntimeError('INVALID_REQUEST', 'reuseExisting must be boolean', 400);
  return { url, purpose, reuseExisting: input.reuseExisting !== false };
}

function parseStopRequest(body) {
  const input = parseBody(body);
  const allowed = new Set(['browser_pid', 'remote_debugging_port', 'launch_generation', 'ownership_nonce']);
  const unknown = Object.keys(input).filter((key) => !allowed.has(key));
  if (unknown.length) throw new RuntimeError('INVALID_REQUEST', 'Unknown stop field', 400, { fields: unknown.sort() });
  return input;
}

function assertLoopbackHost(host) {
  if (!['127.0.0.1', 'localhost', '::1'].includes(String(host).toLowerCase())) {
    throw new RuntimeError('INVALID_REQUEST', 'Runtime host must be loopback', 400);
  }
}

module.exports = { DEFAULT_URL, PAGE_PURPOSES, assertProfileId, parseBody, normalizeRunRequest, normalizePageRequest, parseStopRequest, assertLoopbackHost };
