import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

describe('OmniRoute Real Frontend Integration & Capability Retention', () => {
  const mainApp = fs.readFileSync(
    path.resolve(__dirname, '../MainApp.tsx'),
    'utf8',
  );
  const omniRouteIndex = fs.readFileSync(
    path.resolve(__dirname, './index.tsx'),
    'utf8',
  );

  it('preloads PageOmniRoute in background persistent container on app shell mount', () => {
    expect(mainApp).toContain('data-testid="omniroute-persistent-container"');
    expect(mainApp).toContain('<PageOmniRoute />');
  });

  it('toggles visibility via CSS rather than unmounting PageOmniRoute on tab switch', () => {
    expect(mainApp).toContain('tabId === "OMNI_ROUTE" ? "h-[calc(100vh-56px)] w-full overflow-hidden block" : "hidden"');
  });

  it('does NOT contain hardcoded facade strings or fake configured provider cards', () => {
    expect(omniRouteIndex).not.toContain('Google Veo (Video)');
    expect(omniRouteIndex).not.toContain('Seedance (Video)');
    expect(omniRouteIndex).not.toContain('DEFAULT_PROVIDERS');
    expect(omniRouteIndex).not.toContain('category: "video", status: "configured"');
  });

  it('renders original OmniRoute sidebar navigation groups (OMNIPROXY, Analytics, Monitoring, Agentic, Config)', () => {
    expect(omniRouteIndex).toContain('data-testid="omniroute-sidebar-nav"');
    expect(omniRouteIndex).toContain('OMNIPROXY');
    expect(omniRouteIndex).toContain('Analytics & Costs');
    expect(omniRouteIndex).toContain('Monitoring & Health');
    expect(omniRouteIndex).toContain('Agentic Features');
    expect(omniRouteIndex).toContain('v3.8.49');
  });

  it('fetches real provider and model data from OmniRoute API without localhost iframe', () => {
    expect(omniRouteIndex).not.toContain('<iframe');
    expect(omniRouteIndex).toContain('OMNIROUTE_API_BASE');
    expect(omniRouteIndex).toContain('/api/providers');
    expect(omniRouteIndex).toContain('/v1/models');
  });

  it('displays a non-blocking banner when backend on :20128 is connecting/unreachable', () => {
    expect(omniRouteIndex).toContain('data-testid="omniroute-connecting-banner"');
    expect(omniRouteIndex).toContain('OmniRoute backend service is connecting on port 20128');
  });
});
