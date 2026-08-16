import React, { useCallback, useEffect, useState } from 'react';
import { CheckCircle2, Loader2, Settings, TestTube2, XCircle } from 'lucide-react';
import {
  FlowordVisualProvider,
  getFlowordVisualProvider,
  testFlowordVisualProvider,
} from '../api/flowordClient';

export const VisualGenerationProvider: React.FC = () => {
  const [provider, setProvider] = useState<FlowordVisualProvider | null>(null);
  const [loading, setLoading] = useState(true);
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setProvider(await getFlowordVisualProvider());
    } catch {
      setError('Unable to read OmniRoute visual provider status.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const testProvider = async () => {
    setTesting(true);
    setError(null);
    try {
      setProvider(await testFlowordVisualProvider());
    } catch {
      setError('OmniRoute provider test could not be completed.');
    } finally {
      setTesting(false);
    }
  };

  const configured = provider?.status === 'configured';

  return (
    <section className="floword-card p-5" aria-labelledby="visual-generation-provider-title">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h3 id="visual-generation-provider-title" className="text-sm font-semibold text-white">AI / Visual Generation</h3>
          <p className="mt-1 text-xs text-zinc-500">All AI capabilities (text, voice, image, video) are routed exclusively through OmniRoute.</p>
        </div>
        <span className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-semibold ${configured ? 'bg-green-500/10 text-green-400' : 'bg-amber-500/10 text-amber-400'}`}>
          {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : configured ? <CheckCircle2 className="h-3.5 w-3.5" /> : <XCircle className="h-3.5 w-3.5" />}
          {loading ? 'Checking' : configured ? 'Configured' : 'Not configured'}
        </span>
      </div>

      <dl className="mt-4 grid gap-3 text-xs sm:grid-cols-2">
        <div><dt className="text-zinc-500">Gateway</dt><dd className="mt-1 text-zinc-200">{provider?.provider ?? 'OmniRoute'}</dd></div>
        <div><dt className="text-zinc-500">Capability</dt><dd className="mt-1 text-zinc-200">{provider?.capabilities.join(' / ') ?? 'Text / Voice / Image / Video'}</dd></div>
        <div><dt className="text-zinc-500">Authentication</dt><dd className="mt-1 text-zinc-200">{provider?.auth_method ?? 'OmniRoute API key'}</dd></div>
        <div><dt className="text-zinc-500">Credential source</dt><dd className="mt-1 text-zinc-200">{provider?.credential_source ?? 'OmniRoute (external)'}</dd></div>
      </dl>

      <p className="mt-4 text-xs text-zinc-400" role={error ? 'alert' : undefined}>{error ?? provider?.message ?? 'Checking OmniRoute video provider configuration.'}</p>

      <div className="mt-4 flex flex-wrap gap-2">
        <button
          type="button"
          className="floword-button floword-button-secondary"
          onClick={testProvider}
          disabled={testing || loading}
          id="visual-provider-test-button"
        >
          {testing ? <Loader2 className="h-4 w-4 animate-spin" /> : <TestTube2 className="h-4 w-4" />} Test OmniRoute
        </button>
        <button
          type="button"
          className="floword-button floword-button-secondary"
          onClick={refresh}
          disabled={loading || testing}
          id="visual-provider-refresh-button"
        >
          <Settings className="h-4 w-4" /> Refresh
        </button>
      </div>
    </section>
  );
};
