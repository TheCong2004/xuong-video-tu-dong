use super::capcut_automation::{estimate_render_graph_complexity, file_hashes, media_duration_ms, output_time_ms, progress_fraction, render_video_with_progress_and_pid_and_audio, validate_render_graph_complexity, write_hook_ass, write_manual_subtitle_ass, write_receipt, CapcutAutomationPresetV1, CapcutAutomationReceipt, MediaDimensions, StageTimings, SubtitleCue, TimedRegion};
use super::capcut_automation_engine_manager::CapcutAutomationEngineManager;
use super::capcut_automation_ocr::{infer_ocr_source_language, infer_text_language, recognize_local, translate_regions_local, LocalOcrRequest, LocalOcrTranslationRequest, OcrRegion};
use super::capcut_automation_speaker::{assign_speakers, detect_speakers_local, LocalSpeakerRequest, SpeakerSegment, SpeakerTurn};
use super::capcut_automation_translation::{plan_translation_route, translate_local, translate_segments_by_language, validate_translation_quality_against_source, LocalTranslationRequest, TranslationRouteKind, TranslationSegment};
use super::capcut_automation_transcription::{transcribe_local, validate_transcript_quality, LocalTranscriptionRequest};
use crate::services::pipeline::clients::omniroute_client::{ScriptScene, StructuredScript};
use crate::services::pipeline::voice::{configured_tts_model, configured_voice_id, synthesize_voice_with_runtime, VoiceInput};
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::services::pipeline::output_policy::OutputPathResolver;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{
  atomic::{AtomicBool, Ordering},
  Arc, Mutex,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{OwnedSemaphorePermit, Notify, Semaphore};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapcutAutomationJobState {
  Queued,
  Probing,
  Preparing,
  Rendering,
  Verifying,
  Completed,
  CancelRequested,
  Failed,
  Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapcutAutomationJob {
  pub job_id: String,
  pub request_id: String,
  pub attempt: u32,
  /// Stable identity for this execution attempt. Retries receive a new value.
  #[serde(default)]
  pub attempt_id: String,
  /// Counters persisted for diagnosing dispatch/retry behavior across restarts.
  #[serde(default)]
  pub dispatch_count: u32,
  #[serde(default)]
  pub retry_count: u32,
  #[serde(default)]
  pub last_worker_invocation_at: Option<u64>,
  pub input_path: String,
  pub output_root: Option<String>,
  pub page_name: String,
  pub state: CapcutAutomationJobState,
  pub progress: f64,
  /// Progress within the currently announced stage (0..1).  These fields are
  /// additive so persisted jobs from older builds remain readable.
  #[serde(default)]
  pub stage_progress: f64,
  #[serde(default)]
  pub overall_progress: f64,
  #[serde(default)]
  pub elapsed_ms: u64,
  #[serde(default)]
  pub estimated_remaining_ms: Option<u64>,
  #[serde(default)]
  pub decoded_frames: u64,
  #[serde(default)]
  pub change_candidate_frames: u64,
  #[serde(default)]
  pub ocr_frames: u64,
  #[serde(default)]
  pub skipped_duplicate_frames: u64,
  #[serde(default)]
  pub raw_detection_count: u64,
  #[serde(default)]
  pub merged_track_count: u64,
  #[serde(default)]
  pub processing_fps: Option<f64>,
  #[serde(default)]
  pub speed_ratio: Option<f64>,
  #[serde(default)]
  pub first_progress_at: Option<u64>,
  #[serde(default)]
  pub last_progress_at: Option<u64>,
  #[serde(default)]
  pub message: Option<String>,
  pub stage: String,
  pub created_at: u64,
  pub started_at: Option<u64>,
  pub finished_at: Option<u64>,
  pub processed_ms: Option<u64>,
  pub expected_duration_ms: Option<u64>,
  pub ffmpeg_pid: Option<u32>,
  pub error_code: Option<String>,
  pub error_message: Option<String>,
  pub error: Option<String>,
  pub receipt: Option<CapcutAutomationReceipt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartCapcutAutomationRequest {
  pub input_path: String,
  pub output_root: Option<String>,
  pub page_name: Option<String>,
  pub job_id: Option<String>,
  pub request_id: Option<String>,
  pub preset: Option<CapcutAutomationPresetV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResponse {
  pub path: String,
  pub cache_key: String,
}

struct JobRecord {
  snapshot: Mutex<CapcutAutomationJob>,
  request: StartCapcutAutomationRequest,
  cancel: AtomicBool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistedJob {
  snapshot: CapcutAutomationJob,
  request: StartCapcutAutomationRequest,
}

struct Inner {
  jobs: HashMap<String, Arc<JobRecord>>,
  queue: VecDeque<String>,
  worker_started: bool,
  persistence_path: Option<PathBuf>,
}

#[derive(Clone)]
pub struct CapcutAutomationJobManager {
  inner: Arc<Mutex<Inner>>,
  notify: Arc<Notify>,
  /// Limits simultaneous native renders. Three jobs run by default; the
  /// ARTCRAFT_CAPCUT_CONCURRENCY override allows up to ten when the machine
  /// has enough CPU/RAM, while still preventing an unbounded process fan-out.
  concurrency: Arc<Semaphore>,
}

impl Default for CapcutAutomationJobManager {
  fn default() -> Self {
    Self::new()
  }
}

impl CapcutAutomationJobManager {
  pub fn new() -> Self {
    let concurrency = std::env::var("ARTCRAFT_CAPCUT_CONCURRENCY").ok().and_then(|value| value.parse::<usize>().ok()).filter(|value| (1..=10).contains(value)).unwrap_or(3);
    Self { inner: Arc::new(Mutex::new(Inner { jobs: HashMap::new(), queue: VecDeque::new(), worker_started: false, persistence_path: None })), notify: Arc::new(Notify::new()), concurrency: Arc::new(Semaphore::new(concurrency)) }
  }

  fn persistence_path(root: &AppDataRoot) -> PathBuf {
    root.path().join("capcut-automation").join("jobs.json")
  }

  fn persist(&self) {
    let Ok(inner) = self.inner.lock() else {
      return;
    };
    let Some(path) = inner.persistence_path.clone() else {
      return;
    };
    let entries = inner.jobs.values().filter_map(|record| record.snapshot.lock().ok().map(|snapshot| PersistedJob { snapshot: snapshot.clone(), request: record.request.clone() })).collect::<Vec<_>>();
    drop(inner);
    let Ok(bytes) = serde_json::to_vec_pretty(&entries) else {
      return;
    };
    if let Some(parent) = path.parent() {
      let _ = std::fs::create_dir_all(parent);
    }
    let partial = path.with_extension("json.partial");
    if std::fs::write(&partial, bytes).is_ok() {
      let _ = std::fs::rename(partial, path);
    }
  }

  pub fn restore(&self, app: AppHandle, root: AppDataRoot) -> Vec<CapcutAutomationJob> {
    let path = Self::persistence_path(&root);
    let entries = std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Vec<PersistedJob>>(&bytes).ok()).unwrap_or_default();
    let mut should_start = false;
    if let Ok(mut inner) = self.inner.lock() {
      inner.persistence_path = Some(path);
      for mut entry in entries {
        if inner.jobs.contains_key(&entry.snapshot.job_id) {
          continue;
        }
        if entry.snapshot.attempt_id.is_empty() {
          entry.snapshot.attempt_id = format!("{}-attempt-{}", entry.snapshot.job_id, entry.snapshot.attempt.max(1));
        }
        if matches!(entry.snapshot.state, CapcutAutomationJobState::Failed | CapcutAutomationJobState::Cancelled) {
          if let Err(reason) = retry_preflight_error(&entry.request) {
            // Keep terminal history visible, but make an invalid immutable
            // request explicitly non-retryable in the UI.
            entry.snapshot.error = Some(format!("{}; RETRY_DISABLED: {}", entry.snapshot.error.clone().unwrap_or_default(), reason));
            entry.snapshot.error_message = entry.snapshot.error.clone();
          }
        }
        // A cancellation request is terminal across an app restart. The
        // previous process/child identity cannot be trusted, and requeueing
        // this state made a user-cancelled job run again and block FIFO work.
        if matches!(entry.snapshot.state, CapcutAutomationJobState::CancelRequested) {
          mark_restored_cancelled(&mut entry.snapshot);
        } else if matches!(entry.snapshot.state, CapcutAutomationJobState::Queued | CapcutAutomationJobState::Probing | CapcutAutomationJobState::Preparing | CapcutAutomationJobState::Rendering) {
          reset_restored_job_for_requeue(&mut entry.snapshot);
          inner.queue.push_back(entry.snapshot.job_id.clone());
          should_start = true;
        }
        inner.jobs.insert(entry.snapshot.job_id.clone(), Arc::new(JobRecord { snapshot: Mutex::new(entry.snapshot), request: entry.request, cancel: AtomicBool::new(false) }));
      }
      if should_start && !inner.worker_started {
        inner.worker_started = true;
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
          manager.worker(app, root).await;
        });
      }
    }
    self.persist();
    self.list()
  }

  pub fn start(&self, app: AppHandle, app_data_root: AppDataRoot, request: StartCapcutAutomationRequest) -> Result<CapcutAutomationJob, String> {
    let job_id = valid_id(request.job_id.clone()).unwrap_or_else(|| format!("native-{}", Uuid::new_v4().simple()));
    let request_id = valid_id(request.request_id.clone()).unwrap_or_else(|| job_id.clone());
    let input = request.input_path.trim().to_string();
    if input.is_empty() || !Path::new(&input).is_file() {
      return Err("INPUT_NOT_FOUND".to_string());
    }
    let mut request = request;
    let mut preset = request.preset.clone().unwrap_or_default();
    preset.normalize_audio_contract();
    request.preset = Some(preset.clone());
    preset.validate()?;
    validate_start_preflight(&preset)?;
    let page_name = request.page_name.clone().unwrap_or_else(|| "ArtCraft".to_string());
    let snapshot = CapcutAutomationJob { job_id: job_id.clone(), request_id, attempt: 1, attempt_id: format!("{job_id}-attempt-1"), dispatch_count: 0, retry_count: 0, last_worker_invocation_at: None, input_path: input, output_root: request.output_root.clone(), page_name, state: CapcutAutomationJobState::Queued, progress: 0.0, stage_progress: 0.0, overall_progress: 0.0, elapsed_ms: 0, estimated_remaining_ms: None, decoded_frames: 0, change_candidate_frames: 0, ocr_frames: 0, skipped_duplicate_frames: 0, raw_detection_count: 0, merged_track_count: 0, processing_fps: None, speed_ratio: None, first_progress_at: None, last_progress_at: None, message: Some("Đang chờ xử lý".into()), stage: "QUEUED".into(), created_at: now_ms(), started_at: None, finished_at: None, processed_ms: None, expected_duration_ms: None, ffmpeg_pid: None, error_code: None, error_message: None, error: None, receipt: None };
    let record = Arc::new(JobRecord { snapshot: Mutex::new(snapshot.clone()), request, cancel: AtomicBool::new(false) });
    let mut inner = self.inner.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
    inner.persistence_path = Some(Self::persistence_path(&app_data_root));
    if inner.jobs.contains_key(&job_id) {
      return Err("JOB_ID_ALREADY_EXISTS".to_string());
    }
    inner.jobs.insert(job_id.clone(), record);
    inner.queue.push_back(job_id);
    if !inner.worker_started {
      inner.worker_started = true;
      let manager = self.clone();
      tauri::async_runtime::spawn(async move {
        manager.worker(app, app_data_root).await;
      });
    }
    drop(inner);
    self.persist();
    self.notify.notify_one();
    Ok(snapshot)
  }

  pub fn list(&self) -> Vec<CapcutAutomationJob> {
    self.inner.lock().ok().map(|i| i.jobs.values().filter_map(|r| r.snapshot.lock().ok().map(|s| s.clone())).collect()).unwrap_or_default()
  }
  pub fn get(&self, id: &str) -> Option<CapcutAutomationJob> {
    self.inner.lock().ok()?.jobs.get(id)?.snapshot.lock().ok().map(|s| s.clone())
  }

  pub fn cancel(&self, id: &str) -> Result<CapcutAutomationJob, String> {
    let inner = self.inner.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
    let record = inner.jobs.get(id).ok_or_else(|| "JOB_NOT_FOUND".to_string())?.clone();
    let mut state = record.snapshot.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
    match &state.state {
      CapcutAutomationJobState::Queued => {
        record.cancel.store(true, Ordering::SeqCst);
        state.state = CapcutAutomationJobState::Cancelled;
        state.stage = "CANCELLED".into();
        state.error = Some("JOB_CANCELLED".to_string());
        state.error_code = Some("CAPCUT_CANCELLED".into());
        state.error_message = state.error.clone();
      },
      CapcutAutomationJobState::Rendering | CapcutAutomationJobState::Preparing | CapcutAutomationJobState::Probing => {
        record.cancel.store(true, Ordering::SeqCst);
        state.state = CapcutAutomationJobState::CancelRequested;
        state.stage = "CANCEL_REQUESTED".into();
      },
      CapcutAutomationJobState::CancelRequested | CapcutAutomationJobState::Verifying | CapcutAutomationJobState::Completed | CapcutAutomationJobState::Failed | CapcutAutomationJobState::Cancelled => {},
    }
    let result = state.clone();
    drop(state);
    drop(inner);
    self.persist();
    Ok(result)
  }

  pub fn retry(&self, id: &str) -> Result<CapcutAutomationJob, String> {
    let inner = self.inner.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
    let record = inner.jobs.get(id).ok_or_else(|| "JOB_NOT_FOUND".to_string())?.clone();
    // Retry means rerun the immutable persisted request, never the current
    // global/UI preset. Validate before mutating state or incrementing the
    // attempt so invalid historical requests cannot dispatch.
    retry_preflight_error(&record.request)?;
    let mut state = record.snapshot.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
    if !matches!(state.state, CapcutAutomationJobState::Failed | CapcutAutomationJobState::Cancelled) {
      return Err("JOB_NOT_RETRYABLE".to_string());
    }
    state.state = CapcutAutomationJobState::Queued;
    state.attempt = state.attempt.saturating_add(1);
    state.attempt_id = format!("{}-attempt-{}", state.job_id, state.attempt);
    state.retry_count = state.retry_count.saturating_add(1);
    state.stage = "QUEUED".into();
    state.progress = 0.0;
    state.stage_progress = 0.0;
    state.overall_progress = 0.0;
    state.elapsed_ms = 0;
    state.started_at = None;
    state.finished_at = None;
    state.processed_ms = None;
    state.ffmpeg_pid = None;
    state.error = None;
    state.receipt = None;
    state.error_code = None;
    state.error_message = None;
    record.cancel.store(false, Ordering::SeqCst);
    drop(state);
    drop(inner);
    let mut inner = self.inner.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
    inner.queue.push_back(id.to_string());
    drop(inner);
    self.notify.notify_one();
    self.persist();
    self.get(id).ok_or_else(|| "JOB_MANAGER_LOCK_FAILED".to_string())
  }

  pub fn remove(&self, id: &str) -> Result<(), String> {
    let mut inner = self.inner.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
    if let Some(record) = inner.jobs.get(id) {
      let state = record.snapshot.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
      if !matches!(state.state, CapcutAutomationJobState::Completed | CapcutAutomationJobState::Failed | CapcutAutomationJobState::Cancelled) {
        return Err("JOB_NOT_TERMINAL".to_string());
      }
    }
    inner.queue.retain(|queued| queued != id);
    inner.jobs.remove(id);
    drop(inner);
    self.persist();
    Ok(())
  }

  async fn worker(&self, app: AppHandle, app_data_root: AppDataRoot) {
    loop {
      let record = {
        let mut inner = match self.inner.lock() {
          Ok(i) => i,
          Err(_) => return,
        };
        inner.queue.pop_front().and_then(|id| inner.jobs.get(&id).cloned())
      };
      let Some(record) = record else {
        self.notify.notified().await;
        continue;
      };
      if record.cancel.load(Ordering::SeqCst) {
        if let Ok(mut s) = record.snapshot.lock() {
          s.state = CapcutAutomationJobState::Cancelled;
          s.stage = "CANCELLED".into();
          s.error = Some("JOB_CANCELLED".into());
        }
        self.persist();
        continue;
      }
      let permit = match self.concurrency.clone().acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => return,
      };
      // A queued job can be cancelled while it is waiting for a concurrency
      // slot. Re-check after acquiring the permit so cancellation never
      // dispatches a render that the user already stopped.
      if record.cancel.load(Ordering::SeqCst) {
        if let Ok(mut s) = record.snapshot.lock() {
          s.state = CapcutAutomationJobState::Cancelled;
          s.stage = "CANCELLED".into();
          s.error = Some("JOB_CANCELLED".into());
          s.error_code = Some("CAPCUT_CANCELLED".into());
          s.finished_at = Some(now_ms());
        }
        self.persist();
        continue;
      }
      if let Ok(mut s) = record.snapshot.lock() {
        s.dispatch_count = s.dispatch_count.saturating_add(1);
        s.last_worker_invocation_at = Some(now_ms());
        s.state = CapcutAutomationJobState::Preparing;
        s.stage = "PROBING".into();
        s.stage_progress = 0.0;
        s.overall_progress = 0.0;
        s.message = Some(stage_message("PROBING").into());
        s.started_at = Some(now_ms());
        emit_progress(&app, &s);
      }
      self.persist();
      let record_for_worker = record.clone();
      let app_for_worker = app.clone();
      let root = app_data_root.clone();
      let jobs_for_worker = self.clone();
      tauri::async_runtime::spawn(async move {
        finish_worker_result(&jobs_for_worker, &app_for_worker, &record_for_worker, root, permit).await;
      });
    }
  }
}

async fn finish_worker_result(manager: &CapcutAutomationJobManager, app: &AppHandle, record: &Arc<JobRecord>, root: AppDataRoot, _permit: OwnedSemaphorePermit) {
  let jobs_for_worker = manager.clone();
  let record_for_worker = record.clone();
  let app_for_worker = app.clone();
  let result = tokio::task::spawn_blocking(move || render_job(&app_for_worker, &root, &record_for_worker, &jobs_for_worker)).await;
  match result {
    Ok(Ok(receipt)) => {
      if let Ok(mut s) = record.snapshot.lock() {
        s.state = CapcutAutomationJobState::Verifying;
        s.stage = "VERIFYING".into();
        s.ffmpeg_pid = None;
        emit_progress(app, &s);
      }
      manager.persist();
      if let Ok(mut s) = record.snapshot.lock() {
        s.state = CapcutAutomationJobState::Completed;
        s.stage = "COMPLETED".into();
        s.progress = 1.0;
        s.stage_progress = 1.0;
        s.overall_progress = 1.0;
        s.elapsed_ms = s.started_at.map(|started| now_ms().saturating_sub(started)).unwrap_or(0);
        s.estimated_remaining_ms = Some(0);
        s.message = Some("Đã hoàn tất".into());
        s.receipt = Some(receipt);
        s.finished_at = Some(now_ms());
        emit_progress(app, &s);
      }
      manager.persist();
    },
    Ok(Err(error)) if error == "JOB_CANCELLED" => {
      if let Ok(mut s) = record.snapshot.lock() {
        s.state = CapcutAutomationJobState::Cancelled;
        s.stage = "CANCELLED".into();
        s.ffmpeg_pid = None;
        s.error = Some(error);
        s.error_code = Some("CAPCUT_CANCELLED".into());
        s.finished_at = Some(now_ms());
        emit_progress(app, &s);
      }
      manager.persist();
    },
    Ok(Err(error)) => {
      if let Ok(mut s) = record.snapshot.lock() {
        s.state = CapcutAutomationJobState::Failed;
        s.stage = "FAILED".into();
        s.ffmpeg_pid = None;
        s.error = Some(error);
        s.error_code = Some("CAPCUT_RENDER_FAILED".into());
        s.error_message = s.error.clone();
        s.finished_at = Some(now_ms());
        emit_progress(app, &s);
      }
      manager.persist();
    },
    Err(error) => {
      if let Ok(mut s) = record.snapshot.lock() {
        s.state = CapcutAutomationJobState::Failed;
        s.stage = "FAILED".into();
        s.ffmpeg_pid = None;
        s.error = Some(format!("JOB_WORKER_FAILED: {error}"));
        s.error_code = Some("CAPCUT_WORKER_FAILED".into());
        s.error_message = s.error.clone();
        s.finished_at = Some(now_ms());
        emit_progress(&app, &s);
      }
      // The join error is terminal too; persist it just like every other
      // terminal path so a reload cannot leave an in-flight snapshot.
      manager.persist();
    },
  }
}

fn now_ms() -> u64 {
  std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_millis() as u64).unwrap_or(0)
}

fn emit_progress(app: &AppHandle, job: &CapcutAutomationJob) {
  let _ = app.emit("capcut://automation_progress", job);
}

fn valid_id(value: Option<String>) -> Option<String> {
  let v = value?.trim().to_string();
  if v.is_empty() || v.len() > 128 || v.contains(['\\', '/', ':']) {
    None
  } else {
    Some(v)
  }
}

/// Validate request-level contracts before a job is persisted or dispatched.
/// `auto` is valid when local transcription is enabled: Whisper detects the
/// language first, then the manager resolves a verified translation route.
/// We reject only the case where automatic detection was requested but no
/// detector is enabled, since that request cannot be resolved safely.
fn validate_start_preflight(preset: &CapcutAutomationPresetV1) -> Result<(), String> {
  let requested_source = preset.localization.source_language.trim();
  let source = if requested_source.is_empty() || requested_source.eq_ignore_ascii_case("auto") { preset.source_language.trim() } else { requested_source };
  if source.is_empty() {
    return Err("CAPCUT_SOURCE_LANGUAGE_UNRESOLVED".to_string());
  }
  if source.eq_ignore_ascii_case("auto") && !preset.auto_transcribe {
    return Err("CAPCUT_SOURCE_LANGUAGE_UNRESOLVED".to_string());
  }
  // RapidOCR is multilingual and does not consume Tesseract language codes.
  // For engines that do require a language pack, an empty list is invalid.
  let rapidocr = preset.ocr_engine.trim().eq_ignore_ascii_case("rapidocr-onnxruntime") || preset.ocr_engine.trim().eq_ignore_ascii_case("rapidocr-onnx");
  if preset.auto_ocr && !rapidocr && preset.ocr_languages.is_empty() {
    return Err("CAPCUT_OCR_LANGUAGE_UNRESOLVED".to_string());
  }
  if preset.localization.enabled && preset.localization.translate {
    let target = if preset.localization.target_language.trim().is_empty() { preset.target_language.trim() } else { preset.localization.target_language.trim() };
    if !source.eq_ignore_ascii_case("auto") {
      plan_translation_route(source, target).map_err(|error| if error.starts_with("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE") { error } else { format!("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:{error}") })?;
    }
  }
  Ok(())
}

fn retry_preflight_error(request: &StartCapcutAutomationRequest) -> Result<(), String> {
  let preset = request.preset.clone().unwrap_or_default();
  preset.validate()?;
  if preset.source_language.trim().is_empty() || preset.source_language.eq_ignore_ascii_case("auto") || preset.localization.source_language.eq_ignore_ascii_case("auto") {
    return Err("CAPCUT_SOURCE_LANGUAGE_UNRESOLVED".to_string());
  }
  validate_start_preflight(&preset)
}

/// A persisted in-flight job may contain an ffmpeg PID from a previous app
/// process. That PID is never authoritative after restart (Windows can reuse
/// it), so restored work is requeued with no process identity or progress
/// claims. The worker will assign a fresh PID when it actually spawns ffmpeg.
fn reset_restored_job_for_requeue(snapshot: &mut CapcutAutomationJob) {
  snapshot.state = CapcutAutomationJobState::Queued;
  snapshot.stage = "QUEUED".into();
  snapshot.progress = 0.0;
  snapshot.started_at = None;
  snapshot.finished_at = None;
  snapshot.processed_ms = None;
  snapshot.expected_duration_ms = None;
  snapshot.decoded_frames = 0;
  snapshot.change_candidate_frames = 0;
  snapshot.ocr_frames = 0;
  snapshot.skipped_duplicate_frames = 0;
  snapshot.raw_detection_count = 0;
  snapshot.merged_track_count = 0;
  snapshot.processing_fps = None;
  snapshot.speed_ratio = None;
  snapshot.first_progress_at = None;
  snapshot.last_progress_at = None;
  snapshot.ffmpeg_pid = None;
  snapshot.error_code = None;
  snapshot.error_message = None;
  snapshot.error = None;
  snapshot.receipt = None;
}

fn mark_restored_cancelled(snapshot: &mut CapcutAutomationJob) {
  snapshot.state = CapcutAutomationJobState::Cancelled;
  snapshot.stage = "CANCELLED".into();
  snapshot.error = Some("JOB_CANCELLED".into());
  snapshot.error_code = Some("CAPCUT_CANCELLED".into());
  snapshot.error_message = snapshot.error.clone();
  snapshot.finished_at.get_or_insert_with(now_ms);
  snapshot.ffmpeg_pid = None;
}

struct NativeStageArtifacts {
  subtitle_path: Option<PathBuf>,
  tts_audio_path: Option<PathBuf>,
  transcript_count: usize,
  translated_count: usize,
  ocr_count: usize,
  speaker_count: usize,
  tts_count: usize,
  speaker_assignment_count: usize,
  stage_timings: StageTimings,
  resource_versions: HashMap<String, String>,
  input_duration_ms: u64,
  input_dimensions: Option<MediaDimensions>,
  ocr_regions: Vec<TimedRegion>,
  requested_source_language: String,
  effective_source_language: Option<String>,
  route_kind: Option<String>,
  translation_hops: Vec<String>,
  detected_languages: Vec<String>,
}

fn write_translation_failure_receipt(job_dir: &Path, error: &str) {
  let Some(payload) = error.strip_prefix("CAPCUT_TRANSLATION_FAILED:") else {
    return;
  };
  let Ok(diagnostics) = serde_json::from_str::<serde_json::Value>(payload) else {
    return;
  };
  let path = job_dir.join("translation-failure.json");
  let partial = path.with_extension("json.partial");
  let body = serde_json::json!({
    "schemaVersion": 1,
    "errorCode": "CAPCUT_TRANSLATION_FAILED",
    "terminal": true,
    "diagnostics": diagnostics,
  });
  if let Ok(bytes) = serde_json::to_vec_pretty(&body) {
    if std::fs::write(&partial, bytes).is_ok() {
      let _ = std::fs::rename(&partial, &path);
    }
  }
}

fn announce_stage(app: &AppHandle, record: &JobRecord, stage: &str, jobs: Option<&CapcutAutomationJobManager>) -> Result<(), String> {
  let mut snapshot = record.snapshot.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
  snapshot.stage = stage.to_string();
  snapshot.stage_progress = 0.0;
  snapshot.overall_progress = stage_weight(stage).0;
  snapshot.message = Some(stage_message(stage).to_string());
  snapshot.elapsed_ms = snapshot.started_at.map(|started| now_ms().saturating_sub(started)).unwrap_or(0);
  snapshot.state = match stage {
    "PROBING" => CapcutAutomationJobState::Probing,
    "RENDERING" => CapcutAutomationJobState::Rendering,
    "VERIFYING" | "HASHING" | "WRITING_RECEIPT" => CapcutAutomationJobState::Verifying,
    _ => CapcutAutomationJobState::Preparing,
  };
  emit_progress(app, &snapshot);
  drop(snapshot);
  // Persist stage transitions immediately so a restart cannot show a stale
  // PROBING/TRANSLATING state while jobs.json already moved on.
  if let Some(jobs) = jobs {
    jobs.persist();
  }
  Ok(())
}

/// Stable stage weights keep UI progress monotonic while still exposing the
/// finer-grained progress emitted by FFmpeg.  The final stage reaches 1.0 in
/// the worker completion path.
fn stage_weight(stage: &str) -> (f64, f64) {
  match stage {
    "PROBING" => (0.00, 0.04),
    "TRANSCRIBING" => (0.04, 0.18),
    "TRANSLATING" => (0.18, 0.28),
    "OCR_SCANNING" => (0.28, 0.38),
    "DIARIZING" => (0.38, 0.50),
    "ASSIGNING_VOICES" => (0.50, 0.54),
    "SYNTHESIZING_TTS" => (0.54, 0.68),
    "BUILDING_SUBTITLES" => (0.68, 0.72),
    "BUILDING_FILTER_GRAPH" => (0.72, 0.75),
    "RENDERING" => (0.75, 0.96),
    "VERIFYING" => (0.96, 0.985),
    "HASHING" => (0.985, 0.995),
    "WRITING_RECEIPT" => (0.995, 1.0),
    "COMPLETED" => (1.0, 1.0),
    _ => (0.0, 0.0),
  }
}

fn stage_message(stage: &str) -> &'static str {
  match stage {
    "PROBING" => "Đang phân tích video",
    "TRANSCRIBING" => "Đang nhận dạng lời nói",
    "TRANSLATING" => "Đang dịch sang tiếng Việt",
    "OCR_SCANNING" => "Đang quét chữ trong video",
    "DIARIZING" => "Đang tách giọng người nói",
    "ASSIGNING_VOICES" => "Đang gán giọng đọc",
    "SYNTHESIZING_TTS" => "Đang tạo giọng đọc",
    "BUILDING_SUBTITLES" => "Đang dựng phụ đề",
    "BUILDING_FILTER_GRAPH" => "Đang chuẩn bị hiệu ứng",
    "RENDERING" => "Đang kết xuất video",
    "VERIFYING" => "Đang kiểm tra video",
    "HASHING" => "Đang tính mã kiểm tra",
    "WRITING_RECEIPT" => "Đang ghi biên nhận",
    _ => "Đang xử lý",
  }
}

fn run_ffmpeg_extract(ffmpeg: &Path, input: &Path, output: &Path, args: &[&str]) -> Result<(), String> {
  let mut command = crate::core::lifecycle::startup::tasks::background_command::background_command(std::process::Command::new(ffmpeg));
  command.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(input).args(args).arg(output);
  let status = command.status().map_err(|e| format!("LOCAL_STAGE_PROCESS_START_FAILED: {e}"))?;
  if !status.success() {
    return Err(format!("LOCAL_STAGE_PROCESS_FAILED:{}", status.code().unwrap_or(-1)));
  }
  if !output.is_file() || std::fs::metadata(output).map(|m| m.len()).unwrap_or(0) == 0 {
    return Err("LOCAL_STAGE_OUTPUT_MISSING".to_string());
  }
  Ok(())
}

fn terminate_ocr_scan_child(mut child: std::process::Child, reader: std::thread::JoinHandle<()>, stderr_reader: Option<std::thread::JoinHandle<String>>) {
  let _ = child.kill();
  let _ = child.wait();
  let _ = reader.join();
  let _ = stderr_reader.and_then(|thread| thread.join().ok());
}

/// Scan a complete video through one persistent RapidOCR worker. The worker
/// owns one FFmpeg rawvideo child and applies a low-resolution change gate;
/// synchronous NDJSON reads provide backpressure without retaining frames.
fn run_rapidocr_stream_scan<F, C>(manager: &CapcutAutomationEngineManager, ffmpeg: &Path, input: &Path, dimensions: &MediaDimensions, ocr_interval_ms: u64, on_frame: &mut F, cancelled: C) -> Result<(), String>
where
  F: FnMut(u64, u64, bool, Vec<OcrRegion>),
  C: Fn() -> bool,
{
  let engine = manager.resolve_first(super::capcut_automation_engine_manager::CapcutEngineKind::Ocr)?;
  let executable = engine.executable_path.ok_or_else(|| "CAPCUT_OCR_EXECUTABLE_MISSING".to_string())?;
  let worker = engine.worker_script_path.ok_or_else(|| "CAPCUT_OCR_WORKER_SCRIPT_MISSING".to_string())?;
  let scan_width = 960_u32;
  let scan_height = ((scan_width as f64 * dimensions.height as f64 / dimensions.width.max(1) as f64).round() as u32).max(2) & !1;
  let python_path = executable.parent().and_then(|parent| parent.parent()).and_then(|runtime| runtime.parent()).map(|root| root.join("python")).filter(|path| path.is_dir()).unwrap_or_else(|| manager.install_root().join("python"));
  let mut command = crate::core::lifecycle::startup::tasks::background_command::background_command(std::process::Command::new(&executable));
  command.args([worker.as_os_str(), std::ffi::OsStr::new("--video"), input.as_os_str(), std::ffi::OsStr::new("--ffmpeg"), ffmpeg.as_os_str(), std::ffi::OsStr::new("--width"), std::ffi::OsStr::new(&scan_width.to_string()), std::ffi::OsStr::new("--height"), std::ffi::OsStr::new(&scan_height.to_string()), std::ffi::OsStr::new("--fps"), std::ffi::OsStr::new("2"), std::ffi::OsStr::new("--min-ocr-interval-ms"), std::ffi::OsStr::new(&ocr_interval_ms.max(2_000).to_string())]).env("PYTHONPATH", python_path).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
  let mut child = command.spawn().map_err(|e| format!("CAPCUT_OCR_PROCESS_START_FAILED: {e}"))?;
  let stdout = child.stdout.take().ok_or_else(|| "CAPCUT_OCR_OUTPUT_PIPE_MISSING".to_string())?;
  let stderr = child.stderr.take();
  // Bound the protocol queue so a slow OCR consumer applies backpressure all
  // the way to the worker/FFmpeg pipe instead of accumulating NDJSON lines in
  // memory for long videos.
  let (tx, rx) = std::sync::mpsc::sync_channel::<Option<String>>(32);
  let reader = std::thread::spawn(move || {
    let mut buffered = std::io::BufReader::new(stdout);
    let mut line = String::new();
    loop {
      line.clear();
      match std::io::BufRead::read_line(&mut buffered, &mut line) {
        Ok(0) => break,
        Ok(_) if tx.send(Some(line.clone())).is_err() => break,
        Ok(_) => {},
        Err(_) => break,
      }
    }
    let _ = tx.send(None);
  });
  let stderr_reader = stderr.map(|mut stream| {
    std::thread::spawn(move || {
      let mut tail = Vec::new();
      let _ = std::io::Read::read_to_end(&mut stream, &mut tail);
      String::from_utf8_lossy(&tail[tail.len().saturating_sub(8_192)..]).to_string()
    })
  });
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1_800);
  let mut last_media_pts_ms = 0_u64;
  let mut last_media_progress = std::time::Instant::now();
  let mut closed = false;
  while !closed {
    if cancelled() {
      terminate_ocr_scan_child(child, reader, stderr_reader);
      return Err("JOB_CANCELLED".to_string());
    }
    if std::time::Instant::now() >= deadline {
      terminate_ocr_scan_child(child, reader, stderr_reader);
      return Err("CAPCUT_OCR_WORKER_TIMEOUT".to_string());
    }
    match rx.recv_timeout(std::time::Duration::from_millis(250)) {
      Ok(Some(line)) => {
        let value: serde_json::Value = match serde_json::from_str(&line) {
          Ok(value) => value,
          Err(_) => {
            terminate_ocr_scan_child(child, reader, stderr_reader);
            return Err("CAPCUT_OCR_OUTPUT_INVALID".to_string());
          },
        };
        if let Some(summary) = value.get("summary") {
          if summary.get("ffmpegScanProcessCount").and_then(|v| v.as_u64()) != Some(1) || summary.get("ocrWorkerStartCount").and_then(|v| v.as_u64()) != Some(1) {
            terminate_ocr_scan_child(child, reader, stderr_reader);
            return Err("CAPCUT_OCR_PROCESS_COUNT_INVALID".to_string());
          }
          continue;
        }
        let frame_index = value.get("frameIndex").and_then(|v| v.as_u64()).unwrap_or(0);
        let media_pts_ms = value.get("mediaPtsMs").and_then(|v| v.as_u64()).unwrap_or(0);
        let performed = value.get("ocrPerformed").and_then(|v| v.as_bool()).unwrap_or(false);
        let regions = value.get("regions").and_then(|v| v.as_array()).map(|items| items.iter().filter_map(|item| serde_json::from_value::<OcrRegion>(item.clone()).ok()).collect()).unwrap_or_default();
        on_frame(frame_index, media_pts_ms, performed, regions);
        if media_pts_ms > last_media_pts_ms {
          last_media_pts_ms = media_pts_ms;
          last_media_progress = std::time::Instant::now();
        }
        if last_media_progress.elapsed() >= std::time::Duration::from_secs(60) {
          terminate_ocr_scan_child(child, reader, stderr_reader);
          return Err("CAPCUT_SCAN_NO_PROGRESS".to_string());
        }
      },
      Ok(None) => closed = true,
      Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
        if last_media_progress.elapsed() >= std::time::Duration::from_secs(60) {
          terminate_ocr_scan_child(child, reader, stderr_reader);
          return Err("CAPCUT_SCAN_NO_PROGRESS".to_string());
        }
      },
      Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => closed = true,
    }
  }
  let _ = reader.join();
  let stderr_text = stderr_reader.and_then(|thread| thread.join().ok()).unwrap_or_default();
  let status = child.wait().map_err(|e| format!("CAPCUT_OCR_PROCESS_FAILED: {e}"))?;
  if !status.success() {
    return Err(if stderr_text.trim().is_empty() { format!("CAPCUT_OCR_FAILED:{}", status.code().unwrap_or(-1)) } else { format!("CAPCUT_OCR_FAILED:{}:{}", status.code().unwrap_or(-1), stderr_text.trim()) });
  }
  Ok(())
}

fn probe_media(ffmpeg: &Path, input: &Path) -> Result<(u64, Option<MediaDimensions>), String> {
  let probe = ffmpeg.parent().ok_or_else(|| "FFPROBE_NOT_FOUND".to_string())?.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" });
  let output = crate::core::lifecycle::startup::tasks::background_command::background_command(std::process::Command::new(probe)).args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"]).arg(input).output().map_err(|e| format!("MEDIA_PROBE_FAILED:{e}"))?;
  if !output.status.success() {
    return Err("MEDIA_PROBE_FAILED".to_string());
  }
  let value: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|_| "MEDIA_PROBE_INVALID".to_string())?;
  let duration_ms = value.get("format").and_then(|v| v.get("duration")).and_then(|v| v.as_str()).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0).map(|v| (v * 1_000.0).round() as u64).unwrap_or(0);
  let dimensions = value.get("streams").and_then(|v| v.as_array()).and_then(|streams| streams.iter().find(|s| s.get("codec_type").and_then(|v| v.as_str()) == Some("video"))).and_then(|v| Some(MediaDimensions { width: v.get("width")?.as_u64()? as u32, height: v.get("height")?.as_u64()? as u32 }));
  Ok((duration_ms, dimensions))
}

fn run_native_local_stages(app: &AppHandle, ffmpeg: &Path, input: &Path, job_dir: &Path, preset: &CapcutAutomationPresetV1, record: &JobRecord, jobs: &CapcutAutomationJobManager) -> Result<NativeStageArtifacts, String> {
  let started = std::time::Instant::now();
  announce_stage(app, record, "PROBING", Some(jobs))?;
  let (input_duration_ms, input_dimensions) = probe_media(ffmpeg, input)?;
  let mut timings = StageTimings::default();
  timings.probe_ms = started.elapsed().as_millis() as u64;
  let manager = app.state::<CapcutAutomationEngineManager>();
  let mut transcript = Vec::new();
  let mut translated = Vec::new();
  let mut ocr_regions = Vec::new();
  let mut ocr_text_regions: Vec<OcrRegion> = Vec::new();
  let mut speaker_count = 0usize;
  let mut speaker_turns: Vec<SpeakerTurn> = Vec::new();
  let mut speaker_assignment_count = 0usize;
  let mut tts_audio_path = None;
  // Piper concatenates clips using their measured durations. Keep those
  // timings so burned subtitles follow the spoken audio instead of the
  // original Whisper timestamps (which drift as soon as translated speech
  // is shorter or longer than the source segment).
  let mut tts_segment_timings: std::collections::HashMap<String, (u64, u64)> = std::collections::HashMap::new();
  let mut resource_versions = HashMap::new();
  let requested_source_language = if preset.localization.source_language.trim().is_empty() || preset.localization.source_language == "auto" { preset.source_language.clone() } else { preset.localization.source_language.clone() };
  let mut effective_source_language: Option<String> = None;
  let mut route_kind: Option<String> = None;
  let mut translation_hops: Vec<String> = Vec::new();

  if preset.auto_transcribe {
    let clock = std::time::Instant::now();
    announce_stage(app, record, "TRANSCRIBING", Some(jobs))?;
    // Whisper.cpp consumes decoded PCM reliably, not every container/codec
    // combination.  Decode the input through the same ArtCraft-owned FFmpeg
    // runtime used by the render stage, then pass the temporary WAV to the
    // local transcription adapter.  Keep the intermediate scoped to the job
    // directory and remove it regardless of the adapter result.
    let audio = job_dir.join("transcribe.wav");
    run_ffmpeg_extract(ffmpeg, input, &audio, &["-vn", "-ac", "1", "-ar", "16000", "-f", "wav"])?;
    let response_result = transcribe_local(&manager, LocalTranscriptionRequest { input_path: audio.to_string_lossy().to_string(), language: (preset.source_language != "auto").then(|| preset.source_language.clone()), workspace_path: Some(job_dir.to_string_lossy().to_string()) });
    let _ = std::fs::remove_file(&audio);
    let response = response_result?;
    resource_versions.insert("transcription".into(), response.model_sha256);
    transcript = response.segments;
    timings.transcription_ms = clock.elapsed().as_millis() as u64;
    validate_transcript_quality(&transcript)?;
  }
  if preset.auto_translate {
    let clock = std::time::Instant::now();
    announce_stage(app, record, "TRANSLATING", Some(jobs))?;
    if record.cancel.load(Ordering::SeqCst) {
      return Err("JOB_CANCELLED".to_string());
    }
    if transcript.is_empty() {
      return Err("CAPCUT_TRANSLATION_INPUT_MISSING".to_string());
    }
    let source = transcript.iter().find_map(|s| s.language.clone()).filter(|s| !s.eq_ignore_ascii_case("auto")).unwrap_or_else(|| {
      let joined = transcript.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
      let detected = infer_text_language(&joined);
      if detected == "unknown" {
        preset.source_language.clone()
      } else {
        detected
      }
    });
    let transcript_segments = transcript.iter().map(|s| TranslationSegment { id: s.id.clone(), start_ms: s.start_ms, end_ms: s.end_ms, text: s.text.clone(), translated_text: None }).collect::<Vec<_>>();
    if preset.source_language.eq_ignore_ascii_case("auto") || preset.localization.source_language.eq_ignore_ascii_case("auto") {
      // Auto detection is resolved per segment so mixed-language transcripts do
      // not accidentally send Chinese through the en→vi model. Each returned
      // route is persisted as evidence of the actual hop(s) used.
      let (response_segments, routes) = match translate_segments_by_language(&manager, &transcript_segments, &preset.target_language) {
        Ok(value) => value,
        Err(error) => {
          write_translation_failure_receipt(job_dir, &error);
          return Err(error);
        },
      };
      translated = response_segments;
      if let Some(route) = routes.first() {
        effective_source_language = Some(route.effective_source_language.clone());
        route_kind = Some(
          match route.route_kind {
            TranslationRouteKind::Direct => "DIRECT",
            TranslationRouteKind::Pivot => "PIVOT",
          }
          .to_string(),
        );
      }
      let mut seen_hops = std::collections::HashSet::new();
      for route in routes {
        for hop in route.hops {
          let key = format!("{}→{}:{}", hop.source_language, hop.target_language, hop.model_id);
          if seen_hops.insert(key.clone()) {
            translation_hops.push(key);
          }
          if let Ok(model) = manager.model(&hop.model_id) {
            resource_versions.insert(format!("translation_model_{}_sha256", hop.model_id), model.sha256);
          }
        }
      }
      resource_versions.insert("translation_route".into(), translation_hops.join("|"));
    } else if source == preset.target_language || source == "vi" {
      translated = transcript_segments
        .clone()
        .into_iter()
        .map(|mut s| {
          s.translated_text = Some(s.text.clone());
          s
        })
        .collect();
    } else {
      let route = plan_translation_route(&source, &preset.target_language)?;
      effective_source_language = Some(route.effective_source_language.clone());
      route_kind = Some(
        match route.route_kind {
          TranslationRouteKind::Direct => "DIRECT",
          TranslationRouteKind::Pivot => "PIVOT",
        }
        .to_string(),
      );
      translation_hops = route.hops.iter().map(|hop| format!("{}→{}:{}", hop.source_language, hop.target_language, hop.model_id)).collect();
      for hop in &route.hops {
        if let Ok(model) = manager.model(&hop.model_id) {
          resource_versions.insert(format!("translation_model_{}_sha256", hop.model_id), model.sha256);
        }
      }
      let response = match translate_local(&manager, LocalTranslationRequest { segments: transcript.iter().map(|s| TranslationSegment { id: s.id.clone(), start_ms: s.start_ms, end_ms: s.end_ms, text: s.text.clone(), translated_text: None }).collect(), source_language: source, target_language: preset.target_language.clone() }) {
        Ok(response) => response,
        Err(error) => {
          write_translation_failure_receipt(job_dir, &error);
          return Err(error);
        },
      };
      resource_versions.insert("translation".into(), response.model.clone());
      resource_versions.insert("translation_route".into(), translation_hops.join("|"));
      if let Ok(model) = manager.model("argos-en-vi-1-9") {
        resource_versions.insert("translation_model_sha256".into(), model.sha256);
      }
      translated = response.segments;
    }
    if translated.iter().any(|s| s.translated_text.as_deref().unwrap_or("").trim().is_empty()) {
      return Err("CAPCUT_TRANSLATION_OUTPUT_INCOMPLETE".to_string());
    }
    if record.cancel.load(Ordering::SeqCst) {
      return Err("JOB_CANCELLED".to_string());
    }
    validate_translation_quality_against_source(&translated, &transcript_segments)?;
    timings.translation_ms = clock.elapsed().as_millis() as u64;
  }
  if preset.auto_ocr {
    let clock = std::time::Instant::now();
    announce_stage(app, record, "OCR_SCANNING", Some(jobs))?;
    let dimensions = input_dimensions.clone().ok_or_else(|| "MEDIA_DIMENSIONS_MISSING".to_string())?;
    // Sample the timeline instead of scanning a single still.  This keeps the
    // region's lifetime explicit and lets the renderer mask only the interval
    // in which text was actually observed.
    let interval = preset.ocr_sample_interval_ms.max(250);
    let rapidocr = preset.ocr_engine.trim().eq_ignore_ascii_case("rapidocr-onnxruntime") || preset.ocr_engine.trim().eq_ignore_ascii_case("rapidocr-onnx");
    if rapidocr {
      let scan_started = std::time::Instant::now();
      let mut last_emit_at = std::time::Instant::now() - std::time::Duration::from_secs(1);
      let mut last_persist_at = std::time::Instant::now() - std::time::Duration::from_secs(2);
      let mut scan = |frame_index: u64, media_ms: u64, performed: bool, observed: Vec<OcrRegion>| {
        let observed_count = observed.len() as u64;
        let now = now_ms();
        if !performed {
          let mut persist = false;
          if let Ok(mut snapshot) = record.snapshot.lock() {
            snapshot.decoded_frames = frame_index.saturating_add(1);
            snapshot.skipped_duplicate_frames = snapshot.skipped_duplicate_frames.saturating_add(1);
            snapshot.processed_ms = Some(media_ms.min(input_duration_ms));
            snapshot.stage_progress = progress_fraction(media_ms as f64, Some(input_duration_ms as f64));
            snapshot.overall_progress = 0.28 + 0.10 * snapshot.stage_progress;
            snapshot.processing_fps = Some((frame_index.saturating_add(1)) as f64 / scan_started.elapsed().as_secs_f64().max(0.001));
            snapshot.first_progress_at.get_or_insert(now);
            snapshot.last_progress_at = Some(now);
            if last_emit_at.elapsed() >= std::time::Duration::from_millis(250) {
              emit_progress(app, &snapshot);
              last_emit_at = std::time::Instant::now();
            }
            if last_persist_at.elapsed() >= std::time::Duration::from_secs(1) {
              persist = true;
              last_persist_at = std::time::Instant::now();
            }
          }
          if persist {
            jobs.persist();
          }
          return;
        }
        for region in observed {
          if !ocr_text_regions.iter().any(|existing| existing.text == region.text && (existing.x - region.x).abs() < 0.03 && (existing.y - region.y).abs() < 0.03) {
            ocr_text_regions.push(region.clone());
          }
          let duplicate = ocr_regions.iter().any(|existing: &TimedRegion| {
            let same_area = (existing.x - region.x as f64).abs() < 0.03 && (existing.y - region.y as f64).abs() < 0.03 && (existing.width - region.width as f64).abs() < 0.05 && (existing.height - region.height as f64).abs() < 0.05;
            same_area && existing.end_ms.saturating_add(interval) >= media_ms
          });
          if duplicate {
            if let Some(existing) = ocr_regions.iter_mut().find(|existing| (existing.x - region.x as f64).abs() < 0.03 && (existing.y - region.y as f64).abs() < 0.03 && (existing.width - region.width as f64).abs() < 0.05 && (existing.height - region.height as f64).abs() < 0.05) {
              existing.end_ms = (media_ms + interval).min(input_duration_ms.max(1));
            }
          } else {
            ocr_regions.push(TimedRegion { start_ms: media_ms, end_ms: (media_ms + interval).min(input_duration_ms.max(1)), x: region.x as f64, y: region.y as f64, width: region.width as f64, height: region.height as f64 });
          }
        }
        let mut persist = false;
        if let Ok(mut snapshot) = record.snapshot.lock() {
          snapshot.processed_ms = Some(media_ms.min(input_duration_ms));
          snapshot.decoded_frames = frame_index.saturating_add(1);
          snapshot.change_candidate_frames = snapshot.change_candidate_frames.saturating_add(1);
          snapshot.ocr_frames = snapshot.ocr_frames.saturating_add(1);
          snapshot.raw_detection_count = snapshot.raw_detection_count.saturating_add(observed_count);
          snapshot.merged_track_count = ocr_regions.len() as u64;
          snapshot.stage_progress = progress_fraction(media_ms as f64, Some(input_duration_ms as f64));
          snapshot.overall_progress = 0.28 + 0.10 * snapshot.stage_progress;
          snapshot.processing_fps = Some((frame_index.saturating_add(1)) as f64 / scan_started.elapsed().as_secs_f64().max(0.001));
          snapshot.first_progress_at.get_or_insert(now);
          snapshot.last_progress_at = Some(now);
          snapshot.message = Some(format!("Đang quét chữ: {}s / {}s", media_ms / 1000, input_duration_ms / 1000));
          if last_emit_at.elapsed() >= std::time::Duration::from_millis(250) {
            emit_progress(app, &snapshot);
            last_emit_at = std::time::Instant::now();
          }
          if last_persist_at.elapsed() >= std::time::Duration::from_secs(1) {
            persist = true;
            last_persist_at = std::time::Instant::now();
          }
        }
        if persist {
          jobs.persist();
        }
      };
      run_rapidocr_stream_scan(&manager, ffmpeg, input, &dimensions, preset.ocr_sample_interval_ms, &mut scan, || record.cancel.load(Ordering::SeqCst))?;
    } else {
      let mut sample_ms = 0_u64;
      let mut sample_index = 0_u32;
      while sample_ms < input_duration_ms.max(1) {
        if record.cancel.load(Ordering::SeqCst) {
          return Err("JOB_CANCELLED".into());
        }
        let frame = job_dir.join(format!("ocr-frame-{sample_index}.png"));
        let seek = format!("{:.3}", sample_ms as f64 / 1000.0);
        run_ffmpeg_extract(ffmpeg, input, &frame, &["-ss", &seek, "-frames:v", "1"])?;
        let observed = recognize_local(&manager, LocalOcrRequest { image_path: frame.to_string_lossy().to_string(), language: preset.ocr_languages.first().cloned(), image_width: dimensions.width, image_height: dimensions.height })?.regions;
        for region in observed {
          // Keep a separate text-bearing inventory for the optional offline OCR
          // translation route.  TimedRegion is intentionally render-only, so
          // translation must happen before text is discarded.
          if !ocr_text_regions.iter().any(|existing| existing.text == region.text && (existing.x - region.x).abs() < 0.03 && (existing.y - region.y).abs() < 0.03) {
            ocr_text_regions.push(region.clone());
          }
          let duplicate = ocr_regions.iter().any(|existing: &TimedRegion| {
            let same_area = (existing.x - region.x as f64).abs() < 0.03 && (existing.y - region.y as f64).abs() < 0.03 && (existing.width - region.width as f64).abs() < 0.05 && (existing.height - region.height as f64).abs() < 0.05;
            same_area && existing.end_ms.saturating_add(interval) >= sample_ms
          });
          if duplicate {
            if let Some(existing) = ocr_regions.iter_mut().find(|existing| (existing.x - region.x as f64).abs() < 0.03 && (existing.y - region.y as f64).abs() < 0.03 && (existing.width - region.width as f64).abs() < 0.05 && (existing.height - region.height as f64).abs() < 0.05) {
              existing.end_ms = (sample_ms + interval).min(input_duration_ms.max(1));
            }
          } else {
            ocr_regions.push(TimedRegion { start_ms: sample_ms, end_ms: (sample_ms + interval).min(input_duration_ms.max(1)), x: region.x as f64, y: region.y as f64, width: region.width as f64, height: region.height as f64 });
          }
        }
        let _ = std::fs::remove_file(frame);
        sample_ms = sample_ms.saturating_add(interval);
        sample_index = sample_index.saturating_add(1);
      }
    }
    if preset.auto_translate && !ocr_text_regions.is_empty() {
      // OCR language codes are engine-specific while the translation boundary
      // uses normalized language codes. Keep Chinese explicit so it routes
      // through a verified Chinese→English→Vietnamese pivot.
      let ocr_source_language = if preset.source_language == "auto" {
        match preset.ocr_languages.first().map(String::as_str) {
          Some("eng" | "en") => "en".to_string(),
          Some("chi_sim" | "zh" | "zh-cn" | "zh-hans") => "zh".to_string(),
          Some("chi_tra" | "zt" | "zh-tw" | "zh-hant") => "zt".to_string(),
          Some(other) => other.to_string(),
          None => infer_ocr_source_language(&ocr_text_regions),
        }
      } else {
        preset.source_language.clone()
      };
      let translated = translate_regions_local(&manager, LocalOcrTranslationRequest { regions: ocr_text_regions.clone(), source_language: ocr_source_language.clone(), target_language: preset.target_language.clone() })?;
      if let Ok(route) = plan_translation_route(&ocr_source_language, &preset.target_language) {
        effective_source_language = Some(route.effective_source_language.clone());
        route_kind = Some(
          match route.route_kind {
            TranslationRouteKind::Direct => "DIRECT",
            TranslationRouteKind::Pivot => "PIVOT",
          }
          .to_string(),
        );
        translation_hops = route.hops.iter().map(|hop| format!("{}→{}:{}", hop.source_language, hop.target_language, hop.model_id)).collect();
      }
      if translated.regions.iter().any(|region| region.translated_text.trim().is_empty()) {
        return Err("CAPCUT_OCR_TRANSLATION_OUTPUT_INCOMPLETE".to_string());
      }
      resource_versions.insert("ocr_translation".into(), translated.model);
    }
    timings.ocr_ms = clock.elapsed().as_millis() as u64;
  }
  if preset.auto_diarize {
    let clock = std::time::Instant::now();
    announce_stage(app, record, "DIARIZING", Some(jobs))?;
    let audio = job_dir.join("diarize.wav");
    run_ffmpeg_extract(ffmpeg, input, &audio, &["-vn", "-ac", "1", "-ar", "16000", "-f", "wav"])?;
    speaker_turns = detect_speakers_local(&manager, LocalSpeakerRequest { audio_path: audio.to_string_lossy().to_string(), num_speakers: 0 })?.turns;
    speaker_count = speaker_turns.iter().map(|t| t.speaker_id.clone()).collect::<std::collections::HashSet<_>>().len();
    let _ = std::fs::remove_file(audio);
    timings.diarization_ms = clock.elapsed().as_millis() as u64;
  }
  if preset.auto_diarize {
    let clock = std::time::Instant::now();
    announce_stage(app, record, "ASSIGNING_VOICES", Some(jobs))?;
    if transcript.is_empty() {
      return Err("CAPCUT_ASSIGNMENT_INPUT_MISSING".to_string());
    }
    let segments = transcript.iter().map(|segment| SpeakerSegment { id: segment.id.clone(), start_ms: segment.start_ms, end_ms: segment.end_ms, text: segment.text.clone() }).collect::<Vec<_>>();
    speaker_assignment_count = assign_speakers(&segments, &speaker_turns, &preset.speaker_voice_assignments).len();
    if speaker_assignment_count == 0 && !speaker_turns.is_empty() {
      return Err("CAPCUT_SPEAKER_ASSIGNMENT_EMPTY".to_string());
    }
    // Assignment is deterministic and intentionally local; retain its timing
    // in the diarization bucket until the receipt schema gains a dedicated
    // assignment field.
    timings.diarization_ms = timings.diarization_ms.saturating_add(clock.elapsed().as_millis() as u64);
  }
  let mut subtitle_path = None;
  if preset.auto_tts {
    let clock = std::time::Instant::now();
    announce_stage(app, record, "SYNTHESIZING_TTS", Some(jobs))?;
    let tts_segments = if !translated.is_empty() { translated.clone() } else { transcript.iter().map(|s| TranslationSegment { id: s.id.clone(), start_ms: s.start_ms, end_ms: s.end_ms, text: s.text.clone(), translated_text: Some(s.text.clone()) }).collect() };
    if tts_segments.is_empty() {
      return Err("CAPCUT_TTS_INPUT_MISSING".to_string());
    }
    let scenes = tts_segments.iter().enumerate().map(|(index, segment)| ScriptScene { id: segment.id.clone(), index: index as u32, narration: segment.translated_text.clone().unwrap_or_else(|| segment.text.clone()), caption: segment.translated_text.clone().unwrap_or_else(|| segment.text.clone()), visual_instruction: String::new(), search_keywords: Vec::new(), emotion: String::new(), duration_ms: segment.end_ms.saturating_sub(segment.start_ms) }).collect();
    let script = StructuredScript { title: "CapCut Automation".into(), hook: preset.hook.text.clone(), cta: String::new(), language: preset.target_language.clone(), target_duration_seconds: (input_duration_ms / 1000) as u32, scenes };
    // The provider is persisted with the job. A selected clone must never be
    // sent to Piper, which cannot resolve a voice-profile ID.
    let selected_voice = preset.voice_profile_id.as_deref().filter(|value| !value.trim().is_empty()).map(str::to_owned).unwrap_or_else(|| configured_voice_id("vi"));
    let selected_model = if preset.voice_provider.eq_ignore_ascii_case("ARTCRAFT_SPEECH") { format!("artcraft-speech/{}", preset.voice_model.trim()) } else { configured_tts_model() };
    let input = VoiceInput { script_artifact_id: record.snapshot.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?.job_id.clone(), script, voice: selected_voice, language: preset.target_language.clone(), model: selected_model, piper_executable: None, piper_model: None };
    let output = tauri::async_runtime::block_on(synthesize_voice_with_runtime(app, &input, job_dir, Arc::new(AtomicBool::new(false)))).map_err(|error| format!("CAPCUT_TTS_FAILED:{}", error.code))?;
    tts_audio_path = Some(output.audio_path);
    tts_segment_timings = output.timing.segments.into_iter().map(|segment| (segment.scene_id, ((segment.start_seconds * 1_000.0).round() as u64, (segment.end_seconds * 1_000.0).round() as u64))).collect();
    timings.tts_ms = clock.elapsed().as_millis() as u64;
  }
  if preset.localization.burn_subtitles && (!translated.is_empty() || !preset.localization.manual_cues.is_empty()) {
    let clock = std::time::Instant::now();
    announce_stage(app, record, "BUILDING_SUBTITLES", Some(jobs))?;
    let mut subtitle_preset = preset.clone();
    if !translated.is_empty() {
      subtitle_preset.localization.manual_cues = translated
        .iter()
        .map(|s| {
          let (start_ms, end_ms) = tts_segment_timings.get(&s.id).copied().unwrap_or((s.start_ms, s.end_ms));
          SubtitleCue { start_ms, end_ms: end_ms.max(start_ms.saturating_add(1)), text: s.translated_text.clone().unwrap_or_else(|| s.text.clone()), enabled: true }
        })
        .collect();
    }
    let path = job_dir.join("subtitles.ass");
    write_manual_subtitle_ass(&path, &subtitle_preset)?;
    timings.subtitle_ms = clock.elapsed().as_millis() as u64;
    subtitle_path = Some(path);
  }
  let regions = ocr_regions;
  let ocr_count = regions.len();
  let mut detected_languages = std::collections::BTreeSet::new();
  for segment in &transcript {
    let language = segment.language.clone().filter(|value| !value.eq_ignore_ascii_case("auto")).unwrap_or_else(|| infer_text_language(&segment.text));
    if language != "unknown" {
      detected_languages.insert(language);
    }
  }
  for region in &ocr_text_regions {
    let language = infer_text_language(&region.text);
    if language != "unknown" {
      detected_languages.insert(language);
    }
  }
  let tts_count = if tts_audio_path.is_some() { translated.len().max(transcript.len()) } else { 0 };
  let total_ms = started.elapsed().as_millis() as u64;
  timings.total_ms = total_ms;
  Ok(NativeStageArtifacts { subtitle_path, tts_audio_path, transcript_count: transcript.len(), translated_count: translated.len(), ocr_count, speaker_count, tts_count, speaker_assignment_count, stage_timings: timings, resource_versions, input_duration_ms, input_dimensions, ocr_regions: regions, requested_source_language, effective_source_language, route_kind, translation_hops, detected_languages: detected_languages.into_iter().collect() })
}

fn render_job(app: &AppHandle, root: &AppDataRoot, record: &JobRecord, jobs: &CapcutAutomationJobManager) -> Result<CapcutAutomationReceipt, String> {
  let request = &record.request;
  let preset = request.preset.clone().unwrap_or_default();
  let input = PathBuf::from(request.input_path.trim()).canonicalize().map_err(|e| format!("INPUT_NOT_READABLE: {e}"))?;
  let ffmpeg = tauri::async_runtime::block_on(app_lib::services::get_ffmpeg_path(app)).ok_or_else(|| "RENDER_START_FAILED: packaged FFmpeg is unavailable".to_string())?;
  let output_root = request.output_root.clone().unwrap_or_else(|| root.path().join("outputs").to_string_lossy().to_string());
  let output_dir = OutputPathResolver::prepare_output_directory(&output_root, request.page_name.as_deref().unwrap_or("ArtCraft"))?;
  let (job_id, request_id, attempt_id) = {
    let snapshot = record.snapshot.lock().map_err(|_| "JOB_MANAGER_LOCK_FAILED".to_string())?;
    (snapshot.job_id.clone(), snapshot.request_id.clone(), snapshot.attempt_id.clone())
  };
  let job_dir = attempt_workspace(root.path(), &job_id, &attempt_id);
  std::fs::create_dir_all(&job_dir).map_err(|e| format!("TEMP_DIRECTORY_CREATE_FAILED: {e}"))?;
  let rendered = job_dir.join(format!("rendered.{}", preset.output.container));
  let stage_artifacts = run_native_local_stages(app, &ffmpeg, &input, &job_dir, &preset, record, jobs)?;
  let mut render_preset = preset.clone();
  if !stage_artifacts.ocr_regions.is_empty() {
    render_preset.foreign_text.manual_regions.extend(stage_artifacts.ocr_regions.clone());
  }
  let complexity = estimate_render_graph_complexity(&render_preset, stage_artifacts.input_duration_ms);
  validate_render_graph_complexity(&complexity)?;
  let hook_path = if render_preset.hook.enabled {
    let path = job_dir.join("hook.ass");
    write_hook_ass(&path, &render_preset)?;
    Some(path)
  } else {
    None
  };
  let imported_subtitle_path = match preset.localization.subtitle_path.as_deref().map(Path::new) {
    Some(path) if !path.is_file() => return Err("SUBTITLE_FILE_NOT_FOUND".to_string()),
    Some(path) => Some(path),
    None => None,
  };
  let manual_subtitle_path = if stage_artifacts.subtitle_path.is_none() && render_preset.localization.manual_cues.iter().any(|cue| cue.enabled) {
    let path = job_dir.join("subtitles.ass");
    write_manual_subtitle_ass(&path, &render_preset)?;
    Some(path)
  } else {
    None
  };
  let subtitle_path = stage_artifacts.subtitle_path.as_deref().or(manual_subtitle_path.as_deref()).or(imported_subtitle_path);
  let expected_duration_ms = Some(output_time_ms(stage_artifacts.input_duration_ms, render_preset.playback_rate));
  if let Ok(mut state) = record.snapshot.lock() {
    state.state = CapcutAutomationJobState::Preparing;
    state.stage = "BUILDING_FILTER_GRAPH".into();
    state.expected_duration_ms = expected_duration_ms;
    emit_progress(app, &state);
  }
  let manager = record;
  let render_started = std::time::Instant::now();
  let mut last_render_emit_at = std::time::Instant::now() - std::time::Duration::from_secs(1);
  let mut last_render_persist_at = std::time::Instant::now() - std::time::Duration::from_secs(2);
  announce_stage(app, record, "RENDERING", Some(jobs))?;
  let rendered_hashes = render_video_with_progress_and_pid_and_audio(
    &ffmpeg,
    &input,
    &rendered,
    &render_preset,
    subtitle_path,
    hook_path.as_deref(),
    stage_artifacts.tts_audio_path.as_deref(),
    |pid| {
      if let Ok(mut state) = manager.snapshot.lock() {
        state.ffmpeg_pid = Some(pid);
        emit_progress(app, &state);
      }
    },
    |progress| {
      let mut persist = false;
      if let Ok(mut s) = manager.snapshot.lock() {
        s.progress = progress;
        s.stage_progress = progress.clamp(0.0, 1.0);
        s.overall_progress = 0.75 + (0.21 * progress.clamp(0.0, 1.0));
        s.elapsed_ms = s.started_at.map(|started| now_ms().saturating_sub(started)).unwrap_or(0);
        s.estimated_remaining_ms = s.expected_duration_ms.and_then(|expected| {
          let processed = (progress * expected as f64).round() as u64;
          (progress > 0.0 && progress < 1.0).then_some(expected.saturating_sub(processed))
        });
        s.message = Some(stage_message("RENDERING").into());
        s.processed_ms = s.expected_duration_ms.map(|expected| (progress * expected as f64).round() as u64);
        if last_render_emit_at.elapsed() >= std::time::Duration::from_millis(250) || progress >= 1.0 {
          emit_progress(app, &s);
          last_render_emit_at = std::time::Instant::now();
        }
        if last_render_persist_at.elapsed() >= std::time::Duration::from_secs(1) || progress >= 1.0 {
          persist = true;
          last_render_persist_at = std::time::Instant::now();
        }
      }
      if persist {
        jobs.persist();
      }
    },
    || manager.cancel.load(Ordering::SeqCst),
  )?;
  let render_elapsed_ms = render_started.elapsed().as_millis() as u64;
  if let Some(path) = hook_path {
    let _ = std::fs::remove_file(path);
  }
  let verify_started = std::time::Instant::now();
  let input_sha = file_hashes(&input)?.0;
  if rendered_hashes.0 == input_sha {
    return Err("ARTIFACT_VERIFY_FAILED: output is identical to input".into());
  }
  let filename = OutputPathResolver::generate_final_filename(&job_id, "rendered_video", &preset.output.container);
  let final_path = OutputPathResolver::publish_final_file(&rendered, &output_dir, &filename)?;
  let receipt_path = job_dir.join("receipt.json");
  // Receipt describes the published artifact, not the source.  The render
  // timeline may shorten the video (for example playbackRate=1.1), so probe
  // the final path after the atomic publish.
  let duration_ms = media_duration_ms(&ffmpeg, &final_path).map(|value| value.round() as u64).unwrap_or(0);
  let verify_elapsed_ms = verify_started.elapsed().as_millis() as u64;
  announce_stage(app, record, "HASHING", Some(jobs))?;
  let hash_started = std::time::Instant::now();
  let (output_sha, output_md5) = file_hashes(&final_path)?;
  let hash_elapsed_ms = hash_started.elapsed().as_millis() as u64;
  let completed_at = now_ms();
  let mut stage_timings = stage_artifacts.stage_timings;
  stage_timings.render_ms = render_elapsed_ms;
  stage_timings.verify_ms = verify_elapsed_ms;
  stage_timings.hash_ms = hash_elapsed_ms;
  stage_timings.total_ms = completed_at.saturating_sub(record.snapshot.lock().ok().and_then(|s| s.started_at).unwrap_or(completed_at));
  let progress_snapshot = record.snapshot.lock().ok().map(|snapshot| snapshot.clone());
  let rapidocr_enabled = preset.ocr_engine.eq_ignore_ascii_case("rapidocr-onnxruntime") || preset.ocr_engine.eq_ignore_ascii_case("rapidocr-onnx");
  let detected_languages = stage_artifacts.detected_languages.clone();
  let output_fps = render_preset.output.fps.trim().parse::<f64>().ok().filter(|fps| fps.is_finite() && *fps > 0.0).unwrap_or(30.0);
  let receipt = CapcutAutomationReceipt {
    schema_version: 1,
    job_id,
    request_id,
    input_sha256: input_sha,
    output_sha256: output_sha,
    output_md5,
    output_path: final_path.to_string_lossy().to_string(),
    duration_ms,
    width: preset.output.width,
    height: preset.output.height,
    video_codec: preset.output.video_codec.clone(),
    audio_codec: preset.output.audio_codec.clone(),
    playback_rate: preset.playback_rate,
    subtitle_burned: subtitle_path.is_some() && preset.localization.burn_subtitles,
    subtitle_mode: if stage_artifacts.subtitle_path.is_some() {
      "AUTO".into()
    } else if !preset.localization.manual_cues.is_empty() {
      "MANUAL".into()
    } else if imported_subtitle_path.is_some() {
      "IMPORT".into()
    } else {
      "NONE".into()
    },
    subtitle_cue_count: if stage_artifacts.translated_count > 0 { stage_artifacts.translated_count } else { preset.localization.manual_cues.iter().filter(|cue| cue.enabled).count() },
    translation_output_mode: preset.translation_output_mode.clone(),
    original_audio_policy: preset.original_audio_policy.clone(),
    tts_audio_mode: preset.tts_audio_mode.clone(),
    hook_applied: preset.hook.enabled,
    foreign_text_regions_applied: if render_preset.foreign_text.enabled { render_preset.foreign_text.manual_regions.len() } else { 0 },
    terminal: true,
    state: "COMPLETED".into(),
    preset_id: render_preset.preset_id.clone(),
    created_at: record.snapshot.lock().ok().map(|s| s.created_at),
    started_at: record.snapshot.lock().ok().and_then(|s| s.started_at),
    completed_at: Some(completed_at),
    input_path: Some(input.to_string_lossy().to_string()),
    input_duration_ms: Some(stage_artifacts.input_duration_ms),
    input_dimensions: stage_artifacts.input_dimensions.clone(),
    output_dimensions: Some(MediaDimensions { width: preset.output.width, height: preset.output.height }),
    encoder: Some("libx264+aac".into()),
    stage_timings: Some(stage_timings),
    transcript_segment_count: stage_artifacts.transcript_count,
    translated_segment_count: stage_artifacts.translated_count,
    ocr_region_count: stage_artifacts.ocr_count,
    speaker_count: stage_artifacts.speaker_count,
    speaker_assignment_count: stage_artifacts.speaker_assignment_count,
    tts_segment_count: stage_artifacts.tts_count,
    input_md5: Some(file_hashes(&input)?.1),
    resource_versions: stage_artifacts.resource_versions,
    requested_source_language: Some(stage_artifacts.requested_source_language),
    effective_source_language: stage_artifacts.effective_source_language.clone(),
    ocr_languages: preset.ocr_languages.clone(),
    route_kind: stage_artifacts.route_kind,
    translation_hops: stage_artifacts.translation_hops,
    render_track_count: complexity.render_track_count,
    filter_node_count: complexity.filter_node_count,
    estimated_output_frames: complexity.estimated_output_frames,
    output_fps: Some(output_fps),
    detected_languages: detected_languages.clone(),
    dominant_detected_language: stage_artifacts.effective_source_language.clone(),
    language_detection_confidence: (!detected_languages.is_empty()).then_some(1.0),
    language_evidence: [("audio".into(), stage_artifacts.transcript_count > 0), ("ocr".into(), stage_artifacts.ocr_count > 0)].into_iter().collect(),
    scan_fps: progress_snapshot.as_ref().and_then(|snapshot| snapshot.processing_fps),
    ffmpeg_scan_process_count: if rapidocr_enabled { 1 } else { 0 },
    ocr_worker_start_count: if rapidocr_enabled { 1 } else { 0 },
    onnx_model_load_count: if rapidocr_enabled { 1 } else { 0 },
    decoded_frame_count: progress_snapshot.as_ref().map(|snapshot| snapshot.decoded_frames).unwrap_or(0),
    change_candidate_count: progress_snapshot.as_ref().map(|snapshot| snapshot.change_candidate_frames).unwrap_or(0),
    ocr_frame_count: progress_snapshot.as_ref().map(|snapshot| snapshot.ocr_frames).unwrap_or(0),
    skipped_duplicate_count: progress_snapshot.as_ref().map(|snapshot| snapshot.skipped_duplicate_frames).unwrap_or(0),
    raw_detection_count: progress_snapshot.as_ref().map(|snapshot| snapshot.raw_detection_count).unwrap_or(0),
    merged_track_count: progress_snapshot.as_ref().map(|snapshot| snapshot.merged_track_count).unwrap_or(complexity.render_track_count as u64),
    first_scan_progress_at: progress_snapshot.as_ref().and_then(|snapshot| snapshot.first_progress_at),
    scan_finished_at: Some(completed_at),
    first_render_progress_at: progress_snapshot.as_ref().and_then(|snapshot| snapshot.first_progress_at),
    last_render_progress_at: progress_snapshot.as_ref().and_then(|snapshot| snapshot.last_progress_at),
    max_no_progress_ms: Some(60_000),
    ..Default::default()
  };
  announce_stage(app, record, "WRITING_RECEIPT", Some(jobs))?;
  write_receipt(&receipt_path, &receipt)?;
  Ok(receipt)
}

fn attempt_workspace(root: &Path, job_id: &str, attempt_id: &str) -> PathBuf {
  root.join("video-jobs").join(job_id).join(attempt_id)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn snapshot(job_id: &str, state: CapcutAutomationJobState, pid: Option<u32>) -> CapcutAutomationJob {
    CapcutAutomationJob { job_id: job_id.into(), request_id: format!("request-{job_id}"), attempt: 1, attempt_id: format!("{job_id}-attempt-1"), dispatch_count: 0, retry_count: 0, last_worker_invocation_at: None, input_path: "input.mp4".into(), output_root: None, page_name: "Page".into(), state, progress: 0.7, stage_progress: 0.7, overall_progress: 0.9, elapsed_ms: 700, estimated_remaining_ms: Some(300), decoded_frames: 0, change_candidate_frames: 0, ocr_frames: 0, skipped_duplicate_frames: 0, raw_detection_count: 0, merged_track_count: 0, processing_fps: None, speed_ratio: None, first_progress_at: None, last_progress_at: None, message: Some("stale".into()), stage: "RENDERING".into(), created_at: 1, started_at: Some(2), finished_at: None, processed_ms: Some(700), expected_duration_ms: Some(1_000), ffmpeg_pid: pid, error_code: Some("STALE".into()), error_message: Some("stale".into()), error: Some("stale".into()), receipt: None }
  }

  #[test]
  fn restart_requeues_in_flight_job_without_trusting_stale_pid() {
    let mut job = snapshot("job-1", CapcutAutomationJobState::Rendering, Some(42_424));
    reset_restored_job_for_requeue(&mut job);
    assert!(matches!(job.state, CapcutAutomationJobState::Queued));
    assert_eq!(job.stage, "QUEUED");
    assert_eq!(job.progress, 0.0);
    assert_eq!(job.ffmpeg_pid, None);
    assert_eq!(job.started_at, None);
    assert_eq!(job.processed_ms, None);
    assert_eq!(job.error, None);
  }

  #[test]
  fn restart_does_not_requeue_cancel_requested_job() {
    let mut job = snapshot("job-cancelled", CapcutAutomationJobState::CancelRequested, Some(42_424));
    mark_restored_cancelled(&mut job);
    assert!(matches!(job.state, CapcutAutomationJobState::Cancelled));
    assert_eq!(job.stage, "CANCELLED");
    assert_eq!(job.error.as_deref(), Some("JOB_CANCELLED"));
    assert_eq!(job.error_code.as_deref(), Some("CAPCUT_CANCELLED"));
    assert!(job.finished_at.is_some());
    assert_eq!(job.ffmpeg_pid, None);
  }

  #[test]
  fn restore_queue_order_is_fifo() {
    let mut queue = VecDeque::new();
    for id in ["job-a", "job-b", "job-c"] {
      queue.push_back(id.to_string());
    }
    assert_eq!(queue.into_iter().collect::<Vec<_>>(), vec!["job-a", "job-b", "job-c"]);
  }

  #[test]
  fn preflight_accepts_auto_before_detection() {
    let mut preset = CapcutAutomationPresetV1::default();
    preset.auto_transcribe = true;
    assert!(validate_start_preflight(&preset).is_ok());
  }

  #[test]
  fn preflight_rejects_auto_without_transcription_detector() {
    let mut preset = CapcutAutomationPresetV1::default();
    preset.auto_transcribe = false;
    assert_eq!(validate_start_preflight(&preset).unwrap_err(), "CAPCUT_SOURCE_LANGUAGE_UNRESOLVED");
  }

  #[test]
  fn preflight_accepts_explicit_traditional_chinese_with_rapidocr_contract() {
    let mut preset = CapcutAutomationPresetV1::quick_localized_vertical_v1();
    preset.source_language = "zt".into();
    preset.localization.source_language = "zt".into();
    preset.ocr_languages.clear();
    preset.ocr_engine = "rapidocr-onnxruntime".into();
    assert!(validate_start_preflight(&preset).is_ok());
  }

  #[test]
  fn fresh_traditional_chinese_request_persists_explicit_source_contract() {
    let mut preset = CapcutAutomationPresetV1::quick_localized_vertical_v1();
    preset.source_language = "zt".into();
    preset.localization.source_language = "zt".into();
    let request = StartCapcutAutomationRequest { input_path: "fixture.webm".into(), output_root: None, page_name: None, job_id: Some("fresh-zt".into()), request_id: Some("fresh-zt".into()), preset: Some(preset) };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["preset"]["sourceLanguage"], "zt");
    assert_eq!(value["preset"]["localization"]["sourceLanguage"], "zt");
  }

  #[test]
  fn preflight_rejects_empty_language_pack_for_tesseract() {
    let mut preset = CapcutAutomationPresetV1::quick_localized_vertical_v1();
    preset.source_language = "en".into();
    preset.localization.source_language = "en".into();
    preset.ocr_engine = "tesseract".into();
    preset.ocr_languages.clear();
    assert_eq!(validate_start_preflight(&preset).unwrap_err(), "CAPCUT_OCR_LANGUAGE_UNRESOLVED");
  }

  #[test]
  fn retry_invalid_immutable_request_does_not_dispatch_or_increment_attempt() {
    let manager = CapcutAutomationJobManager::new();
    let job_id = "retry-invalid".to_string();
    let request = StartCapcutAutomationRequest { input_path: "input.mp4".into(), output_root: None, page_name: Some("Page".into()), job_id: Some(job_id.clone()), request_id: Some("request-retry-invalid".into()), preset: Some(CapcutAutomationPresetV1::default()) };
    let record = Arc::new(JobRecord { snapshot: Mutex::new(snapshot(&job_id, CapcutAutomationJobState::Failed, None)), request, cancel: AtomicBool::new(false) });
    manager.inner.lock().unwrap().jobs.insert(job_id.clone(), record);

    assert_eq!(manager.retry(&job_id).unwrap_err(), "CAPCUT_SOURCE_LANGUAGE_UNRESOLVED");
    let state = manager.get(&job_id).unwrap();
    assert!(matches!(state.state, CapcutAutomationJobState::Failed));
    assert_eq!(state.attempt, 1);
    assert_eq!(state.attempt_id, "retry-invalid-attempt-1");
    assert!(manager.inner.lock().unwrap().queue.is_empty());
  }

  #[test]
  fn attempt_workspace_is_scoped_by_job_and_attempt_id() {
    let root = Path::new(r"C:\Users\test\Artcraft");
    assert_eq!(attempt_workspace(root, "job-1", "job-1-attempt-2"), root.join("video-jobs").join("job-1").join("job-1-attempt-2"));
  }

  #[test]
  fn translation_failure_receipt_is_sanitized_and_structured() {
    let dir = tempfile::tempdir().unwrap();
    let error = r#"CAPCUT_TRANSLATION_FAILED:{"errorCode":"CAPCUT_TRANSLATION_FAILED","hopIndex":1,"operationStage":"DECODE","exitCode":1,"modelId":"argos-en-vi-1-9","segmentCount":3,"sanitizedMessage":"decode failed"}"#;
    write_translation_failure_receipt(dir.path(), error);
    let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.path().join("translation-failure.json")).unwrap()).unwrap();
    assert_eq!(receipt["diagnostics"]["hopIndex"], 1);
    assert_eq!(receipt["diagnostics"]["operationStage"], "DECODE");
    assert_eq!(receipt["diagnostics"]["exitCode"], 1);
    assert!(receipt.to_string().find("decode failed").is_some());
    assert!(receipt.to_string().find("transcript").is_none());
  }
}
