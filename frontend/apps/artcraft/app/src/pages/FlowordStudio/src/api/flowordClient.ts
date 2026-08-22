// Floword Studio backend client.
//
// Contract: React → Tauri command → Rust → SQLite / OmniRoute / CapCut Mate.
// Workflow commands use Tauri/Rust; capability discovery uses one configurable
// HTTP gateway. The frontend never calls service-specific ports and never
// fabricates models, readiness, or job ids.

import { readFile, writeFile } from '@tauri-apps/plugin-fs';

// ---------------------------------------------------------------------------
// Command envelope + typed error
// ---------------------------------------------------------------------------

interface CommandSuccess<T> {
  status: 'success';
  payload: T;
}

interface CommandFailure {
  status: 'bad_request' | 'not_found' | 'server_error' | 'unauthorized' | 'too_many_requests';
  error_message?: string;
  error_details?: { error_code?: string; job_id?: string } | null;
}

/// Typed error carrying the backend's structured `error_code`
/// (e.g. WORKFLOW_NOT_FOUND, OMNIROUTE_UNAVAILABLE, INTERNAL_ERROR).
export class FlowordCommandError extends Error {
  readonly errorCode?: string;
  readonly jobId?: string;

  constructor(errorCode: string | undefined, message: string, jobId?: string) {
    super(message);
    this.name = 'FlowordCommandError';
    this.errorCode = errorCode;
    this.jobId = jobId;
  }
}

function toFlowordError(raw: unknown): FlowordCommandError {
  if (raw instanceof FlowordCommandError) return raw;
  if (raw && typeof raw === 'object') {
    const obj = raw as CommandFailure & { error_code?: string; job_id?: string; message?: string };
    const code = obj.error_details?.error_code ?? obj.error_code;
    const jobId = obj.error_details?.job_id ?? obj.job_id;
    const msg = obj.error_message ?? obj.message ?? 'Command failed';
    return new FlowordCommandError(code, msg, jobId);
  }
  return new FlowordCommandError(undefined, typeof raw === 'string' ? raw : String(raw));
}

function getTauriInvoke(): ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: { invoke?: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> } }).__TAURI_INTERNALS__;
  return internals?.invoke ?? null;
}

/// Invoke a Tauri command and unwrap the `{status:"success", payload}` envelope.
/// Rejections (Rust `Err`) and non-success envelopes throw a `FlowordCommandError`.
async function invokeCommand<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const invoke = getTauriInvoke();
  if (!invoke) {
    throw new FlowordCommandError('TAURI_UNAVAILABLE', 'Tauri runtime not available (desktop app only)');
  }

  let raw: unknown;
  try {
    raw = await invoke(cmd, args);
  } catch (err) {
    throw toFlowordError(err);
  }

  if (raw && typeof raw === 'object' && 'status' in raw) {
    const env = raw as CommandSuccess<T> | CommandFailure;
    if (env.status === 'success') {
      return (env as CommandSuccess<T>).payload;
    }
    throw toFlowordError(env);
  }
  // Defensive: our commands always wrap, but tolerate a bare payload.
  return raw as T;
}

// ---------------------------------------------------------------------------
// Workflow commands (job_id is the ONLY identifier the frontend polls with)
// ---------------------------------------------------------------------------

export interface EnqueueFlowordWorkflowRequest {
  page_id?: string;
  workflow_name: string;
  prompt: string;
  topic?: string;
  source_urls?: string[];
  source_files?: string[];
  workflow_mode?: string;
  image_prompt?: string;
  expand_9_16_prompt?: string;
  expand_prompt?: string;
  video_prompt?: string;
  source_image_artifact?: unknown;
  target_platform?: string;
  aspect_ratio?: string;
  target_duration_seconds?: number;
  output_mode?: string;
  model_id?: string;
  voice_id?: string;
  language?: string;
  content_source?: string;
  story_url?: string;
  research_enabled?: boolean;
  research_platform?: string;
  research_query?: string;
  research_mode?: string;
  xhs_variant?: string;
  cookie_mode?: string;
  cookie_browser?: string;
  cookie_browser_profile?: string;
  cookie_file_path?: string;
  cookie_skip_patterns?: string[];
  title?: string;
  caption?: string;
  hashtags?: string[];
  description?: string;
  publish_platforms?: string[];
  post_mode?: string;
  schedule_time?: string;
  custom_filename?: string;
}

export interface EnqueueFlowordWorkflowResponse {
  job_id: string;
  workflow_id?: string | null;
  status: string;
}

export interface GetFlowordWorkflowResponse {
  job_id: string;
  page_id?: string | null;
  page_snapshot?: Record<string, unknown> | null;
  status: string;
  business_status: string;
  current_stage: string;
  input_payload?: Record<string, unknown> | null;
  failure_message?: string | null;
  failure_code?: string | null;
  failure_stage?: string | null;
  stage_outputs?: string | null;
  stage_states?: FlowordStageState[];
  created_at: number;
  started_at?: number | null;
  completed_at?: number | null;
}

export type FlowordStageStatus = 'pending' | 'running' | 'waiting_input' | 'retrying' | 'completed' | 'skipped' | 'failed' | 'cancelled';

export interface FlowordStageError {
  code: string;
  message: string;
  retryable: boolean;
  service?: string | null;
  stage_id?: string | null;
  timestamp?: string | null;
}

export interface FlowordStageState {
  stage_id: string;
  status: FlowordStageStatus;
  attempt: number;
  started_at?: string | null;
  finished_at?: string | null;
  service?: string | null;
  error?: FlowordStageError | null;
}

export interface CancelFlowordWorkflowResponse {
  cancelled: boolean;
  had_live_token: boolean;
}

export interface RetryFlowordStepResponse {
  retried: boolean;
  job_id: string;
  resumed_stage: string;
  step_retry_count: number;
}

export interface BrowserWorkerInfo {
  worker_id: string;
  profile_id?: string | null;
  profile_name?: string | null;
  state: string;
  has_extension: boolean;
  grok_logged_in: boolean;
  last_heartbeat_at?: string | null;
}

export interface ArtifactRef {
  artifact_id: string;
  kind: string;
  location: string;
  mime_type: string;
  size_bytes: number;
  sha256: string;
  created_at: string;
  metadata: Record<string, unknown>;
}

export interface IngestFlowordSourceImageRequest {
  file_path?: string;
  base64_data?: string;
  file_name?: string;
  page_id?: string;
}

export interface IngestFlowordSourceImageResponse {
  artifact: ArtifactRef;
  preview_url: string;
}

export interface FlowordSettingsResponse {
  max_concurrent_jobs: number;
}

export interface PipelineJobEvent {
  id: string;
  job_id: string;
  sequence: number;
  stage_id?: string | null;
  business_status?: string | null;
  event_type: string;
  level: string;
  message: string;
  error_code?: string | null;
  metadata_json?: string | null;
  created_at: number;
}

export function enqueueFlowordWorkflow(request: EnqueueFlowordWorkflowRequest): Promise<EnqueueFlowordWorkflowResponse> {
  return invokeCommand<EnqueueFlowordWorkflowResponse>('enqueue_floword_workflow', { request });
}

export function getFlowordWorkflow(jobId: string): Promise<GetFlowordWorkflowResponse> {
  return invokeCommand<GetFlowordWorkflowResponse>('get_floword_workflow', { request: { job_id: jobId } });
}

export interface ListFlowordWorkflowsResponse {
  workflows: GetFlowordWorkflowResponse[];
}

export function listFlowordWorkflows(): Promise<GetFlowordWorkflowResponse[]> {
  return invokeCommand<ListFlowordWorkflowsResponse>('list_floword_workflows').then(
    (res) => res.workflows ?? []
  );
}

export function cancelFlowordWorkflow(jobId: string): Promise<CancelFlowordWorkflowResponse> {
  return invokeCommand<CancelFlowordWorkflowResponse>('cancel_floword_workflow', { request: { job_id: jobId } });
}

export function retryFlowordStep(jobId: string, stepId: string): Promise<RetryFlowordStepResponse> {
  return invokeCommand<RetryFlowordStepResponse>('retry_floword_step', { request: { job_id: jobId, step_id: stepId } });
}

export function retryFlowordJobFromStart(jobId: string): Promise<RetryFlowordStepResponse> {
  return invokeCommand<RetryFlowordStepResponse>('retry_floword_job_from_start', { request: { job_id: jobId } });
}

export function skipFlowordResearch(jobId: string): Promise<RetryFlowordStepResponse> {
  return invokeCommand<RetryFlowordStepResponse>('skip_floword_research', { request: { job_id: jobId } });
}

export function listBrowserWorkers(): Promise<BrowserWorkerInfo[]> {
  return invokeCommand<{ workers: BrowserWorkerInfo[] }>('list_browser_workers_command').then(
    (res) => res.workers ?? []
  );
}

// ---------------------------------------------------------------------------
// Donut Profile Catalog -- persistent browser identities
// ---------------------------------------------------------------------------

// Persistent Donut browser profile. Mirrors Donut's ApiProfile.
// Always available regardless of whether the browser process is running.
export interface DonutProfileInfo {
  id: string;
  name: string;
  browser: string;
  is_running: boolean;
  process_id?: number | null;
  tags: string[];
  group_id?: string | null;
  last_launch?: number | null;
  proxy_id?: string | null;
  vpn_id?: string | null;
  sync_mode: string;
  cloud_sync_enabled: boolean;
}

// DonutProfileInfo enriched with runtime worker state after joining
// profiles[] with workers[] on profile.id === worker.profile_id.
export interface DonutProfileEnriched extends DonutProfileInfo {
  worker_id?: string;
  worker_state?: string;
  extension_ready?: boolean;
  grok_logged_in?: boolean;
}

// Fetch the full Donut Profile Catalog. Returns all profiles (online + offline).
export function listDonutProfiles(): Promise<DonutProfileInfo[]> {
  return invokeCommand<{ profiles: DonutProfileInfo[] }>('list_donut_profiles_command').then(
    (res) => res.profiles ?? []
  );
}

// Convenience: fetch profiles + workers in parallel, join, return enriched list.
export async function listDonutProfilesEnriched(): Promise<DonutProfileEnriched[]> {
  const [profiles, workers] = await Promise.all([
    listDonutProfiles().catch((): DonutProfileInfo[] => []),
    listBrowserWorkers().catch((): BrowserWorkerInfo[] => []),
  ]);
  return profiles.map((p) => {
    const w = workers.find((wk) => wk.profile_id === p.id);
    return {
      ...p,
      worker_id: w?.worker_id,
      worker_state: w?.state,
      extension_ready: w?.has_extension ?? false,
      grok_logged_in: w?.grok_logged_in ?? false,
    };
  });
}

export function ingestFlowordSourceImage(request: IngestFlowordSourceImageRequest): Promise<IngestFlowordSourceImageResponse> {
  return invokeCommand<IngestFlowordSourceImageResponse>('ingest_floword_source_image_command', { request });
}

export function getFlowordSettings(): Promise<FlowordSettingsResponse> {
  return invokeCommand<FlowordSettingsResponse>('get_floword_settings_command');
}

export function updateFlowordSettings(maxConcurrentJobs: number): Promise<FlowordSettingsResponse> {
  return invokeCommand<FlowordSettingsResponse>('update_floword_settings_command', { request: { max_concurrent_jobs: maxConcurrentJobs } });
}

export function listPipelineJobEvents(jobId: string): Promise<PipelineJobEvent[]> {
  return invokeCommand<{ events: PipelineJobEvent[] }>('list_pipeline_job_events_command', { request: { job_id: jobId } }).then(
    (res) => res.events ?? []
  );
}

// Existing ArtCraft Tauri image action; no gateway or task lifecycle needed.
export function flipImage(imageBase64: string): Promise<string> {
  return invokeCommand<string>('flip_image', { image: imageBase64 });
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }
  return window.btoa(binary);
}

function base64ToBytes(value: string): Uint8Array {
  const binary = window.atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

export interface ImageTransformResult {
  status: 'completed';
  result: { format: 'png'; sizeBytes: number };
  artifacts: Array<{ path: string; type: 'image/png' }>;
}

export async function flipImageFile(inputPath: string, outputPath: string): Promise<ImageTransformResult> {
  const input = await readFile(inputPath);
  const output = base64ToBytes(await flipImage(bytesToBase64(input)));
  await writeFile(outputPath, output);
  return {
    status: 'completed',
    result: { format: 'png', sizeBytes: output.byteLength },
    artifacts: [{ path: outputPath, type: 'image/png' }],
  };
}

// ---------------------------------------------------------------------------
// OmniRoute models (via Tauri only — no direct :20128 call, no fake models)
// ---------------------------------------------------------------------------

export interface OmniRouteModel {
  id: string;
  provider?: string;
}

interface ListOmniRouteModelsResponse {
  models: OmniRouteModel[];
}

/// Fetch models from the backend OmniRoute client. On failure returns an empty
/// list — the caller must render an "unavailable" state, never a hard-coded model.
export async function fetchOmniRouteModels(): Promise<OmniRouteModel[]> {
  try {
    const res = await invokeCommand<ListOmniRouteModelsResponse>('list_omniroute_models');
    return res.models ?? [];
  } catch {
    return [];
  }
}

export type VisualProviderStatus = 'configured' | 'not_configured' | 'invalid';

export interface FlowordVisualProvider {
  provider: string;
  status: VisualProviderStatus;
  capabilities: string[];
  credential_type: string;
  credential_source: string;
  auth_method: string;
  oauth_supported: boolean;
  api_key_supported: boolean;
  message: string;
}

export function getFlowordVisualProvider(): Promise<FlowordVisualProvider> {
  return invokeCommand<FlowordVisualProvider>('get_floword_visual_provider');
}

export function testFlowordVisualProvider(): Promise<FlowordVisualProvider> {
  return invokeCommand<FlowordVisualProvider>('test_floword_visual_provider');
}

// ---------------------------------------------------------------------------
// Readiness (backend-probed; no hard-coded READY)
// ---------------------------------------------------------------------------

export type ServiceStatusState = 'READY' | 'DEGRADED' | 'UNAVAILABLE' | 'AUTH_REQUIRED' | 'WAITING_INPUT';

export interface ServiceHealth {
  name: string;
  status: ServiceStatusState;
  endpoint: string;
  lastChecked: string;
  latencyMs: number;
  message: string;
  errorCode?: string;
}

export interface DetailedReadinessStatus {
  mateAgent: ServiceHealth;
  omniRoute: ServiceHealth;
  mediaCrawler: ServiceHealth;
  openMontage: ServiceHealth;
  playwrightCdp: ServiceHealth;
  storage: ServiceHealth;
  capCutRender: ServiceHealth;
  isReadyForExecution: boolean;
}

interface BackendServiceReadiness {
  id: string;
  status: string; // ready | degraded | unavailable | auth_required | waiting_input
  latency_ms?: number;
  error_code?: string | null;
  message?: string | null;
}

interface BackendReadinessResponse {
  services: BackendServiceReadiness[];
  is_ready_for_execution: boolean;
}

function blankHealth(name: string): ServiceHealth {
  return { name, status: 'UNAVAILABLE', endpoint: '', lastChecked: '', latencyMs: 0, message: 'No data' };
}

/// The default readiness before the first backend probe returns: everything
/// unavailable, execution blocked. No optimistic READY.
export const DEFAULT_READINESS: DetailedReadinessStatus = {
  mateAgent: blankHealth('CapCut Mate'),
  omniRoute: blankHealth('OmniRoute LLM Gateway'),
  mediaCrawler: blankHealth('MediaCrawler'),
  openMontage: blankHealth('OpenMontage'),
  playwrightCdp: blankHealth('Playwright / CDP'),
  storage: blankHealth('ArtifactStore'),
  capCutRender: blankHealth('CapCut Render'),
  isReadyForExecution: false,
};

function mapStatus(raw: string): ServiceStatusState {
  switch (raw) {
    case 'ready':
      return 'READY';
    case 'degraded':
      return 'DEGRADED';
    case 'auth_required':
      return 'AUTH_REQUIRED';
    case 'waiting_input':
      return 'WAITING_INPUT';
    default:
      return 'UNAVAILABLE';
  }
}

function toHealth(name: string, svc: BackendServiceReadiness | undefined): ServiceHealth {
  if (!svc) return blankHealth(name);
  return {
    name,
    status: mapStatus(svc.status),
    endpoint: svc.id,
    lastChecked: new Date().toLocaleTimeString(),
    latencyMs: svc.latency_ms ?? 0,
    message: svc.message ?? '',
    errorCode: svc.error_code ?? undefined,
  };
}

/// Probe backend readiness via Tauri. On failure returns DEFAULT_READINESS
/// (all unavailable) so the UI blocks execution rather than showing a false READY.
export async function fetchDetailedReadiness(): Promise<DetailedReadinessStatus> {
  let res: BackendReadinessResponse;
  try {
    res = await invokeCommand<BackendReadinessResponse>('get_floword_readiness');
  } catch {
    return DEFAULT_READINESS;
  }

  const byId = new Map(res.services.map((s) => [s.id, s]));
  return {
    mateAgent: toHealth('CapCut Mate', byId.get('capcut')),
    omniRoute: toHealth('OmniRoute LLM Gateway', byId.get('omniroute')),
    mediaCrawler: toHealth('MediaCrawler', byId.get('mediacrawler')),
    openMontage: toHealth('OpenMontage', byId.get('openmontage')),
    playwrightCdp: toHealth('Playwright / CDP', byId.get('playwright_sidecar') ?? byId.get('chrome_cdp')),
    storage: toHealth('ArtifactStore', byId.get('storage')),
    capCutRender: toHealth('CapCut Render', byId.get('capcut')),
    isReadyForExecution: res.is_ready_for_execution,
  };
}

// ---------------------------------------------------------------------------
// Shared backend gateway (the only HTTP base URL Floword knows)
// ---------------------------------------------------------------------------

const BACKEND_BASE_URL = (
  (import.meta as unknown as { env?: Record<string, string> }).env?.VITE_BACKEND_BASE_URL?.trim() || 'http://127.0.0.1:30000'
).replace(/\/+$/, '');

interface GatewayErrorBody {
  detail?: string;
  error?: {
    code?: string;
    message?: string;
    service?: string;
  };
}

async function gatewayRequest<T>(
  path: string,
  options: { method?: 'GET' | 'POST'; body?: unknown; timeoutMs?: number } = {},
): Promise<T> {
  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort(), options.timeoutMs ?? 10_000);
  let response: Response;
  try {
    response = await fetch(`${BACKEND_BASE_URL}${path}`, {
      method: options.method ?? 'GET',
      headers: {
        Accept: 'application/json',
        ...(options.body === undefined ? {} : { 'Content-Type': 'application/json' }),
      },
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      signal: controller.signal,
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') {
      throw new FlowordCommandError('GATEWAY_TIMEOUT', 'Backend gateway request timed out');
    }
    throw new FlowordCommandError('GATEWAY_UNAVAILABLE', 'Backend gateway is unavailable');
  } finally {
    window.clearTimeout(timeoutId);
  }

  if (!response.ok) {
    let body: GatewayErrorBody = {};
    try {
      body = (await response.json()) as GatewayErrorBody;
    } catch {
      // The HTTP status remains the fallback error detail.
    }
    throw new FlowordCommandError(
      body.error?.code,
      body.error?.message ?? body.detail ?? `Backend gateway request failed (${response.status})`,
    );
  }

  return response.json() as Promise<T>;
}

export type GatewayServiceStatus = 'ready' | 'degraded' | 'offline' | 'not_configured' | 'error';

export interface GatewayService {
  id: string;
  name: string;
  category: string;
  status: GatewayServiceStatus;
  enabled: boolean;
  health: Record<string, unknown>;
  capabilities: string[];
  uiMode?: 'separate';
}

export interface SystemHealthResponse {
  status: 'ready' | 'degraded';
  gateway: 'ready';
  services: { total: number; ready: number; unhealthy: number };
}

export interface ServicesResponse {
  services: GatewayService[];
}

export interface AiHealthResponse {
  status: GatewayServiceStatus;
  status_code?: number;
  error?: { code: string; message: string; service: string };
}

export interface OmniModel {
  id: string;
  object?: string;
  owned_by?: string;
}

export interface ModelsResponse {
  data: OmniModel[];
}

export const getSystemHealth = (): Promise<SystemHealthResponse> =>
  gatewayRequest<SystemHealthResponse>('/api/system/health');

export const getServices = (): Promise<ServicesResponse> =>
  gatewayRequest<ServicesResponse>('/api/services');

export const getAiModels = (): Promise<ModelsResponse> =>
  gatewayRequest<ModelsResponse>('/api/ai/models');

export const getAiHealth = (): Promise<AiHealthResponse> =>
  gatewayRequest<AiHealthResponse>('/api/ai/health');

export const getAiProviders = (): Promise<unknown> =>
  gatewayRequest('/api/ai/providers', { timeoutMs: 30_000 });

// ---------------------------------------------------------------------------
// Capability APIs (normalized gateway routes; no service-specific ports)
// ---------------------------------------------------------------------------

export interface ResearchPlatform {
  id?: string;
  value?: string;
  name?: string;
  label?: string;
  icon?: string;
}

export interface ResearchCapabilityOption extends ResearchPlatform {
  value: string;
  label: string;
}

interface ResearchPlatformsResponse {
  platforms: ResearchCapabilityOption[];
}

interface ResearchOptionsResponse {
  crawler_types: ResearchCapabilityOption[];
}

export interface ResearchCapabilities {
  platforms: ResearchCapabilityOption[];
  modes: ResearchCapabilityOption[];
}

export interface CrawlerStatus {
  status: string;
  platform?: string | null;
  [key: string]: unknown;
}

/** Load the catalogs owned by MediaCrawler; React carries no duplicate option list. */
export const getResearchCapabilities = async (): Promise<ResearchCapabilities> => {
  const [platforms, options] = await Promise.all([
    gatewayRequest<ResearchPlatformsResponse>('/api/config/platforms'),
    gatewayRequest<ResearchOptionsResponse>('/api/config/options'),
  ]);
  return {
    platforms: platforms.platforms ?? [],
    modes: options.crawler_types ?? [],
  };
};

/** Compatibility surface for the Configure session tool, backed by the same real catalog. */
export const getResearchPlatforms = (): Promise<ResearchPlatformsResponse> =>
  gatewayRequest('/api/config/platforms');

export const startResearchCrawler = (request: Record<string, unknown>): Promise<CrawlerStatus> =>
  gatewayRequest('/api/research/crawler/start', { method: 'POST', body: request, timeoutMs: 30_000 });

export const getResearchCrawlerStatus = (): Promise<CrawlerStatus> =>
  gatewayRequest('/api/research/crawler/status');

export const stopResearchCrawler = (): Promise<Record<string, unknown>> =>
  gatewayRequest('/api/research/crawler/stop', { method: 'POST' });

export type ResearchSessionStatusValue = 'DISCONNECTED' | 'CONNECTING' | 'AWAITING_LOGIN' | 'CONNECTED' | 'EXPIRED' | 'INVALID' | 'ERROR';

export interface ResearchSession {
  platform: string;
  variant?: string | null;
  auth_method: 'browser' | 'qrcode' | 'cookie';
  profile_id: string;
  status: ResearchSessionStatusValue;
  last_verified_at?: string | null;
  error?: { code: string; message: string } | null;
}

export const getResearchSessionStatus = (platform: string, variant?: string): Promise<ResearchSession> => {
  const query = new URLSearchParams({ platform });
  if (variant) query.set('variant', variant);
  return gatewayRequest(`/api/research/session/status?${query.toString()}`);
};

export const loginResearchSession = (platform: string, authMethod: ResearchSession['auth_method'], variant?: string): Promise<ResearchSession> =>
  gatewayRequest('/api/research/session/login', { method: 'POST', body: { platform, auth_method: authMethod, variant }, timeoutMs: 30_000 });

export const verifyResearchSession = (platform: string, authMethod?: ResearchSession['auth_method'], variant?: string): Promise<ResearchSession> =>
  gatewayRequest('/api/research/session/verify', { method: 'POST', body: { platform, auth_method: authMethod, variant }, timeoutMs: 90_000 });

export const reconnectResearchSession = (platform: string, variant?: string): Promise<ResearchSession> =>
  gatewayRequest('/api/research/session/reconnect', { method: 'POST', body: { platform, variant }, timeoutMs: 90_000 });

export const clearResearchSession = (platform: string, variant?: string): Promise<ResearchSession> =>
  gatewayRequest('/api/research/session/clear', { method: 'POST', body: { platform, variant }, timeoutMs: 30_000 });

export interface YouTubeSearchVideo {
  id: string;
  title: string;
  url: string;
  channel?: string;
  thumbnail?: string;
  duration?: string;
}

export interface YouTubeSearchResponse {
  videos: YouTubeSearchVideo[];
  continuation?: string;
}

export const searchYouTube = (query: string, limit = 10): Promise<YouTubeSearchResponse> =>
  gatewayRequest('/api/media/youtube/search', {
    method: 'POST',
    body: { query, limit },
    timeoutMs: 90_000,
  });

export type VideoPlanStrategy = 'single' | 'concat' | 'batch' | 'series';

export interface VideoPlanRequest {
  sources: string[];
  strategy: VideoPlanStrategy;
  baseName?: string;
  seriesContext?: string | null;
  episodeTemplate?: string;
}

export interface VideoPlan {
  name: string;
  ordered_sources: string[];
  episode_number?: number | null;
  episode_label?: string | null;
  series_context?: string | null;
}

export const buildVideoPlans = (request: VideoPlanRequest): Promise<{ plans: VideoPlan[] }> =>
  gatewayRequest('/api/video/plans', { method: 'POST', body: request });

export interface VideoProbe {
  durationSeconds: number;
  width: number;
  height: number;
  videoCodec?: string | null;
  audioCodec?: string | null;
  sizeBytes: number;
}

export const probeVideo = (path: string): Promise<{ probe: VideoProbe }> =>
  gatewayRequest('/api/video/probe', { method: 'POST', body: { path }, timeoutMs: 30_000 });

export interface StoryGenre {
  id: string;
  name?: string;
  language?: string;
}

export const getStoryGenres = (): Promise<{ genres: StoryGenre[] }> =>
  gatewayRequest('/api/create/story/genres');

export const getStoryProjects = (): Promise<{ books: Record<string, unknown>[] }> =>
  gatewayRequest('/api/create/story/projects');

export interface CreateStoryProjectRequest {
  title: string;
  genre: string;
  language?: string;
  platform?: string;
  chapterWordCount?: number;
  targetChapters?: number;
  blurb?: string;
}

export interface CreateStoryProjectResponse {
  status: string;
  bookId: string;
}

export const createStoryProject = (request: CreateStoryProjectRequest): Promise<CreateStoryProjectResponse> =>
  gatewayRequest('/api/create/story/projects', { method: 'POST', body: request, timeoutMs: 30_000 });

export const getStoryCreateStatus = (bookId: string): Promise<Record<string, unknown>> =>
  gatewayRequest(`/api/create/story/projects/${encodeURIComponent(bookId)}/create-status`);

export interface MontagePipeline {
  value: string;
  label: string;
}

export const getMontagePipelines = (): Promise<{ pipelines: MontagePipeline[] }> =>
  gatewayRequest('/api/montage/pipelines');

export const getMontageProjects = (): Promise<{ projects: Record<string, unknown>[] }> =>
  gatewayRequest('/api/montage/projects');

export interface CreateMontageProjectRequest {
  projectId: string;
  title: string;
  pipelineType: string;
}

export const createMontageProject = (request: CreateMontageProjectRequest): Promise<Record<string, unknown>> =>
  gatewayRequest('/api/montage/projects', { method: 'POST', body: request });

// ---------------------------------------------------------------------------
// ContentPage Domain API
// ---------------------------------------------------------------------------

export interface ContentPage {
  id: string;
  name: string;
  slug: string;
  output_root: string;
  target_platform?: string | null;
  default_model_id?: string | null;
  default_workflow_id?: string | null;
  default_language?: string | null;
  default_tone?: string | null;
  default_aspect_ratio?: string | null;
  browser_profile_id?: string | null;
  worker_pool_id?: string | null;
  default_image_prompt?: string | null;
  default_expand_9_16_prompt?: string | null;
  default_video_prompt?: string | null;
  is_archived: boolean;
  created_at: number;
  updated_at: number;
}

export interface CreateContentPageRequest {
  id?: string;
  name: string;
  slug?: string;
  output_root: string;
  target_platform?: string;
  default_model_id?: string;
  default_workflow_id?: string;
  default_language?: string;
  default_tone?: string;
  default_aspect_ratio?: string;
  browser_profile_id?: string;
  worker_pool_id?: string;
  default_image_prompt?: string;
  default_expand_9_16_prompt?: string;
  default_video_prompt?: string;
}

export interface UpdateContentPageRequest {
  id: string;
  name: string;
  slug?: string;
  output_root: string;
  target_platform?: string;
  default_model_id?: string;
  default_workflow_id?: string;
  default_language?: string;
  default_tone?: string;
  default_aspect_ratio?: string;
  browser_profile_id?: string;
  worker_pool_id?: string;
  default_image_prompt?: string;
  default_expand_9_16_prompt?: string;
  default_video_prompt?: string;
}

export interface ResolveOutputPathResponse {
  output_directory: string;
  date_string: string;
  sanitized_page_name: string;
}

export async function listContentPages(includeArchived = false): Promise<ContentPage[]> {
  const res = await invokeCommand<{ pages: ContentPage[] }>('list_content_pages_command', {
    include_archived: includeArchived,
  });
  return res.pages;
}

export async function getContentPage(pageId: string): Promise<ContentPage> {
  const res = await invokeCommand<{ page: ContentPage }>('get_content_page_command', {
    page_id: pageId,
  });
  return res.page;
}

export async function createContentPage(request: CreateContentPageRequest): Promise<ContentPage> {
  const res = await invokeCommand<{ page: ContentPage }>('create_content_page_command', {
    request,
  });
  return res.page;
}

export async function updateContentPage(request: UpdateContentPageRequest): Promise<ContentPage> {
  const res = await invokeCommand<{ page: ContentPage }>('update_content_page_command', {
    request,
  });
  return res.page;
}

export async function archiveContentPage(pageId: string, isArchived = true): Promise<boolean> {
  const res = await invokeCommand<{ success: boolean }>('archive_content_page_command', {
    page_id: pageId,
    is_archived: isArchived,
  });
  return res.success;
}

export async function resolveFlowordOutputPath(outputRoot: string, pageName: string): Promise<ResolveOutputPathResponse> {
  return invokeCommand<ResolveOutputPathResponse>('resolve_floword_output_path_command', {
    output_root: outputRoot,
    page_name: pageName,
  });
}

// ---------------------------------------------------------------------------
// Publishing Engine API
// ---------------------------------------------------------------------------

export interface ContentPagePublishTarget {
  id: string;
  page_id: string;
  platform: 'facebook' | 'tiktok' | 'youtube';
  enabled: boolean;
  account_label?: string | null;
  destination_id: string;
  destination_handle?: string | null;
  browser_profile_id: string;
  post_mode: 'auto' | 'review';
  default_slots_json: string;
  created_at: number;
  updated_at: number;
}

export type PublicationStatus =
  | 'NOT_STARTED'
  | 'WAITING_APPROVAL'
  | 'SCHEDULED'
  | 'READY_TO_POST'
  | 'POSTING'
  | 'POSTED'
  | 'POST_ERROR'
  | 'AUTH_REQUIRED'
  | 'VERIFY_REQUIRED'
  | 'CANCELLED';

export interface JobPublication {
  id: string;
  job_id: string;
  page_id: string;
  platform: 'facebook' | 'tiktok' | 'youtube';
  target_config_id?: string | null;
  browser_profile_id: string;
  status: PublicationStatus;
  scheduled_at?: number | null;
  approved_at?: number | null;
  started_at?: number | null;
  posted_at?: number | null;
  attempt_count: number;
  idempotency_key: string;
  platform_post_id?: string | null;
  post_url?: string | null;
  title?: string | null;
  caption?: string | null;
  hashtags_json?: string | null;
  description?: string | null;
  video_path?: string | null;
  last_error_code?: string | null;
  last_error_message?: string | null;
  created_at: number;
  updated_at: number;
}

export interface ListJobPublicationsRequest {
  job_id?: string;
  page_id?: string;
  platform?: string;
  status?: string;
  limit?: number;
}

export async function listJobPublications(request: ListJobPublicationsRequest = {}): Promise<JobPublication[]> {
  const res = await invokeCommand<{ publications: JobPublication[] }>('list_job_publications_command', {
    request,
  });
  return res.publications ?? [];
}

export async function approvePublication(publicationId: string, scheduledAt?: number): Promise<JobPublication> {
  const res = await invokeCommand<{ publication: JobPublication }>('approve_publication_command', {
    request: {
      publication_id: publicationId,
      scheduled_at: scheduledAt,
    },
  });
  return res.publication;
}

export async function rejectPublication(publicationId: string, reason?: string): Promise<JobPublication> {
  const res = await invokeCommand<{ publication: JobPublication }>('reject_publication_command', {
    request: {
      publication_id: publicationId,
      reason,
    },
  });
  return res.publication;
}

export async function schedulePublication(publicationId: string, scheduledAt?: number, usePageDefaultSlot?: boolean): Promise<JobPublication> {
  const res = await invokeCommand<{ publication: JobPublication }>('schedule_publication_command', {
    request: {
      publication_id: publicationId,
      scheduled_at: scheduledAt,
      use_page_default_slot: usePageDefaultSlot,
    },
  });
  return res.publication;
}

export async function retryPublication(publicationId: string): Promise<JobPublication> {
  const res = await invokeCommand<{ publication: JobPublication }>('retry_publication_command', {
    request: {
      publication_id: publicationId,
    },
  });
  return res.publication;
}

export async function postNowPublication(publicationId: string): Promise<JobPublication> {
  const res = await invokeCommand<{ publication: JobPublication }>('post_now_publication_command', {
    request: {
      publication_id: publicationId,
    },
  });
  return res.publication;
}

export async function listContentPagePublishTargets(pageId: string): Promise<ContentPagePublishTarget[]> {
  const res = await invokeCommand<{ targets: ContentPagePublishTarget[] }>('list_content_page_publish_targets_command', {
    request: {
      page_id: pageId,
    },
  });
  return res.targets ?? [];
}

export interface UpsertContentPagePublishTargetRequest {
  id?: string;
  page_id: string;
  platform: string;
  enabled: boolean;
  account_label?: string;
  destination_id: string;
  destination_handle?: string;
  browser_profile_id: string;
  post_mode?: string;
  default_slots_json?: string;
}

export async function upsertContentPagePublishTarget(request: UpsertContentPagePublishTargetRequest): Promise<ContentPagePublishTarget> {
  const res = await invokeCommand<{ target: ContentPagePublishTarget }>('upsert_content_page_publish_target_command', {
    request,
  });
  return res.target;
}

export async function deleteContentPagePublishTarget(id: string): Promise<boolean> {
  const res = await invokeCommand<{ success: boolean }>('delete_content_page_publish_target_command', {
    request: { id },
  });
  return res.success;
}

// ---------------------------------------------------------------------------
// Scale & Operations APIs
// ---------------------------------------------------------------------------

export interface DashboardSummary {
  total_jobs: number;
  queued: number;
  waiting_worker: number;
  generating_image: number;
  converting_9_16: number;
  generating_video: number;
  downloading: number;
  saving_local: number;
  ready_to_post: number;
  scheduled: number;
  posting: number;
  done: number;
  error: number;
  auth_required: number;

  publications_facebook: number;
  publications_tiktok: number;
  publications_youtube: number;
  publications_posted: number;
  publications_scheduled: number;
  publications_waiting_approval: number;
  publications_error: number;
}

export interface DashboardSummaryRequest {
  page_id?: string;
  date_from?: number;
  date_to?: number;
  status?: string;
  platform?: string;
}

export async function getDashboardSummary(request: DashboardSummaryRequest = {}): Promise<DashboardSummary> {
  const res = await invokeCommand<{ summary: DashboardSummary }>('floword_dashboard_summary_command', {
    request,
  });
  return res.summary;
}

export interface ListPipelineJobsPaginatedRequest {
  page_id?: string;
  status?: string;
  date_from?: number;
  date_to?: number;
  search_query?: string;
  limit?: number;
  offset?: number;
}

export interface PaginatedPipelineJobsResult {
  jobs: Array<{
    id: { pipeline_job_id: string } | string;
    status: string;
    current_stage: string;
    maybe_page_id?: string;
    maybe_input_payload?: string;
    maybe_stage_outputs?: string;
    maybe_on_failure_message?: string;
    maybe_page_snapshot?: string;
    maybe_business_status?: string;
    maybe_started_at?: number;
    maybe_failure_code?: string;
    maybe_failure_stage?: string;
    created_at: number;
    updated_at: number;
    maybe_completed_at?: number;
  }>;
  total_count: number;
  limit: number;
  offset: number;
}

export async function listPipelineJobsPaginated(request: ListPipelineJobsPaginatedRequest = {}): Promise<PaginatedPipelineJobsResult> {
  const res = await invokeCommand<{ result: PaginatedPipelineJobsResult }>('list_pipeline_jobs_paginated_command', {
    request,
  });
  return res.result;
}

export interface BulkImportRow {
  row_index: number;
  page_id: string;
  source_image?: string;
  image_prompt?: string;
  expand_916_prompt?: string;
  video_prompt?: string;
  title?: string;
  caption?: string;
  hashtags: string[];
  platforms: string[];
  post_mode?: string;
  post_time?: string;
  output_override?: string;
}

export interface BulkValidationError {
  row_index: number;
  field: string;
  code: string;
  message: string;
}

export interface BulkValidationSummary {
  total_rows: number;
  valid_count: number;
  invalid_count: number;
  valid_rows: BulkImportRow[];
  errors: BulkValidationError[];
}

export interface BulkCommitResponse {
  batch_id: string;
  total_created: number;
  created_job_ids: string[];
}

export async function validateBulkImport(params: { csv_content?: string; rows?: BulkImportRow[] }): Promise<BulkValidationSummary> {
  const res = await invokeCommand<{ validation: BulkValidationSummary }>('validate_bulk_import_command', {
    request: params,
  });
  return res.validation;
}

export async function commitBulkImport(rows: BulkImportRow[]): Promise<BulkCommitResponse> {
  const res = await invokeCommand<{ result: BulkCommitResponse }>('commit_bulk_import_command', {
    request: { rows },
  });
  return res.result;
}

export interface PromptTemplate {
  id: string;
  name: string;
  image_prompt: string;
  expand_prompt?: string;
  video_prompt: string;
  created_at: number;
  updated_at: number;
}

export async function listPromptTemplates(): Promise<PromptTemplate[]> {
  const res = await invokeCommand<{ templates: PromptTemplate[] }>('list_prompt_templates_command', {});
  return res.templates ?? [];
}

export interface UpsertPromptTemplateRequest {
  id?: string;
  name: string;
  image_prompt: string;
  expand_prompt?: string;
  video_prompt: string;
}

export async function upsertPromptTemplate(request: UpsertPromptTemplateRequest): Promise<PromptTemplate> {
  const res = await invokeCommand<{ template: PromptTemplate }>('upsert_prompt_template_command', {
    request,
  });
  return res.template;
}

export async function deletePromptTemplate(id: string): Promise<boolean> {
  const res = await invokeCommand<{ success: boolean }>('delete_prompt_template_command', {
    request: { id },
  });
  return res.success;
}

export interface FlowordSetting {
  key: string;
  value_json: string;
  updated_at: number;
}

export async function getFlowordSystemSetting(key: string): Promise<FlowordSetting | null> {
  const res = await invokeCommand<{ setting: FlowordSetting | null }>('get_floword_system_setting_command', {
    request: { key },
  });
  return res.setting ?? null;
}

export async function updateFlowordSystemSetting(key: string, valueJson: string): Promise<FlowordSetting> {
  const res = await invokeCommand<{ setting: FlowordSetting }>('update_floword_system_setting_command', {
    request: { key, value_json: valueJson },
  });
  return res.setting;
}

export interface StorageHealthReport {
  page_id: string;
  target_path: string;
  exists: boolean;
  writable: boolean;
  free_space_bytes?: number;
  last_save_success: boolean;
  error_message?: string;
}

export async function checkStorageHealth(pageId: string): Promise<StorageHealthReport> {
  const res = await invokeCommand<{ report: StorageHealthReport }>('check_storage_health_command', {
    request: { page_id: pageId },
  });
  return res.report;
}

export interface ProbeDetail {
  service: string;
  ready: boolean;
  message: string;
  latency_ms?: number;
}

export interface SystemReadinessReport {
  overall_ready: boolean;
  core_generation_ready: boolean;
  publishing_orchestrator_ready: boolean;
  sqlite_ready: boolean;
  artifact_storage_ready: boolean;
  floword_scheduler_ready: boolean;
  publishing_worker_ready: boolean;
  donut_ready: boolean;
  workers_online_count: number;
  grok_profile_ready: boolean;
  facebook_capability_available: boolean;
  facebook_profile_ready: boolean;
  tiktok_capability_available: boolean;
  tiktok_profile_ready: boolean;
  youtube_capability_available: boolean;
  youtube_profile_ready: boolean;
  details: ProbeDetail[];
}

export async function checkSystemReadiness(): Promise<SystemReadinessReport> {
  const res = await invokeCommand<{ report: SystemReadinessReport }>('check_system_readiness_command', {});
  return res.report;
}

export async function openDonutBrowserGui(): Promise<boolean> {
  try {
    const res = await invokeCommand<{ success: boolean }>('open_donut_browser_gui_command');
    return res?.success ?? false;
  } catch (err) {
    console.warn('Failed to launch Donut Browser GUI:', err);
    return false;
  }
}



