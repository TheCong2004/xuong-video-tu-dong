import fs from 'node:fs';
import path from 'node:path';

describe('consolidated Floword shell', () => {
  const topBar = fs.readFileSync(
    path.resolve(__dirname, '../../app/src/components/signaled/TopBar/TopBar.tsx'),
    'utf8',
  );
  const activeSource = topBar.replace(/\/\*[\s\S]*?\*\//g, '');
  const visualProvider = fs.readFileSync(
    path.resolve(__dirname, '../../app/src/pages/FlowordStudio/src/components/VisualGenerationProvider.tsx'),
    'utf8',
  );

  it('does not render legacy Credits or Upgrade controls', () => {
    expect(activeSource).not.toMatch(/Buy credits|>\s*Upgrade\s*</i);
    expect(activeSource).not.toContain('sumTotalCredits');
  });

  it('does not poll global credit or subscription state', () => {
    expect(activeSource).not.toContain('useCreditsState');
    expect(activeSource).not.toContain('CREDITS_POLL_INTERVAL');
    expect(activeSource).not.toContain('useSubscriptionState');
  });

  it('routes visual generation through OmniRoute instead of provider-specific direct logins', () => {
    expect(visualProvider).toContain('getFlowordVisualProvider');
    expect(visualProvider).toContain('testFlowordVisualProvider');
    expect(visualProvider).not.toContain('open_sora_login_command');
    expect(visualProvider).not.toMatch(/type=["']password["']/);
  });
});
