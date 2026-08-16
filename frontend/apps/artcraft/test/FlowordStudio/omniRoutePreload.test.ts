import fs from 'node:fs';
import path from 'node:path';

describe('OmniRoute UI Instant Preload & Persistent Lifecycle', () => {
  const mainApp = fs.readFileSync(
    path.resolve(__dirname, '../../app/src/pages/MainApp.tsx'),
    'utf8',
  );
  const omniRouteIndex = fs.readFileSync(
    path.resolve(__dirname, '../../app/src/pages/OmniRoute/index.tsx'),
    'utf8',
  );

  it('preloads PageOmniRoute in background persistent container on app shell mount', () => {
    expect(mainApp).toContain('data-testid="omniroute-persistent-container"');
    expect(mainApp).toContain('<PageOmniRoute />');
  });

  it('toggles visibility via CSS rather than unmounting PageOmniRoute on tab switch', () => {
    expect(mainApp).toContain('tabId === "OMNI_ROUTE" ? "h-[calc(100vh-56px)] w-full overflow-hidden block" : "hidden"');
  });

  it('does NOT gate iframe render behind health readiness (renders iframe unconditionally)', () => {
    expect(omniRouteIndex).toContain('data-testid="omniroute-iframe"');
    expect(omniRouteIndex).not.toMatch(/isReady\s*\?\s*<iframe/);
  });

  it('displays a non-blocking floating banner when backend is connecting/unreachable', () => {
    expect(omniRouteIndex).toContain('data-testid="omniroute-connecting-banner"');
    expect(omniRouteIndex).toContain('OmniRoute đang kết nối...');
  });
});
