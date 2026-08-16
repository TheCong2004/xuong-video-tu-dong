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
}

export interface EnqueueFlowordWorkflowResponse {
  job_id: string;
  workflow_id?: string | null;
  status: string;
}

export interface GetFlowordWorkflowResponse {
  job_id: string;
  status: string;
  current_stage: string;
  failure_message?: string | null;
  stage_outputs?: string | null;
  stage_states?: FlowordStageState[];
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

export function enqueueFlowordWorkflow(request: EnqueueFlowordWorkflowRequest): Promise<EnqueueFlowordWorkflowResponse> {
  return invokeCommand<EnqueueFlowordWorkflowResponse>('enqueue_floword_workflow', { request });
}

export function getFlowordWorkflow(jobId: string): Promise<GetFlowordWorkflowResponse> {
  return invokeCommand<GetFlowordWorkflowResponse>('get_floword_workflow', { request: { job_id: jobId } });
}

export function cancelFlowordWorkflow(jobId: string): Promise<CancelFlowordWorkflowResponse> {
  return invokeCommand<CancelFlowordWorkflowResponse>('cancel_floword_workflow', { request: { job_id: jobId } });
}

export function retryFlowordStep(jobId: string, stepId: string): Promise<RetryFlowordStepResponse> {
  return invokeCommand<RetryFlowordStepResponse>('retry_floword_step', { request: { job_id: jobId, step_id: stepId } });
}

export function skipFlowordResearch(jobId: string): Promise<RetryFlowordStepResponse> {
  return invokeCommand<RetryFlowordStepResponse>('skip_floword_research', { request: { job_id: jobId } });
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
  import.meta.env.VITE_BACKEND_BASE_URL?.trim() || 'http://127.0.0.1:30000'
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

