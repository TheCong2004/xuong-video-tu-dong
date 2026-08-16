import React, { useEffect, useMemo, useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import {
  createMontageProject,
  createStoryProject,
  flipImageFile,
  getAiModels,
  getAiProviders,
  getMontagePipelines,
  getResearchPlatforms,
  getResearchSessionStatus,
  loginResearchSession,
  verifyResearchSession,
  reconnectResearchSession,
  clearResearchSession,
  getStoryCreateStatus,
  getStoryGenres,
  MontagePipeline,
  probeVideo,
  ResearchPlatform,
  searchYouTube,
  ResearchSession,
  StoryGenre,
  VideoProbe,
  YouTubeSearchVideo,
} from '../api/flowordClient';

export type CapabilityTool = 'story' | 'video' | 'image' | 'research' | 'media' | 'providers';

interface CapabilityToolsViewProps {
  tool: CapabilityTool;
  researchPlatform?: string;
  xhsVariant?: 'mainland' | 'international';
  onVariantChange?: (variant: 'mainland' | 'international') => void;
}

const fieldClass = 'w-full rounded-[9px] border border-white/[0.08] bg-white/[0.04] px-3 py-2.5 text-sm text-white outline-none focus:border-white/20';
const buttonClass = 'rounded-[9px] bg-[#e54d5e] px-4 py-2.5 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50';

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function ResultBlock({ value }: { value: unknown }) {
  if (value === null || value === undefined) return null;
  return (
    <pre className="mt-4 max-h-72 overflow-auto rounded-[9px] border border-white/[0.08] bg-black/20 p-4 text-xs leading-5 text-zinc-300">
      {JSON.stringify(value, null, 2)}
    </pre>
  );
}

function StoryTool() {
  const [genres, setGenres] = useState<StoryGenre[]>([]);
  const [title, setTitle] = useState('');
  const [genre, setGenre] = useState('general');
  const [bookId, setBookId] = useState('');
  const [status, setStatus] = useState('idle');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    getStoryGenres().then((value) => {
      setGenres(value.genres ?? []);
      if (value.genres?.[0]?.id) setGenre(value.genres[0].id);
    }).catch((reason) => setError(errorText(reason)));
  }, []);

  useEffect(() => {
    if (!bookId || !['creating', 'running'].includes(status)) return;
    const timer = window.setInterval(() => {
      getStoryCreateStatus(bookId).then((value) => {
        const next = typeof value.status === 'string' ? value.status : 'unknown';
        setStatus(next);
        setBusy(['creating', 'running'].includes(next));
      }).catch((reason) => {
        setError(errorText(reason));
        setBusy(false);
      });
    }, 3000);
    return () => window.clearInterval(timer);
  }, [bookId, status]);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError('');
    try {
      const result = await createStoryProject({ title, genre });
      setBookId(result.bookId);
      setStatus(result.status);
    } catch (reason) {
      setError(errorText(reason));
      setBusy(false);
    }
  };

  return (
    <section className="floword-card max-w-3xl p-6">
      <form className="grid gap-5 sm:grid-cols-2" onSubmit={submit}>
        <label className="sm:col-span-2"><span className="mb-2 block text-xs font-medium text-zinc-400">Title</span><input className={fieldClass} value={title} onChange={(event) => setTitle(event.target.value)} required /></label>
        <label><span className="mb-2 block text-xs font-medium text-zinc-400">Genre</span><select className={fieldClass} value={genre} onChange={(event) => setGenre(event.target.value)}>{genres.length ? genres.map((item) => <option key={item.id} value={item.id}>{item.name || item.id}</option>) : <option value="general">general</option>}</select></label>
        <div className="flex items-end"><button type="submit" className={buttonClass} disabled={busy || !title.trim()}>{busy ? 'Creating…' : 'Create Project'}</button></div>
      </form>
      {bookId && <ResultBlock value={{ status, result: { projectId: bookId }, artifacts: [] }} />}
      {error && <p role="alert" className="mt-4 text-sm text-red-400">{error}</p>}
    </section>
  );
}

function VideoTool() {
  const [path, setPath] = useState('');
  const [probe, setProbe] = useState<VideoProbe | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const choose = async () => {
    const selected = await open({ multiple: false, filters: [{ name: 'Video', extensions: ['mp4', 'mov', 'mkv', 'webm', 'avi'] }] });
    if (typeof selected === 'string') setPath(selected);
  };
  const analyze = async () => {
    setBusy(true); setError(''); setProbe(null);
    try { setProbe((await probeVideo(path)).probe); } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); }
  };
  return (
    <section className="floword-card max-w-3xl p-6">
      <div className="flex flex-col gap-3 sm:flex-row"><input className={fieldClass} value={path} readOnly placeholder="Select a local video" /><button type="button" className="rounded-[9px] border border-white/10 px-4 py-2.5 text-sm text-zinc-200" onClick={choose}>Select Video</button><button type="button" className={buttonClass} onClick={analyze} disabled={!path || busy}>{busy ? 'Analyzing…' : 'Analyze'}</button></div>
      {probe && <ResultBlock value={{ status: 'completed', result: { filename: path.split(/[\\/]/).pop(), ...probe }, artifacts: [] }} />}
      {error && <p role="alert" className="mt-4 text-sm text-red-400">{error}</p>}
    </section>
  );
}

function ImageTool() {
  const [inputPath, setInputPath] = useState('');
  const [result, setResult] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const choose = async () => {
    const selected = await open({ multiple: false, filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'webp', 'tiff'] }] });
    if (typeof selected === 'string') setInputPath(selected);
  };
  const run = async () => {
    const outputPath = await save({ defaultPath: 'floword-rotated.png', filters: [{ name: 'PNG image', extensions: ['png'] }] });
    if (!outputPath) return;
    setBusy(true); setError(''); setResult(null);
    try { setResult(await flipImageFile(inputPath, outputPath)); } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); }
  };
  return (
    <section className="floword-card max-w-3xl p-6">
      <p className="mb-4 text-sm leading-6 text-zinc-400">Runs the shared local Image Editor transform: rotate 180° and export a real PNG file.</p>
      <div className="flex flex-col gap-3 sm:flex-row"><input className={fieldClass} value={inputPath} readOnly placeholder="Select a local image" /><button type="button" className="rounded-[9px] border border-white/10 px-4 py-2.5 text-sm text-zinc-200" onClick={choose}>Select Image</button><button type="button" className={buttonClass} onClick={run} disabled={!inputPath || busy}>{busy ? 'Processing…' : 'Rotate & Export'}</button></div>
      <ResultBlock value={result} />
      {error && <p role="alert" className="mt-4 text-sm text-red-400">{error}</p>}
    </section>
  );
}

function ResearchSessionTool({
  initialPlatform,
  initialVariant,
  onVariantChange,
}: {
  initialPlatform?: string;
  initialVariant?: 'mainland' | 'international';
  onVariantChange?: (variant: 'mainland' | 'international') => void;
}) {
  const [platforms, setPlatforms] = useState<ResearchPlatform[]>([]);
  const [platform, setPlatform] = useState(initialPlatform || 'xhs');
  const [variant, setVariant] = useState<'mainland' | 'international'>(initialVariant || 'mainland');
  const [authMethod, setAuthMethod] = useState<ResearchSession['auth_method']>('browser');
  const [session, setSession] = useState<ResearchSession | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    getResearchPlatforms().then((value) => {
      const list = Array.isArray(value) ? value : value.platforms;
      setPlatforms(list ?? []);
      const first = list?.[0]?.value ?? list?.[0]?.id;
      if (first) setPlatform(first);
    }).catch((reason) => setError(errorText(reason)));
  }, []);
  const refresh = async (quiet = false) => {
    if (!quiet) setBusy(true);
    setError('');
    try {
      const activeVariant = platform === 'xhs' ? variant : undefined;
      const current = await getResearchSessionStatus(platform, activeVariant);
      setSession(current);
      if (!quiet && current.status === 'CONNECTED') setSession(await verifyResearchSession(platform, current.auth_method, activeVariant));
    } catch (reason) { setError(errorText(reason)); } finally { if (!quiet) setBusy(false); }
  };
  useEffect(() => { void refresh(); }, [platform, variant]);
  useEffect(() => {
    if (session?.status !== 'AWAITING_LOGIN' && session?.status !== 'CONNECTING') return undefined;
    const timer = window.setInterval(() => { void refresh(true); }, 2000);
    return () => window.clearInterval(timer);
  }, [platform, variant, session?.status]);
  const login = async () => {
    setBusy(true); setError('');
    try { 
      const activeVariant = platform === 'xhs' ? variant : undefined;
      setSession(await loginResearchSession(platform, authMethod, activeVariant)); 
    } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); }
  };
  const verify = async () => {
    setBusy(true); setError('');
    try { 
      const activeVariant = platform === 'xhs' ? variant : undefined;
      setSession(await verifyResearchSession(platform, authMethod, activeVariant)); 
    } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); }
  };
  const reconnect = async () => {
    setBusy(true); setError('');
    try { 
      const activeVariant = platform === 'xhs' ? variant : undefined;
      setSession(await reconnectResearchSession(platform, activeVariant)); 
    } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); }
  };
  const clear = async () => {
    setBusy(true); setError('');
    try { 
      const activeVariant = platform === 'xhs' ? variant : undefined;
      setSession(await clearResearchSession(platform, activeVariant)); 
    } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); }
  };
  const connected = session?.status === 'CONNECTED';
  return (
    <section className="floword-card max-w-3xl p-6">
      <div className="grid gap-4 sm:grid-cols-2">
        <div><label className="mb-1.5 block text-xs text-zinc-400">Platform</label><select className={fieldClass} value={platform} onChange={(event) => setPlatform(event.target.value)}>{platforms.length ? platforms.map((item) => { const value = item.value ?? item.id ?? ''; return <option key={value} value={value}>{item.label ?? item.name ?? value}</option>; }) : <option value="xhs">xhs</option>}</select></div>
        {platform === 'xhs' && (
          <div>
            <label className="mb-1.5 block text-xs text-zinc-400">XHS Variant</label>
            <select
              className={fieldClass}
              value={variant}
              onChange={(event) => {
                const next = event.target.value as 'mainland' | 'international';
                setVariant(next);
                onVariantChange?.(next);
              }}
            >
              <option value="mainland">Xiaohongshu Mainland</option>
              <option value="international">RedNote International</option>
            </select>
          </div>
        )}
        <div><label className="mb-1.5 block text-xs text-zinc-400">Authentication</label><select className={fieldClass} value={authMethod} onChange={(event) => setAuthMethod(event.target.value as ResearchSession['auth_method'])}><option value="browser">Browser Session</option><option value="qrcode">QR Code</option><option value="cookie">Cookie compatibility fallback</option></select></div>
      </div>
      <div className="mt-4 rounded-lg border border-white/[0.08] bg-black/20 p-4">
        <div className="flex items-center justify-between gap-3"><div><div className="text-sm font-medium text-white">Session</div><div className="mt-1 text-xs text-zinc-500">{session?.profile_id ?? `mediacrawler:${platform}${platform === 'xhs' && variant === 'international' ? ':international' : ''}`}</div></div><span className={connected ? 'text-sm text-green-400' : 'text-sm text-zinc-400'}>{connected ? '● Connected' : `○ ${session?.status ?? 'Loading'}`}</span></div>
        {session?.last_verified_at && <div className="mt-2 text-xs text-zinc-500">Last verified: {new Date(session.last_verified_at).toLocaleString()}</div>}
        {session?.error && <div className="mt-2 text-xs text-rose-400">{session.error.code}: {session.error.message}</div>}
      </div>
      <div className="mt-4 flex flex-wrap gap-2"><button type="button" className={buttonClass} onClick={login} disabled={busy}>{busy ? 'Working…' : 'Login'}</button><button type="button" className="rounded-[9px] border border-white/10 px-3 py-2 text-xs text-zinc-200" onClick={verify} disabled={busy}>Verify</button><button type="button" className="rounded-[9px] border border-white/10 px-3 py-2 text-xs text-zinc-200" onClick={reconnect} disabled={busy}>Reconnect</button><button type="button" className="rounded-[9px] border border-rose-500/30 px-3 py-2 text-xs text-rose-300" onClick={clear} disabled={busy}>Clear local session</button></div>
      <p className="mt-3 text-xs leading-5 text-zinc-500">Browser/QR uses MediaCrawler's canonical CDP profile. Clear removes only local authentication state; it does not claim remote logout. Phone login remains unavailable because the core requires its external Redis SMS-code flow.</p>
      {error && <p role="alert" className="mt-4 text-sm text-red-400">{error}</p>}
    </section>
  );
}

function MediaTool() {
  const [query, setQuery] = useState('');
  const [videos, setVideos] = useState<YouTubeSearchVideo[]>([]);
  const [pipelines, setPipelines] = useState<MontagePipeline[]>([]);
  const [projectId, setProjectId] = useState('');
  const [title, setTitle] = useState('');
  const [pipeline, setPipeline] = useState('framework-smoke');
  const [montageResult, setMontageResult] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => { getMontagePipelines().then((value) => setPipelines(value.pipelines)).catch(() => undefined); }, []);
  const search = async (event: React.FormEvent) => { event.preventDefault(); setBusy(true); setError(''); try { setVideos((await searchYouTube(query, 10)).videos); } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); } };
  const createMontage = async (event: React.FormEvent) => { event.preventDefault(); setBusy(true); setError(''); try { const value = await createMontageProject({ projectId, title, pipelineType: pipeline }); setMontageResult({ status: 'completed', result: value, artifacts: [] }); } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); } };
  return (
    <div className="grid gap-5 xl:grid-cols-2">
      <section className="floword-card p-6"><h2 className="text-sm font-semibold text-white">YouTube Search</h2><form className="mt-4 flex gap-3" onSubmit={search}><input className={fieldClass} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search videos" required /><button className={buttonClass} disabled={busy}>Search</button></form><div className="mt-4 divide-y divide-white/[0.06]">{videos.map((video) => <a key={video.id} href={video.url} target="_blank" rel="noreferrer" className="block py-3"><div className="text-sm font-medium text-white">{video.title}</div><div className="mt-1 text-xs text-zinc-500">{video.channel || video.id}</div></a>)}</div></section>
      <section className="floword-card p-6"><h2 className="text-sm font-semibold text-white">Montage Project</h2><form className="mt-4 grid gap-3" onSubmit={createMontage}><input className={fieldClass} value={projectId} onChange={(event) => setProjectId(event.target.value)} placeholder="project-id (kebab-case)" required /><input className={fieldClass} value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Project title" required /><select className={fieldClass} value={pipeline} onChange={(event) => setPipeline(event.target.value)}>{pipelines.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}</select><button className={buttonClass} disabled={busy}>Create Project</button></form><ResultBlock value={montageResult} /></section>
      {error && <p role="alert" className="text-sm text-red-400 xl:col-span-2">{error}</p>}
    </div>
  );
}

function ProvidersTool() {
  const [providers, setProviders] = useState<Array<{ id: string; name: string; status: string }>>([]);
  const [modelCount, setModelCount] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const refresh = async () => {
    setBusy(true); setError('');
    try {
      const [raw, models] = await Promise.all([getAiProviders(), getAiModels()]);
      const record = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
      const list = Array.isArray(raw) ? raw : Array.isArray(record.providers) ? record.providers : Array.isArray(record.data) ? record.data : [];
      setProviders(list.flatMap((item) => {
        if (!item || typeof item !== 'object') return [];
        const value = item as Record<string, unknown>;
        const id = String(value.id ?? value.provider_id ?? value.name ?? 'provider');
        return [{ id, name: String(value.name ?? value.label ?? id), status: String(value.status ?? (value.enabled === false ? 'disabled' : 'configured')) }];
      }));
      setModelCount(models.data.length);
    } catch (reason) { setError(errorText(reason)); } finally { setBusy(false); }
  };
  useEffect(() => { void refresh(); }, []);
  const summary = useMemo(() => ({ providers: providers.length, models: modelCount }), [providers.length, modelCount]);
  return (
    <section className="floword-card max-w-4xl overflow-hidden"><div className="flex items-center justify-between border-b border-white/[0.08] px-5 py-4"><div><h2 className="text-sm font-semibold text-white">OmniRoute Providers</h2><p className="mt-1 text-xs text-zinc-500">{summary.providers} providers · {summary.models} models. Credentials are never displayed.</p></div><button type="button" className="rounded-[9px] border border-white/10 px-3 py-2 text-xs text-zinc-200" onClick={refresh} disabled={busy}>{busy ? 'Loading…' : 'Refresh'}</button></div>{providers.map((provider) => <div key={provider.id} className="flex items-center justify-between gap-4 border-b border-white/[0.06] px-5 py-4 last:border-0"><div><div className="text-sm font-medium text-white">{provider.name}</div><div className="mt-1 font-mono text-xs text-zinc-500">{provider.id}</div></div><span className="rounded-full bg-white/[0.05] px-2.5 py-1 text-xs text-zinc-400">{provider.status}</span></div>)}{!busy && providers.length === 0 && !error && <div className="p-8 text-center text-sm text-zinc-500">No provider records returned.</div>}{error && <p role="alert" className="p-5 text-sm text-red-400">{error}</p>}</section>
  );
}

export const CapabilityToolsView: React.FC<CapabilityToolsViewProps> = ({
  tool,
  researchPlatform,
  xhsVariant,
  onVariantChange,
}) => {
  if (tool === 'story') return <StoryTool />;
  if (tool === 'video') return <VideoTool />;
  if (tool === 'image') return <ImageTool />;
  if (tool === 'research') {
    return (
      <ResearchSessionTool
        initialPlatform={researchPlatform}
        initialVariant={xhsVariant}
        onVariantChange={onVariantChange}
      />
    );
  }
  if (tool === 'media') return <MediaTool />;
  return <ProvidersTool />;
};
