import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
export type { UnlistenFn } from '@tauri-apps/api/event';

export type PipelineStage = 'script_generation' | 'video_assembly' | 'done';

export type TaskStatus =
  | 'pending'
  | 'started'
  | 'complete_success'
  | 'complete_failure'
  | 'cancelled_by_user';

export interface PipelineJobItem {
  id: string;
  status: TaskStatus;
  current_stage: PipelineStage;
  maybe_stage_outputs?: string | null;
  maybe_on_failure_message?: string | null;
}

export interface EnqueuePipelineJobResponse {
  job_id: string;
}

export interface ListPipelineJobsResponse {
  jobs: PipelineJobItem[];
}

export interface CancelPipelineJobResponse {
  cancelled: boolean;
}

export interface LocalAutomationPreset {
  schemaVersion: number;
  /** Stable one-click workflow identifier shared with the Rust contract. */
  presetId?: "QUICK_LOCALIZED_VERTICAL_V1";
  name: string;
  mirrorHorizontal: boolean;
  playbackRate: number;
  preserveAudioPitch: boolean;
  color: { brightness: number; contrast: number; saturation: number; gamma: number; hue: number; temperature: number; highlights: number; shadows: number };
  localization: { enabled: boolean; sourceLanguage: string; targetLanguage: string; transcriptionEngine: string; translate: boolean; burnSubtitles: boolean; subtitleStyle: { fontName: string; fontSize: number; marginV: number }; subtitlePath?: string | null; manualCues?: Array<{ startMs: number; endMs: number; text: string; enabled: boolean }> };
  hook: { enabled: boolean; text: string; startMs: number; endMs: number; style: { fontName: string; fontSize: number; alignment: string; margin: number; outline: number } };
  foreignText: { enabled: boolean; detectionMode: "MANUAL" | "OCR" | "OCR_WITH_MANUAL_REVIEW"; action: "BLUR" | "COVER" | "STICKER"; manualRegions: Array<{ startMs: number; endMs: number; x: number; y: number; width: number; height: number }>; stickerPath?: string | null; stickers?: Array<{ id: string; path: string; x: number; y: number; width: number; height: number; opacity: number; startMs: number; endMs: number; enabled: boolean }> };
  output: { container: string; videoCodec: string; audioCodec: string; ratio: string; width: number; height: number; fps: string; scaleMode: string; qualityPreset: string };
  audioPolicy: "KEEP_IF_RIGHTS_CONFIRMED" | "MUTE_ORIGINAL" | "REPLACE_WITH_LICENSED_AUDIO" | "REPLACE_WITH_USER_AUDIO";
  publishSchedule?: { timezone: "Asia/Ho_Chi_Minh"; slots: string[] } | null;
  autoTranscribe?: boolean;
  sourceLanguage?: string;
  targetLanguage?: string;
  autoTranslate?: boolean;
  autoOcr?: boolean;
  ocrLanguages?: string[];
  /** `rapidocr-onnxruntime` is multilingual; Tesseract codes are only valid
   * when the selected engine explicitly supports language packs. */
  ocrEngine?: string;
  ocrSampleIntervalMs?: number;
  autoDiarize?: boolean;
  autoTts?: boolean;
  translationOutputMode?: "SUBTITLE_ONLY" | "DUBBED_AUDIO";
  originalAudioPolicy?: "KEEP_ORIGINAL" | "REMOVE_ORIGINAL";
  ttsAudioMode?: "REPLACE" | "DUCK_ORIGINAL" | "MIX";
  originalAudioGain?: number;
  speakerVoiceAssignments?: Record<string, string>;
  /** VoiceStudio gallery/profile id for cloned-voice dubbing. */
  voiceProfileId?: string | null;
  /** Runtime is persisted per job so a clone never silently falls back. */
  voiceProvider?: "PIPER" | "ARTCRAFT_SPEECH";
  voiceModel?: string;
}

export interface LocalAutomationReceipt {
  schemaVersion: number;
  jobId: string;
  requestId: string;
  inputSha256: string;
  outputSha256: string;
  outputMd5: string;
  outputPath: string;
  durationMs: number;
  width: number;
  height: number;
  videoCodec: string;
  audioCodec: string;
  playbackRate: number;
  subtitleBurned: boolean;
  subtitleMode?: string;
  subtitleCueCount?: number;
  translationOutputMode?: 'SUBTITLE_ONLY' | 'DUBBED_AUDIO' | string;
  originalAudioPolicy?: 'KEEP_ORIGINAL' | 'REMOVE_ORIGINAL' | string;
  ttsAudioMode?: 'REPLACE' | 'DUCK_ORIGINAL' | 'MIX' | string;
  hookApplied: boolean;
  foreignTextRegionsApplied: number;
  terminal: boolean;
  state: string;
  presetId?: string | null;
  createdAt?: number | null;
  startedAt?: number | null;
  completedAt?: number | null;
  inputPath?: string | null;
  inputDurationMs?: number | null;
  inputDimensions?: { width: number; height: number } | null;
  outputDimensions?: { width: number; height: number } | null;
  encoder?: string | null;
  stageTimings?: Record<string, number> | null;
  transcriptSegmentCount?: number;
  translatedSegmentCount?: number;
  ocrRegionCount?: number;
  speakerCount?: number;
  ttsSegmentCount?: number;
  cancelCount?: number;
  retryCount?: number;
  restored?: boolean;
  inputMd5?: string | null;
  previewSha256?: string | null;
  resourceVersions?: Record<string, string>;
  warnings?: string[];
  renderTrackCount?: number;
  filterNodeCount?: number;
  estimatedOutputFrames?: number;
  outputFps?: number | null;
  detectedLanguages?: string[];
  dominantDetectedLanguage?: string | null;
  languageDetectionConfidence?: number | null;
  effectiveSourceLanguages?: string[];
  translationRoutes?: string[][];
  requestedSourceLanguage?: string | null;
  effectiveSourceLanguage?: string | null;
  ocrLanguages?: string[];
  routeKind?: 'DIRECT' | 'PIVOT' | string | null;
  translationHops?: string[];
}

export type NativeAutomationJobState = 'QUEUED' | 'PROBING' | 'PREPARING' | 'RENDERING' | 'VERIFYING' | 'COMPLETED' | 'CANCEL_REQUESTED' | 'FAILED' | 'CANCELLED';
export interface NativeAutomationJob {
  jobId: string; requestId: string; attempt: number; attemptId?: string; dispatchCount?: number; retryCount?: number; lastWorkerInvocationAt?: number | null; inputPath: string; outputRoot?: string | null; pageName: string;
  state: NativeAutomationJobState; progress: number; stageProgress?: number; overallProgress?: number; elapsedMs?: number; estimatedRemainingMs?: number | null; decodedFrames?: number; changeCandidateFrames?: number; ocrFrames?: number; skippedDuplicateFrames?: number; rawDetectionCount?: number; mergedTrackCount?: number; processingFps?: number | null; speedRatio?: number | null; firstProgressAt?: number | null; lastProgressAt?: number | null; message?: string | null; stage: string; createdAt: number; startedAt?: number | null; finishedAt?: number | null; processedMs?: number | null; expectedDurationMs?: number | null; ffmpegPid?: number | null; errorCode?: string | null; errorMessage?: string | null; error?: string | null; receipt?: LocalAutomationReceipt | null;
}
export interface NativeAutomationProgressPayload extends NativeAutomationJob {}

export const NATIVE_AUTOMATION_PROGRESS_EVENT = 'capcut://automation_progress';

/** Start the legacy provider only after the user explicitly selects Legacy. */
export async function ensureLegacyCapCutMate(): Promise<void> {
  if (!isTauriAvailable()) return;
  try {
    await Promise.race([
      invoke<void>('ensure_legacy_capcut_mate'),
      new Promise((resolve) => setTimeout(resolve, 600)),
    ]);
  } catch {
    // Unified backend on port 30000 is already running
  }
}

export async function startNativeAutomationJob(request: RunLocalAutomationRequest): Promise<NativeAutomationJob> {
  if (!isTauriAvailable()) throw new Error('Native Automation chỉ hoạt động trong Xưởng Sản Xuất Video Desktop');
  return invoke<NativeAutomationJob>('start_capcut_automation_job', { request });
}
export async function listNativeAutomationJobs(): Promise<NativeAutomationJob[]> {
  if (!isTauriAvailable()) return [];
  return invoke<NativeAutomationJob[]>('list_capcut_automation_jobs');
}
export async function getNativeAutomationJob(jobId: string): Promise<NativeAutomationJob> {
  return invoke<NativeAutomationJob>('get_capcut_automation_job', { request: { job_id: jobId } });
}
export async function cancelNativeJob(jobId: string): Promise<NativeAutomationJob> {
  return invoke<NativeAutomationJob>('cancel_capcut_automation_job', { request: { job_id: jobId } });
}
export async function retryNativeJob(jobId: string): Promise<NativeAutomationJob> {
  return invoke<NativeAutomationJob>('retry_capcut_automation_job', { request: { job_id: jobId } });
}
export async function removeNativeJob(jobId: string): Promise<void> {
  return invoke<void>('remove_capcut_automation_job', { request: { job_id: jobId } });
}
export async function previewNativeAutomation(inputPath: string, preset: LocalAutomationPreset, startMs = 0, durationMs = 5000): Promise<{ path: string; cacheKey: string }> {
  return invoke<{ path: string; cacheKey: string }>('preview_capcut_automation', { request: { inputPath, preset, startMs, durationMs } });
}

export interface LocalTranscriptSegment {
  id: string;
  start_ms: number;
  end_ms: number;
  text: string;
  language?: string | null;
  confidence?: number | null;
  speaker_id?: string | null;
}

export interface LocalTranscriptionResponse {
  engine: string;
  model_sha256: string;
  executable_sha256: string;
  segments: LocalTranscriptSegment[];
}

export async function transcribeCapCutAutomationLocal(inputPath: string, language?: string): Promise<LocalTranscriptionResponse> {
  return invoke<LocalTranscriptionResponse>('transcribe_capcut_automation_local', { request: { input_path: inputPath, language: language ?? null } });
}

export interface LocalTranslationSegment extends LocalTranscriptSegment {
  translated_text?: string | null;
}

export interface LocalTranslationResponse {
  provider: string;
  model: string;
  segments: LocalTranslationSegment[];
}

export async function translateCapCutAutomationLocal(segments: LocalTranslationSegment[], sourceLanguage = 'en', targetLanguage = 'vi'): Promise<LocalTranslationResponse> {
  return invoke<LocalTranslationResponse>('translate_capcut_automation_local', { request: { segments, source_language: sourceLanguage, target_language: targetLanguage } });
}

export interface LocalOcrRegion {
  id: string;
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
  confidence?: number | null;
}

export interface LocalOcrResponse { engine: string; language: string; regions: LocalOcrRegion[]; }

export async function ocrCapCutAutomationLocal(inputPath: string, imageWidth: number, imageHeight: number, language = 'eng'): Promise<LocalOcrResponse> {
  return invoke<LocalOcrResponse>('ocr_capcut_automation_local', { request: { image_path: inputPath, image_width: imageWidth, image_height: imageHeight, language } });
}

export interface LocalOcrTranslatedRegion extends LocalOcrRegion { translated_text: string; }
export interface LocalOcrTranslationResponse { provider: string; model: string; regions: LocalOcrTranslatedRegion[]; }

export async function translateOcrCapCutAutomationLocal(regions: LocalOcrRegion[], sourceLanguage = 'en', targetLanguage = 'vi'): Promise<LocalOcrTranslationResponse> {
  return invoke<LocalOcrTranslationResponse>('translate_ocr_capcut_automation_local', { request: { regions, source_language: sourceLanguage, target_language: targetLanguage } });
}

export interface LocalSpeakerTurn { speakerId: string; startMs: number; endMs: number; confidence?: number | null; }
export interface LocalSpeakerResponse { engine: string; sampleRate: number; turns: LocalSpeakerTurn[]; }

/** Detect speaker turns with the ArtCraft-owned offline diarization engine. */
export async function detectSpeakersCapCutAutomationLocal(audioPath: string, numSpeakers = 0): Promise<LocalSpeakerResponse> {
  return invoke<LocalSpeakerResponse>('detect_speakers_capcut_automation_local', { request: { audio_path: audioPath, num_speakers: numSpeakers } });
}

export type VoiceStudioProfile = Record<string, unknown> & { id?: string; voice_id?: string; name?: string; profile_name?: string };

/** Read VoiceStudio capabilities without coupling the UI to its internal schema. */
export async function getVoiceStudioCapabilities(): Promise<Record<string, unknown>> {
  return invoke<Record<string, unknown>>('get_voice_studio_capabilities');
}

/** Starts ArtCraft's embedded speech service after verifying its identity. */
export async function ensureArtcraftSpeechRuntime(acceptModelTerms = false): Promise<Record<string, unknown>> {
  return invoke<Record<string, unknown>>('ensure_artcraft_speech_runtime', {
    request: { accept_model_terms: acceptModelTerms },
  });
}

/** List persisted local voice profiles available for cloned-voice synthesis. */
export async function listVoiceStudioProfiles(): Promise<VoiceStudioProfile[]> {
  const result = await invoke<unknown>('list_voice_studio_profiles');
  if (Array.isArray(result)) return result as VoiceStudioProfile[];
  if (result && typeof result === 'object') {
    const values = (result as { voices?: unknown; profiles?: unknown }).voices ?? (result as { profiles?: unknown }).profiles;
    return Array.isArray(values) ? values as VoiceStudioProfile[] : [];
  }
  return [];
}

export async function uploadVoiceStudioClip(request: { path: string; name?: string; reference_text?: string }): Promise<Record<string, unknown>> {
  return invoke<Record<string, unknown>>('upload_voice_studio_clip', { request });
}

export async function saveVoiceStudioProfile(voiceId: string, profileName: string): Promise<Record<string, unknown>> {
  return invoke<Record<string, unknown>>('save_voice_studio_profile', { request: { voice_id: voiceId, profile_name: profileName } });
}
export async function listenNativeAutomationProgress(cb: (payload: NativeAutomationProgressPayload) => void): Promise<UnlistenFn> {
  if (!isTauriAvailable()) return NOOP_UNLISTEN;
  return listen<NativeAutomationProgressPayload>(NATIVE_AUTOMATION_PROGRESS_EVENT, (event) => cb(event.payload));
}

export interface RunLocalAutomationRequest {
  inputPath: string;
  outputRoot?: string;
  pageName?: string;
  jobId?: string;
  requestId?: string;
  preset?: LocalAutomationPreset;
}

/** Render locally with ArtCraft's packaged FFmpeg; no CapCut Mate server is used. */
export async function runLocalAutomation(request: RunLocalAutomationRequest): Promise<LocalAutomationReceipt> {
  if (!isTauriAvailable()) throw new Error('Local render chỉ hoạt động trong ứng dụng Desktop');
  const response = await invoke<{ receipt: LocalAutomationReceipt }>('run_capcut_automation_command', { request });
  return response.receipt;
}

export interface StageCompletePayload {
  job_id: string;
  completed_stage: string;
  next_stage: string;
}

export interface JobCompletePayload {
  job_id: string;
  video_url: string;
}

export interface JobFailedPayload {
  job_id: string;
  failed_stage: string;
  error_message: string;
}

/** Check if running inside Tauri runtime desktop app vs standard web browser */
export function isTauriAvailable(): boolean {
  return typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);
}

/**
 * Enqueue a new automated video generation job into the pipeline queue.
 */
export async function enqueuePipelineJob(prompt: string): Promise<string> {
  if (!isTauriAvailable()) {
    throw new Error('Rust Pipeline chỉ hoạt động trên app Desktop Tauri (Xưởng Sản Xuất Video), không khả dụng trên Web Browser');
  }
  const res = await invoke<EnqueuePipelineJobResponse>('enqueue_pipeline_job_command', {
    request: { prompt },
  });
  return res.job_id;
}

/**
 * Fetch all pipeline jobs (pending, in-progress, completed, failed).
 */
export async function listPipelineJobs(): Promise<PipelineJobItem[]> {
  if (!isTauriAvailable()) {
    return [];
  }
  const res = await invoke<ListPipelineJobsResponse>('list_pipeline_jobs_command');
  return res.jobs;
}

/**
 * Cancel an active pipeline job.
 */
export async function cancelPipelineJob(jobId: string): Promise<boolean> {
  if (!isTauriAvailable()) {
    return false;
  }
  const res = await invoke<CancelPipelineJobResponse>('cancel_pipeline_job_command', {
    request: { job_id: jobId },
  });
  return res.cancelled;
}

/** Event names emitted by the Rust pipeline_worker_thread */
export const PIPELINE_EVENTS = {
  STAGE_COMPLETE: 'pipeline://stage_complete',
  JOB_COMPLETE: 'pipeline://job_complete',
  JOB_FAILED: 'pipeline://job_failed',
} as const;

const NOOP_UNLISTEN: UnlistenFn = () => {};

/** Listen for stage progress updates */
export async function listenStageComplete(
  cb: (payload: StageCompletePayload) => void
): Promise<UnlistenFn> {
  if (!isTauriAvailable()) return NOOP_UNLISTEN;
  return listen<StageCompletePayload>(PIPELINE_EVENTS.STAGE_COMPLETE, (event) => cb(event.payload));
}

/** Listen for successful job completion */
export async function listenJobComplete(
  cb: (payload: JobCompletePayload) => void
): Promise<UnlistenFn> {
  if (!isTauriAvailable()) return NOOP_UNLISTEN;
  return listen<JobCompletePayload>(PIPELINE_EVENTS.JOB_COMPLETE, (event) => cb(event.payload));
}

/** Listen for job execution failures */
export async function listenJobFailed(
  cb: (payload: JobFailedPayload) => void
): Promise<UnlistenFn> {
  if (!isTauriAvailable()) return NOOP_UNLISTEN;
  return listen<JobFailedPayload>(PIPELINE_EVENTS.JOB_FAILED, (event) => cb(event.payload));
}
