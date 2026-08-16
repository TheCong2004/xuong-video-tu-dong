import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import { resolveAllowedOrigin, applyCorsHeaders } from './src/server/cors/origins';

describe('OmniRoute CORS & Auth Transport Configuration', () => {
  it('allows local ArtCraft development and Tauri origins in CORS resolver', () => {
    expect(resolveAllowedOrigin('http://localhost:5173')).toBe('http://localhost:5173');
    expect(resolveAllowedOrigin('http://127.0.0.1:5173')).toBe('http://127.0.0.1:5173');
    expect(resolveAllowedOrigin('http://tauri.localhost')).toBe('http://tauri.localhost');
  });

  it('sets Access-Control-Allow-Credentials to true for allowed local origins', () => {
    const request = new Request('http://127.0.0.1:20128/api/providers', {
      headers: { Origin: 'http://localhost:5173' },
    });
    const response = new Response(JSON.stringify([]), { status: 200 });

    applyCorsHeaders(response, request);

    expect(response.headers.get('Access-Control-Allow-Origin')).toBe('http://localhost:5173');
    expect(response.headers.get('Access-Control-Allow-Credentials')).toBe('true');
  });

  it('rejects unallowed external origins from accessing management endpoints', () => {
    expect(resolveAllowedOrigin('http://untrusted-external-site.com')).toBeNull();
  });

  it('configures frontend API client to send credentials: include', () => {
    const indexContent = fs.readFileSync(
      path.resolve(__dirname, './index.tsx'),
      'utf8',
    );
    expect(indexContent).toContain('credentials: "include"');
  });
});
