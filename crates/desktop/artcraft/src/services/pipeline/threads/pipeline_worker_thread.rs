//! The pipeline worker: a background loop that drives multi-stage pipeline jobs
//! from `pending` to `complete_success` (or `complete_failure`).

use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::core::state::task_database::TaskDatabase;
use crate::services::pipeline::artifact_store::{workflow_artifact_root, ArtifactStore};
use crate::services::pipeline::caption_segmenter::segment_script_to_captions;
use crate::services::pipeline::capcut::{prepare_capcut, CapcutInput};
use crate::services::pipeline::clients::capcut_mate_client::{add_audio as capcut_add_audio, add_captions as capcut_add_captions, add_video_assets as capcut_add_video_assets, create_draft as capcut_create_draft, gen_video as capcut_gen_video, health_check as capcut_mate_health_check, inspect_draft as capcut_inspect_draft, materialize_rendered_video, poll_gen_video_status as capcut_poll_gen_video_status, publish_draft as capcut_publish_draft, register_artifact_asset, save_draft as capcut_save_draft, verify_draft_exists as capcut_verify_draft_exists, DraftManifest, DEFAULT_HEIGHT, DEFAULT_WIDTH};
use crate::services::pipeline::clients::omniroute_client::{execute_story_script_request, generate_structured_script, list_video_models, StructuredScript};
use crate::services::pipeline::contracts::{is_direct_video_or_media_url, ArtifactKind, ArtifactRef, ContentSource, PipelineContext, StageError, StageId, StageState};
use crate::services::pipeline::events::{emit_job_complete, emit_job_failed, emit_stage_complete, emit_stage_state, JobCompletePayload, JobFailedPayload, StageCompletePayload, StageStatePayload};
use crate::services::pipeline::hardening::{invalidate_output_values, prepare_resume, should_retry_stage, stage_needs_run, stage_policy};
use crate::services::pipeline::ingest_analyze::{ingest_local_source_with_app, ingest_url_source_with_config, ingest_web_story_source, IngestAnalyzeError, IngestAnalyzeResult};
use crate::services::pipeline::media_timeline::{prepare_media_timeline, run_openmontage, should_retry as should_retry_media_timeline, MediaTimelineError, MediaTimelineInput};
use crate::services::pipeline::research::{canonical_metadata, prepare_research, research_state_mut, run_mediacrawler, should_retry as should_retry_research, stage_error as research_stage_error, ResearchError, ResearchInput, ResearchOutcome, ResearchPreparation};
use crate::services::pipeline::story_script::{prepare_story_script, run_story_studio, should_retry as should_retry_story_script, StoryScriptError, StoryScriptInput, StoryStudioOutput};
use crate::services::pipeline::voice::{prepare_voice, should_retry as should_retry_voice, synthesize_voice_with_runtime, VoiceError, VoiceInput, VoiceTiming};
use crate::services::pipeline::grok_image_edit_stage::{execute_grok_image_edit_stage, GrokImageEditInput, GrokImageEditOutput};
use crate::services::pipeline::visual_assets::{build_scene_plan, generate_visual_assets, ScenePlan};
use crate::services::pipeline::state::cancellation_registry::{clear_job, is_cancelled, register_job};
use crate::services::pipeline::state::command_dispatcher::CommandDispatcher;
use enums::tauri::pipeline::pipeline_stage::PipelineStage;
use enums::tauri::tasks::task_status::TaskStatus;
use errors::AnyhowResult;
use log::{error, info, warn};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use sqlite_tasks::queries::content_pages::get_content_page_by_id::{get_content_page_by_id, GetContentPageByIdArgs};
use sqlite_tasks::queries::pipeline::fail_pipeline_job::{fail_pipeline_job, FailPipelineJobArgs};
use sqlite_tasks::queries::pipeline::list_pending_pipeline_jobs::{list_pending_pipeline_jobs, ListPendingPipelineJobsArgs};
use sqlite_tasks::queries::pipeline::pipeline_job::PipelineJob;
use sqlite_tasks::queries::pipeline::update_pipeline_job_stage::{update_pipeline_job_stage, UpdatePipelineJobStageArgs};
use sqlite_tasks::queries::pipeline::update_pipeline_job_status::{update_pipeline_job_status, UpdatePipelineJobStatusArgs};
use crate::services::pipeline::output_policy::OutputPathResolver;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use tokens::tokens::sqlite::pipeline_jobs::PipelineJobId;
use tokio::sync::OnceCell as TokioOnceCell;
use app_lib::services::pipeline::PipelineDownloadConfig;

/// Idle sleep when there is nothing to do.
const IDLE_SLEEP_MS: u64 = 2_000;
/// Sleep after an unexpected error in the outer loop.
const ERROR_SLEEP_MS: u64 = 5_000;
const INGEST_MAX_ATTEMPTS: u32 = 2;

/// Output mode requested by the job (Project Brief).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
  DraftOnly,
  RenderVideo,
}

impl OutputMode {
  fn parse(raw: Option<&str>) -> Self {
    match raw {
      Some("render_video") => Self::RenderVideo,
      _ => Self::DraftOnly,
    }
  }
}

/// Structured pipeline run error mapping each failure to the stage it occurred in.
#[derive(Debug, Clone)]
pub struct PipelineRunError {
  pub stage: PipelineStage,
  pub error_code: String,
  pub error_message: String,
}

impl PipelineRunError {
  pub fn new(stage: PipelineStage, error_code: &str, error_message: String) -> Self {
    Self { stage, error_code: error_code.to_string(), error_message }
  }
}

impl std::fmt::Display for PipelineRunError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[{}] {}: {}", self.stage.to_str(), self.error_code, self.error_message)
  }
}

impl std::error::Error for PipelineRunError {}

/// Extract structured `PipelineRunError` from an `anyhow::Error`.
fn extract_pipeline_error(err: &anyhow::Error) -> PipelineRunError {
  if let Some(run_err) = err.downcast_ref::<PipelineRunError>() {
    run_err.clone()
  } else {
    let err_str = format!("{err:?}");
    let err_code = extract_error_code(&err_str);
    PipelineRunError::new(PipelineStage::PreflightCheck, &err_code, err_str)
  }
}

/// A lazily-initialized shared HTTP client for CapCut Mate calls.
pub(crate) static CAPCUT_CLIENT: TokioOnceCell<reqwest::Client> = TokioOnceCell::const_new();

async fn get_capcut_client() -> AnyhowResult<&'static reqwest::Client> {
  let client = CAPCUT_CLIENT.get_or_init(|| async { reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).build().expect("Failed to build CapCut Mate HTTP client") }).await;
  Ok(client)
}

/// Statuses that mean "the worker should still act on this job".
static PIPELINE_PENDING_STATUSES: Lazy<HashSet<TaskStatus>> = Lazy::new(|| {
  let mut set = HashSet::new();
  set.insert(TaskStatus::Pending);
  // A Started job can only be left behind by process/app termination because
  // this is the single canonical worker. Reclaim it and resume from persisted
  // canonical stage/artifact state on restart.
  set.insert(TaskStatus::Started);
  set
});

pub async fn pipeline_worker_thread(app_handle: AppHandle, app_data_root: AppDataRoot, task_database: TaskDatabase, dispatcher: CommandDispatcher) -> ! {
  info!("[FlowordDB] worker db_path={}", task_database.db_path_display());
  let artifacts_root = app_data_root.pipeline_artifacts_dir();
  info!("[ArtifactStore] runtime_root={}", artifacts_root.display());
  loop {
    let res = worker_loop(&app_handle, &task_database, &dispatcher, &artifacts_root).await;
    if let Err(err) = res {
      error!("[JOB][OUTER_LOOP_ERROR] Pipeline worker loop error: {:?}", err);
    }
    tokio::time::sleep(std::time::Duration::from_millis(ERROR_SLEEP_MS)).await;
  }
}

async fn worker_loop(app_handle: &AppHandle, task_database: &TaskDatabase, dispatcher: &CommandDispatcher, artifacts_root: &std::path::Path) -> AnyhowResult<()> {
  loop {
    let pending = list_pending_pipeline_jobs(ListPendingPipelineJobsArgs { db: task_database.get_connection(), statuses: &PIPELINE_PENDING_STATUSES }).await?;

    if pending.jobs.is_empty() {
      tokio::time::sleep(std::time::Duration::from_millis(IDLE_SLEEP_MS)).await;
      continue;
    }

    for job in pending.jobs {
      let job_id = job.id.clone();

      // Atomic job claim: update status from Pending -> Started
      let claimed = update_pipeline_job_status(UpdatePipelineJobStatusArgs { db: task_database.get_connection(), pipeline_job_id: &job_id, status: TaskStatus::Started }).await?;

      if !claimed {
        warn!("[JOB][CLAIM_SKIP] Job {} was already claimed by another process", job_id.as_str());
        continue;
      }

      // Register a cancellation flag for this run so `cancel_floword_workflow`
      // can abort in-flight adapter work (render polling + between-stage checks).
      let cancel_flag = register_job(job_id.as_str());

      let result = run_job_pipeline(app_handle, task_database, dispatcher, &job, &cancel_flag, artifacts_root).await;

      if let Err(err) = result {
        let run_error = extract_pipeline_error(&err);

        if run_error.error_code == "RESEARCH_AUTH_REQUIRED" {
          info!("[JOB][WAITING_INPUT] Job {} is waiting for interactive Research authentication", job_id.as_str());
          update_pipeline_job_status(UpdatePipelineJobStatusArgs { db: task_database.get_connection(), pipeline_job_id: &job_id, status: TaskStatus::WaitingInput }).await?;
          clear_job(job_id.as_str());
          continue;
        }

        // A cancellation is a terminal state, not a failure.
        if run_error.error_code == "RENDER_CANCELLED" || is_cancelled(job_id.as_str()) {
          info!("[JOB][CANCELLED] Job {} cancelled by user", job_id.as_str());
          let _ = update_pipeline_job_status(UpdatePipelineJobStatusArgs { db: task_database.get_connection(), pipeline_job_id: &job_id, status: TaskStatus::CancelledByUser }).await;
          clear_job(job_id.as_str());
          continue;
        }

        let err_str = run_error.error_message.clone();
        error!("[JOB][FAILED] Job {} failed at {}: {} (code={})", job_id.as_str(), run_error.stage.to_str(), err_str, run_error.error_code);

        fail_pipeline_job(FailPipelineJobArgs { db: task_database.get_connection(), pipeline_job_id: &job_id, failure_message: &err_str }).await?;

        emit_job_failed(app_handle, JobFailedPayload { job_id: job_id.as_str().to_string(), failed_stage: run_error.stage.to_str().to_string(), error_code: run_error.error_code, error_message: err_str });
      }

      clear_job(job_id.as_str());
    }
  }
}

/// Helper to parse standard error codes.
fn extract_error_code(err_str: &str) -> String {
  for code in &["LLM_UNAVAILABLE", "LLM_TIMEOUT", "LLM_UNAUTHORIZED", "LLM_RATE_LIMITED", "LLM_INVALID_RESPONSE", "LLM_EMPTY_SCRIPT", "CAPCUT_DRAFT_CREATE_FAILED", "CAPCUT_DRAFT_STAGE_FAILED", "CAPCUT_DESKTOP_ROOT_NOT_FOUND", "CAPCUT_DRAFT_PUBLISH_FAILED", "CAPCUT_DRAFT_VERIFY_FAILED", "CAPCUT_MEDIA_REFERENCE_INVALID", "CAPCUT_DRAFT_NOT_FOUND", "CAPCUT_UNAVAILABLE", "CAPCUT_ASSET_TRANSPORT_FAILED", "CAPCUT_TIMEOUT", "DRAFT_CREATE_FAILED", "VIDEO_ADD_FAILED", "AUDIO_ADD_FAILED", "CAPTION_ADD_FAILED", "DRAFT_SAVE_FAILED", "DRAFT_INSPECT_FAILED", "RENDER_UNSUPPORTED", "RENDER_FAILED", "RENDER_TIMEOUT", "RENDER_CANCELLED", "PREFLIGHT_FAILED"] {
    if err_str.contains(code) {
      return code.to_string();
    }
  }
  "PIPELINE_ERROR".to_string()
}

/// Bail out with a cancellation error if the user requested it.
fn check_cancelled(job_id: &str, stage: PipelineStage) -> AnyhowResult<()> {
  if is_cancelled(job_id) {
    return Err(PipelineRunError::new(stage, "RENDER_CANCELLED", "User requested job cancellation".to_string()).into());
  }
  Ok(())
}

/// Execute full job pipeline through state machine stages.
async fn run_job_pipeline(app_handle: &AppHandle, task_database: &TaskDatabase, dispatcher: &CommandDispatcher, job: &PipelineJob, cancel_flag: &Arc<AtomicBool>, artifacts_root: &std::path::Path) -> AnyhowResult<()> {
  let job_id_str = job.id.as_str().to_string();
  let input = parse_input(job);
  let prompt = input.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
  let local_file = first_input_string(&input, "local_file", "source_files");
  let source_url = first_input_string(&input, "source_url", "source_urls");
  let story_url = first_input_string(&input, "story_url", "story_urls");
  let workflow_mode = input.get("workflow_mode").and_then(Value::as_str).unwrap_or("source_based");
  let original_creation = matches!(workflow_mode, "original" | "original_creation");
  let research_enabled = input.get("research_enabled").and_then(Value::as_bool).unwrap_or(false);
  let research_query = input.get("research_query").and_then(Value::as_str);
  let declared_source = input.get("content_source").and_then(Value::as_str);

  let content_source = ContentSource::resolve(
    declared_source,
    local_file.as_deref(),
    source_url.as_deref(),
    story_url.as_deref(),
    research_enabled,
    research_query,
  );

  let has_source_input = local_file.is_some() || source_url.is_some() || story_url.is_some();
  validate_workflow_input(workflow_mode, content_source, &prompt, has_source_input, research_enabled, research_query)?;

  let output_mode = OutputMode::parse(input.get("output_mode").and_then(|v| v.as_str()));
  let allow_draft_fallback = input.get("allow_draft_fallback").and_then(|v| v.as_bool()).unwrap_or(false);
  let model_id = input.get("model_id").and_then(|v| v.as_str()).map(|s| s.to_string());
  let target_duration_seconds = input.get("target_duration_seconds").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
  let language = input.get("language").and_then(|v| v.as_str()).unwrap_or("vi").to_string();
  let youwee_download_config = PipelineDownloadConfig {
    cookie_mode: input.get("cookie_mode").and_then(Value::as_str).map(str::to_string),
    cookie_browser: input.get("cookie_browser").and_then(Value::as_str).map(str::to_string),
    cookie_browser_profile: input.get("cookie_browser_profile").and_then(Value::as_str).map(str::to_string),
    cookie_file_path: input.get("cookie_file_path").and_then(Value::as_str).map(str::to_string),
    cookie_skip_patterns: input.get("cookie_skip_patterns").and_then(Value::as_array).map(|values| values.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default(),
    proxy_url: None,
  };
  let mut outputs = parse_stage_outputs(job);
  let persisted_context = outputs.get("pipeline_context").cloned().and_then(|value| serde_json::from_value::<PipelineContext>(value).ok()).filter(|saved| saved.job_id == job_id_str);
  let resume_from = match persisted_context.as_ref() {
    Some(saved) => {
      let mut probe = saved.clone();
      prepare_resume(&mut probe).map_err(contract_pipeline_run_error)?
    },
    None => None,
  };

  let work_dir = workflow_artifact_root(artifacts_root, job.id.as_str());
  let mut phase3_script: Option<StructuredScript> = None;
  let mut phase6_context: Option<PipelineContext> = None;

  let mut context = outputs
    .get("pipeline_context")
    .cloned()
    .and_then(|value| serde_json::from_value::<PipelineContext>(value).ok())
    .filter(|saved| saved.job_id == job_id_str)
    .unwrap_or(pipeline_context_from_input(&job_id_str, &input, &prompt, model_id.clone(), &language, target_duration_seconds, output_mode, local_file.clone(), source_url.clone(), story_url.clone(), Some(content_source.as_str().to_string()))?);

  // Production Grok Image Edit Workflow Call Site (Fail-Closed)
  let is_grok_image_edit = workflow_mode == "grok_image_edit"
    || workflow_mode == "image_edit"
    || input.get("method").and_then(Value::as_str) == Some("grok.image.edit")
    || input.get("service").and_then(Value::as_str) == Some("grok");

  if is_grok_image_edit {
    check_cancelled(&job_id_str, PipelineStage::ScriptGenerating)?;

    // 1. Mandatory Page validation (fail-closed)
    let page_id = input.get("page_id")
      .or_else(|| input.get("page"))
      .and_then(Value::as_str)
      .ok_or_else(|| PipelineRunError::new(PipelineStage::ScriptGenerating, "PAGE_REQUIRED", "Grok image edit workflow requires page_id".to_string()))?;

    // 2. Mandatory Prompt validation (fail-closed)
    if prompt.trim().is_empty() {
      return Err(PipelineRunError::new(PipelineStage::ScriptGenerating, "IMAGE_PROMPT_REQUIRED", "Grok image edit workflow requires a non-empty image prompt".to_string()).into());
    }

    // 3. Mandatory Source Image Artifact validation (fail-closed)
    let source_image_artifact = if let Some(art_val) = input.get("source_image_artifact").or_else(|| input.get("source_artifact")) {
      serde_json::from_value::<ArtifactRef>(art_val.clone())
        .map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "SOURCE_IMAGE_REQUIRED", format!("Invalid source image artifact: {e}")))?
    } else if let Some(local_path) = local_file.as_deref() {
      let path = std::path::Path::new(local_path);
      if !path.exists() {
        return Err(PipelineRunError::new(PipelineStage::ScriptGenerating, "SOURCE_IMAGE_REQUIRED", format!("Source image file does not exist at {local_path}")).into());
      }
      let (detected_mime, _ext) = crate::services::pipeline::grok_image_edit_stage::detect_image_mime(&std::fs::read(path).map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "SOURCE_IMAGE_REQUIRED", e.to_string()))?)
        .map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "SOURCE_IMAGE_REQUIRED", e))?;
      let stored = ArtifactStore::register_typed_artifact(
        &work_dir,
        &job_id_str,
        StageId::StoryScript,
        "input",
        ArtifactKind::GeneratedImage,
        path,
        serde_json::json!({ "mime_type": detected_mime }),
      ).map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "SOURCE_IMAGE_REQUIRED", e.to_string()))?;
      stored.to_artifact_ref(StageId::StoryScript)
        .map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "SOURCE_IMAGE_REQUIRED", e.to_string()))?
    } else if let Some(first_art) = context.artifact_refs.iter().find(|a| a.kind == ArtifactKind::GeneratedImage || a.kind == ArtifactKind::Story || a.kind == ArtifactKind::SourceVideo) {
      first_art.clone()
    } else {
      return Err(PipelineRunError::new(PipelineStage::ScriptGenerating, "SOURCE_IMAGE_REQUIRED", "Grok image edit workflow requires a source image artifact".to_string()).into());
    };

    emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::PreflightCheck, PipelineStage::ScriptGenerating, 50, "Editing image via Grok worker").await?;

    let attempt_id = "1";
    let edit_output = run_grok_image_edit_stage(
      &job_id_str,
      page_id,
      source_image_artifact,
      &prompt,
      &work_dir,
      attempt_id,
    ).await?;

    // Push GeneratedImage to context and persist outputs as IMAGE_DONE
    context.artifact_refs.push(edit_output.generated_artifact.clone());
    outputs["image_edit"] = serde_json::to_value(&edit_output).map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "IMAGE_EDIT_SERIALIZATION_FAILED", e.to_string()))?;
    outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "IMAGE_EDIT_SERIALIZATION_FAILED", e.to_string()))?;
    outputs["status"] = json!("IMAGE_DONE");
    persist_outputs(task_database, &job.id, PipelineStage::ScriptReady, &serialize_outputs(&outputs)?).await?;
    emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::ScriptGenerating, PipelineStage::ScriptReady, 100, "Image edited successfully via Grok").await?;
    return Ok(());
  }

  if let Some(resume_from) = prepare_resume(&mut context).map_err(contract_pipeline_run_error)? {
    invalidate_output_values(&mut outputs, resume_from);
    info!("[JOB][RESUME] Resuming canonical pipeline at {resume_from}; valid upstream artifacts will be reused");
  }

  if original_creation && !context.artifact_refs.iter().any(|artifact| artifact.kind == ArtifactKind::GeneratedVideo) {
    let connected_video_models = list_video_models().await.map_err(|error| {
      PipelineRunError::new(
        PipelineStage::PreflightCheck,
        "VISUAL_PROVIDER_UNAVAILABLE",
        format!("Cannot verify OmniRoute video-provider connections: {error}"),
      )
    })?;
    if connected_video_models.is_empty() {
      return Err(PipelineRunError::new(
        PipelineStage::PreflightCheck,
        "VISUAL_PROVIDER_UNAVAILABLE",
        "Original Creation needs at least one active, healthy video-provider connection. Gemini/OpenCode keys cover text only; add and test Veo, Runway, Seedance, Sora, or another video provider in OmniRoute, or use Source Based/Remix with local media.".to_string(),
      )
      .into());
    }
    info!("OMNIROUTE_VIDEO_PREFLIGHT connected_models={}", connected_video_models.len());
  }

  // Phase 1: Ingest / Source Content Acquisition
  let skips_ingest = matches!(content_source, ContentSource::PromptOnly | ContentSource::TrendResearch) || (original_creation && !has_source_input);
  if skips_ingest && stage_needs_run(&context, StageId::IngestAnalyze) {
    let state = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::IngestAnalyze).expect("ingest stage contract");
    state.skip_stage(chrono::Utc::now().to_rfc3339()).map_err(contract_pipeline_run_error)?;
    outputs["ingest_analyze"] = json!({ "artifact_ids": [], "stage": state.clone() });
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: state.clone(), progress: Some(14), message: Some("Source ingest not required for this content source".to_string()) });
  } else if stage_needs_run(&context, StageId::IngestAnalyze) {
    emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::PreflightCheck, PipelineStage::IngestAnalyze, 14, "Acquiring and analyzing source content").await?;
    let (ingest_result, ingest_state) = match run_ingest_analyze_stage(app_handle, local_file.as_deref(), source_url.as_deref(), story_url.as_deref(), &work_dir, &job_id_str, cancel_flag, &youwee_download_config).await {
      Ok(success) => success,
      Err((run_error, failed_state)) => {
        if let Some(state) = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::IngestAnalyze) {
          *state = failed_state;
        }
        outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::IngestAnalyze, "INGEST_SERIALIZATION_FAILED", error.to_string()))?;
        outputs["ingest_analyze"] = json!({ "artifact_ids": [], "stage": context.stage_states.iter().find(|state| state.stage_id == StageId::IngestAnalyze) });
        persist_outputs(task_database, &job.id, PipelineStage::IngestAnalyze, &serialize_outputs(&outputs)?).await?;
        return Err(run_error.into());
      },
    };
    let output_artifact_ids = ingest_result.artifact_refs.iter().map(|artifact| artifact.artifact_id.clone()).collect::<Vec<_>>();
    context.artifact_refs.extend(ingest_result.artifact_refs);
    if let Some(state) = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::IngestAnalyze) {
      *state = ingest_state;
    }
    outputs["ingest_analyze"] = json!({ "artifact_ids": output_artifact_ids, "stage": context.stage_states.iter().find(|state| state.stage_id == StageId::IngestAnalyze) });
  } else if let Some(state) = context.stage_states.iter().find(|state| state.stage_id == StageId::IngestAnalyze) {
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: state.clone(), progress: Some(25), message: Some("Reused validated ingest/analyze artifacts".to_string()) });
  }
  outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::IngestAnalyze, "INGEST_SERIALIZATION_FAILED", error.to_string()))?;
  persist_outputs(task_database, &job.id, PipelineStage::IngestAnalyze, &serialize_outputs(&outputs)?).await?;

  // Phase 2: MediaCrawler Research
  if stage_needs_run(&context, StageId::Research) {
    match prepare_research(&mut context, &chrono::Utc::now().to_rfc3339()).map_err(research_contract_pipeline_run_error)? {
      ResearchPreparation::Skipped => {
        let state = context.stage_states.iter().find(|state| state.stage_id == StageId::Research).cloned().expect("research stage contract");
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: state.clone(), progress: Some(27), message: Some("Research disabled; MediaCrawler not called".to_string()) });
        outputs["research"] = json!({ "artifact_ids": [], "stage": state });
      },
      ResearchPreparation::Ready(research_input) => {
        emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::PreflightCheck, PipelineStage::Research, 27, "Running MediaCrawler research").await?;
        match run_research_stage(app_handle, &research_input, &work_dir, &job_id_str, cancel_flag).await {
          Ok((payload, artifact_ref, stage)) => {
            context.artifact_refs.push(artifact_ref.clone());
            *research_state_mut(&mut context).map_err(research_contract_pipeline_run_error)? = stage.clone();
            outputs["research"] = json!({ "artifact_ids": [artifact_ref.artifact_id], "artifact": payload, "stage": stage });
          },
          Err((run_error, failed_state)) => {
            *research_state_mut(&mut context).map_err(research_contract_pipeline_run_error)? = failed_state.clone();
            outputs["research"] = json!({ "artifact_ids": [], "stage": failed_state });
            outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::Research, "RESEARCH_SERIALIZATION_FAILED", error.to_string()))?;
            persist_outputs(task_database, &job.id, PipelineStage::Research, &serialize_outputs(&outputs)?).await?;
            return Err(run_error.into());
          },
        }
      },
    }
  } else if let Some(state) = context.stage_states.iter().find(|state| state.stage_id == StageId::Research) {
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: state.clone(), progress: Some(29), message: Some("Reused canonical research state".to_string()) });
  }
  outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::Research, "RESEARCH_SERIALIZATION_FAILED", error.to_string()))?;
  persist_outputs(task_database, &job.id, PipelineStage::Research, &serialize_outputs(&outputs)?).await?;

    if stage_needs_run(&context, StageId::StoryScript) {
      let story_input = prepare_story_script(&context).map_err(|error| PipelineRunError::new(PipelineStage::ScriptGenerating, "STORY_SCRIPT_DEPENDENCY_INVALID", error.to_string()))?;
      match run_story_script_stage(app_handle, &story_input, model_id.as_deref(), &work_dir, &job_id_str, cancel_flag).await {
        Ok(result) => {
          let artifact_ids = result.artifact_refs.iter().map(|artifact| artifact.artifact_id.clone()).collect::<Vec<_>>();
          context.artifact_refs.extend(result.artifact_refs);
          if let Some(state) = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::StoryScript) {
            *state = result.stage.clone();
          }
          outputs["story"] = result.story;
          outputs["script_request"] = result.script_request;
          outputs["script"] = serde_json::to_value(&result.script).unwrap_or(Value::Null);
          outputs["story_script"] = json!({ "artifact_ids": artifact_ids, "stage": result.stage });
          phase3_script = Some(result.script);
        },
        Err((run_error, failed_state)) => {
          if let Some(state) = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::StoryScript) {
            *state = failed_state.clone();
          }
          outputs["story_script"] = json!({ "artifact_ids": [], "stage": failed_state });
          outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::ScriptGenerating, "STORY_SCRIPT_SERIALIZATION_FAILED", error.to_string()))?;
          persist_outputs(task_database, &job.id, PipelineStage::ScriptGenerating, &serialize_outputs(&outputs)?).await?;
          return Err(run_error.into());
        },
      }
    } else {
      phase3_script = Some(load_structured_script(&context).map_err(|error| PipelineRunError::new(PipelineStage::ScriptGenerating, "STORY_ARTIFACT_INVALID", error.to_string()))?);
      if let Some(state) = context.stage_states.iter().find(|state| state.stage_id == StageId::StoryScript) {
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: state.clone(), progress: Some(35), message: Some("Reused validated story/script artifacts".to_string()) });
      }
    }
    outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::ScriptGenerating, "STORY_SCRIPT_SERIALIZATION_FAILED", error.to_string()))?;
    persist_outputs(task_database, &job.id, PipelineStage::ScriptReady, &serialize_outputs(&outputs)?).await?;

    // Original Creation resolves a real visual provider immediately after the
    // scene plan, before spending time on voice or contacting OpenMontage.
    if original_creation && !context.artifact_refs.iter().any(|artifact| artifact.kind == ArtifactKind::GeneratedVideo) {
      let plan_ref = context.artifact_refs.iter().find(|artifact| artifact.kind == ArtifactKind::ScenePlan).ok_or_else(|| PipelineRunError::new(PipelineStage::MediaTimeline, "SCENE_PLAN_INVALID", "Scene plan artifact is missing".to_string()))?;
      let plan: ScenePlan = serde_json::from_slice(&std::fs::read(&plan_ref.location).map_err(|error| PipelineRunError::new(PipelineStage::MediaTimeline, "SCENE_PLAN_INVALID", error.to_string()))?).map_err(|error| PipelineRunError::new(PipelineStage::MediaTimeline, "SCENE_PLAN_INVALID", error.to_string()))?;
      let aspect_ratio = input.get("aspect_ratio").and_then(Value::as_str).unwrap_or("9:16");
      emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::ScriptReady, PipelineStage::MediaTimeline, 37, "Generating real visual assets for Original Creation").await?;
      match generate_visual_assets(app_handle, &work_dir, &job_id_str, &plan, aspect_ratio, Arc::clone(cancel_flag)).await {
        Ok(visuals) => {
          let artifact_ids = visuals.iter().map(|artifact| artifact.artifact_id.clone()).collect::<Vec<_>>();
          context.artifact_refs.extend(visuals);
          if let Some(state) = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::StoryScript) {
            let new_ids = artifact_ids.iter().filter(|id| !state.output_artifact_ids.contains(id)).cloned().collect::<Vec<_>>();
            state.output_artifact_ids.extend(new_ids);
          }
          outputs["media_assets"] = json!({ "artifact_ids": artifact_ids, "service": "omniroute", "asset_type": "video" });
          outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::MediaTimeline, "MEDIA_TIMELINE_SERIALIZATION_FAILED", error.to_string()))?;
          persist_outputs(task_database, &job.id, PipelineStage::ScriptReady, &serialize_outputs(&outputs)?).await?;
        },
        Err(error) => {
          let state = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::MediaTimeline).expect("media timeline stage contract");
          state.start_stage(Some("omniroute".to_string()), chrono::Utc::now().to_rfc3339()).map_err(contract_pipeline_run_error)?;
          state.fail_stage(StageError::sanitized(error.code, &error.message, false), chrono::Utc::now().to_rfc3339()).map_err(contract_pipeline_run_error)?;
          outputs["media_assets"] = json!({ "artifact_ids": [], "stage": state.clone() });
          outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|serialize_error| PipelineRunError::new(PipelineStage::MediaTimeline, "MEDIA_TIMELINE_SERIALIZATION_FAILED", serialize_error.to_string()))?;
          persist_outputs(task_database, &job.id, PipelineStage::MediaTimeline, &serialize_outputs(&outputs)?).await?;
          return Err(PipelineRunError::new(PipelineStage::MediaTimeline, error.code, error.message).into());
        },
      }
    }

    // Phase 4 business stage: the script artifact ID is the only handoff.
    // TTS executes through OmniRoute; audio and timings are validated from the
    // real files/probes before either artifact is exposed downstream.
    if stage_needs_run(&context, StageId::Voice) {
      let voice_input = prepare_voice(&context).map_err(|error| PipelineRunError::new(PipelineStage::ScriptGenerating, "VOICE_DEPENDENCY_INVALID", error.to_string()))?;
      match run_voice_stage(app_handle, &voice_input, &work_dir, &job_id_str, cancel_flag).await {
        Ok(result) => {
          let artifact_ids = result.artifact_refs.iter().map(|artifact| artifact.artifact_id.clone()).collect::<Vec<_>>();
          context.artifact_refs.extend(result.artifact_refs);
          if let Some(state) = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Voice) {
            *state = result.stage.clone();
          }
          outputs["voice"] = json!({ "artifact_ids": artifact_ids, "timing": result.timing, "stage": result.stage });
        },
        Err((run_error, failed_state)) => {
          if let Some(state) = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Voice) {
            *state = failed_state.clone();
          }
          outputs["voice"] = json!({ "artifact_ids": [], "stage": failed_state });
          outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::ScriptGenerating, "VOICE_SERIALIZATION_FAILED", error.to_string()))?;
          persist_outputs(task_database, &job.id, PipelineStage::ScriptReady, &serialize_outputs(&outputs)?).await?;
          return Err(run_error.into());
        },
      }
    } else if let Some(state) = context.stage_states.iter().find(|state| state.stage_id == StageId::Voice) {
      emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: state.clone(), progress: Some(40), message: Some("Reused validated voice artifacts".to_string()) });
    }
    outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::ScriptGenerating, "VOICE_SERIALIZATION_FAILED", error.to_string()))?;
    persist_outputs(task_database, &job.id, PipelineStage::ScriptReady, &serialize_outputs(&outputs)?).await?;

    // Phase 5 business stage: OpenMontage receives only canonical artifact-ID
    // handoffs and produces the minimum video/voice/caption timeline. CapCut
    // remains a separate downstream stage and is intentionally untouched here.
    if stage_needs_run(&context, StageId::MediaTimeline) {
      log_media_timeline_inputs(&context, &job_id_str);
      let media_timeline_input = prepare_media_timeline(&context, &work_dir).map_err(|error| PipelineRunError::new(PipelineStage::MediaTimeline, "MEDIA_TIMELINE_INPUT_MISSING", error.to_string()))?;
      persist_outputs(task_database, &job.id, PipelineStage::MediaTimeline, &serialize_outputs(&outputs)?).await?;
      match run_media_timeline_stage(app_handle, &media_timeline_input, &work_dir, &job_id_str, cancel_flag).await {
        Ok(result) => {
          let artifact_ids = result.artifact_refs.iter().map(|artifact| artifact.artifact_id.clone()).collect::<Vec<_>>();
          context.artifact_refs.extend(result.artifact_refs);
          if let Some(state) = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::MediaTimeline) {
            *state = result.stage.clone();
          }
          outputs["media_timeline"] = json!({ "artifact_ids": artifact_ids, "timeline": result.timeline, "captions": result.captions, "stage": result.stage });
        },
        Err((run_error, failed_state)) => {
          if let Some(state) = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::MediaTimeline) {
            *state = failed_state.clone();
          }
          outputs["media_timeline"] = json!({ "artifact_ids": [], "stage": failed_state });
          outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::MediaTimeline, "MEDIA_TIMELINE_SERIALIZATION_FAILED", error.to_string()))?;
          persist_outputs(task_database, &job.id, PipelineStage::MediaTimeline, &serialize_outputs(&outputs)?).await?;
          return Err(run_error.into());
        },
      }
    } else if let Some(state) = context.stage_states.iter().find(|state| state.stage_id == StageId::MediaTimeline) {
      emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: state.clone(), progress: Some(43), message: Some("Reused validated timeline/caption artifacts".to_string()) });
    }
    outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| PipelineRunError::new(PipelineStage::MediaTimeline, "MEDIA_TIMELINE_SERIALIZATION_FAILED", error.to_string()))?;
    persist_outputs(task_database, &job.id, PipelineStage::MediaTimeline, &serialize_outputs(&outputs)?).await?;
    phase6_context = Some(context);

  if output_mode == OutputMode::DraftOnly && phase6_context.as_ref().is_some_and(|context| !stage_needs_run(context, StageId::Capcut)) {
    let context = phase6_context.as_ref().expect("phase6 context checked");
    let draft_url = reusable_draft_url(context).map_err(|error| PipelineRunError::new(PipelineStage::DraftSaving, "DRAFT_INSPECT_FAILED", error.to_string()))?;
    outputs["pipeline_context"] = serde_json::to_value(context)?;
    outputs["draft_url"] = json!(draft_url);
    outputs["video_url"] = Value::Null;
    outputs["rendering_supported"] = json!(false);
    finalize_draft_ready(app_handle, task_database, &job.id, &draft_url, &serialize_outputs(&outputs)?).await?;
    return Ok(());
  }

  // 2. Stage: ScriptGenerating (30%)
  check_cancelled(&job_id_str, PipelineStage::ScriptGenerating)?;
  emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::PreflightCheck, PipelineStage::ScriptGenerating, 30, "Story script ready").await?;

  let script: StructuredScript = if let Some(script) = phase3_script {
    script
  } else {
    let _cpu_permit = dispatcher.acquire_cpu().await;
    info!("[JOB][LLM] Generating structured script (model={:?})...", model_id);
    generate_structured_script(&prompt, model_id.as_deref(), target_duration_seconds, &language).await.map_err(|e| {
      let err_str = format!("{e:?}");
      let code = extract_error_code(&err_str);
      PipelineRunError::new(PipelineStage::ScriptGenerating, &code, err_str)
    })?
  };

  if outputs.get("story_script").is_none() {
    let script_dir = work_dir.join("script");
    std::fs::create_dir_all(&script_dir).map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "PIPELINE_ERROR", format!("Failed to create script dir: {e}")))?;
    let script_file_path = script_dir.join("script.json");
    let script_json = serde_json::to_string_pretty(&script).map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "PIPELINE_ERROR", format!("Failed to serialize script: {e}")))?;
    std::fs::write(&script_file_path, &script_json).map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "PIPELINE_ERROR", format!("Failed to write script file: {e}")))?;
    let script_artifact = ArtifactStore::register_artifact(&work_dir, job.id.as_str(), "step-2-omniroute", "OmniRouteAdapter", "script", &script_file_path, json!({ "prompt": prompt, "model": model_id })).map_err(|e| PipelineRunError::new(PipelineStage::ScriptGenerating, "PIPELINE_ERROR", format!("{e:?}")))?;
    outputs["script"] = serde_json::to_value(&script).unwrap_or(Value::Null);
    outputs["script_artifact"] = json!(script_artifact);
    persist_outputs(task_database, &job.id, PipelineStage::ScriptReady, &serialize_outputs(&outputs)?).await?;
  } else {
    persist_outputs(task_database, &job.id, PipelineStage::ScriptReady, &serialize_outputs(&outputs)?).await?;
  }

  // Build narration text used for caption segmentation.
  let narration = script.scenes.iter().map(|s| s.narration.as_str()).collect::<Vec<_>>().join(" ");

  let capcut_input: Option<CapcutInput> = phase6_context.as_ref().map(prepare_capcut).transpose().map_err(|error| PipelineRunError::new(PipelineStage::DraftCreating, "CAPCUT_DEPENDENCY_INVALID", error.to_string()))?;
  let client = get_capcut_client().await?;
  let captions = capcut_input.as_ref().map(|input| input.captions.clone()).unwrap_or_else(|| segment_script_to_captions(&narration));
  let mut capcut_state = phase6_context.as_ref().and_then(|context| context.stage_states.iter().find(|state| state.stage_id == StageId::Capcut)).cloned().unwrap_or_else(|| StageState::pending(StageId::Capcut));
  if let Some(input) = capcut_input.as_ref() {
    capcut_state.input_artifact_ids = input.input_artifact_ids.clone();
  }
  let (_draft_url, draft_id, saved_url, manifest, published) = loop {
    capcut_state.start_stage(Some("capcut_mate".to_string()), chrono::Utc::now().to_rfc3339()).map_err(contract_pipeline_error)?;
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: capcut_state.clone(), progress: Some(45), message: Some("Creating verified CapCut draft from timeline artifacts".to_string()) });
    if let Some(context) = phase6_context.as_mut() {
      *context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Capcut).expect("capcut stage contract") = capcut_state.clone();
      outputs["pipeline_context"] = serde_json::to_value(&*context)?;
      outputs["capcut"] = json!({ "artifact_ids": [], "stage": capcut_state.clone() });
      persist_outputs(task_database, &job.id, PipelineStage::DraftCreating, &serialize_outputs(&outputs)?).await?;
    }

    let attempt = tokio::time::timeout(std::time::Duration::from_secs(stage_policy(StageId::Capcut).timeout_seconds), async {
      check_cancelled(&job_id_str, PipelineStage::DraftCreating).map_err(|error| extract_pipeline_error(&error))?;
      emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::ScriptReady, PipelineStage::DraftCreating, 45, "Creating CapCut draft").await.map_err(|error| extract_pipeline_error(&error))?;
      info!("[JOB][CAPCUT] Creating draft project...");
      let created = capcut_create_draft(client, DEFAULT_WIDTH, DEFAULT_HEIGHT).await.map_err(|error| map_capcut_error(PipelineStage::DraftCreating, &error))?;
      let draft_url = created.draft_url.clone();
      let draft_id = created.draft_id.clone();
      outputs["draft_url"] = json!(draft_url);
      outputs["draft_id"] = json!(draft_id);
      persist_outputs(task_database, &job.id, PipelineStage::DraftCreated, &serialize_outputs(&outputs).map_err(|error| extract_pipeline_error(&error))?).await.map_err(|error| extract_pipeline_error(&error))?;

      if let Some(input) = capcut_input.as_ref() {
        check_cancelled(&job_id_str, PipelineStage::DraftCreating).map_err(|error| extract_pipeline_error(&error))?;
        let mut visual_urls = std::collections::HashMap::new();
        for segment in &input.video_segments {
          if !visual_urls.contains_key(&segment.artifact_id) {
            let url = register_artifact_asset(client, &segment.artifact_id, std::path::Path::new(&segment.path)).await.map_err(|error| map_capcut_error(PipelineStage::DraftCreating, &error))?;
            visual_urls.insert(segment.artifact_id.clone(), url);
          }
        }
        let voice_url = register_artifact_asset(client, &input.voice_audio_artifact_id, std::path::Path::new(&input.voice_segment.path)).await.map_err(|error| map_capcut_error(PipelineStage::DraftCreating, &error))?;
        capcut_add_video_assets(client, &draft_url, &visual_urls, &input.video_segments).await.map_err(|error| map_capcut_error(PipelineStage::DraftCreating, &error))?;
        capcut_add_audio(client, &draft_url, &voice_url, &input.voice_segment).await.map_err(|error| map_capcut_error(PipelineStage::DraftCreating, &error))?;
      }

      check_cancelled(&job_id_str, PipelineStage::CaptionAdding).map_err(|error| extract_pipeline_error(&error))?;
      emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::DraftCreated, PipelineStage::CaptionAdding, 60, "Adding captions to draft").await.map_err(|error| extract_pipeline_error(&error))?;
      capcut_add_captions(client, &draft_url, &captions).await.map_err(|error| map_capcut_error(PipelineStage::CaptionAdding, &error))?;

      check_cancelled(&job_id_str, PipelineStage::DraftSaving).map_err(|error| extract_pipeline_error(&error))?;
      emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::CaptionAdding, PipelineStage::DraftSaving, 75, "Saving draft project").await.map_err(|error| extract_pipeline_error(&error))?;
      let saved_url = capcut_save_draft(client, &draft_url).await.map_err(|error| map_capcut_error(PipelineStage::DraftSaving, &error))?;
      capcut_verify_draft_exists(client, &draft_id).await.map_err(|error| map_capcut_error(PipelineStage::DraftSaving, &error))?;
      let manifest = capcut_inspect_draft(client, &draft_id, &created.draft_path).await.map_err(|error| map_capcut_error(PipelineStage::DraftSaving, &error))?;
      if capcut_input.is_some() && (manifest.visual_track_count.unwrap_or(0) == 0 || manifest.audio_track_count.unwrap_or(0) == 0 || manifest.caption_track_count.unwrap_or(0) == 0) {
        return Err(PipelineRunError::new(PipelineStage::DraftSaving, "DRAFT_INSPECT_FAILED", "Verified draft is missing video, voice, or caption tracks".to_string()));
      }
      let published = capcut_publish_draft(client, &created).await.map_err(|error| map_capcut_error(PipelineStage::DraftSaving, &error))?;
      Ok((draft_url, draft_id, saved_url, manifest, published))
    })
    .await;

    let failure = match attempt {
      Ok(Ok(success)) => break success,
      Ok(Err(error)) => error,
      Err(_) => PipelineRunError::new(PipelineStage::DraftSaving, "CAPCUT_TIMEOUT", "CapCut stage exceeded its production timeout".to_string()),
    };
    if cancel_flag.load(Ordering::SeqCst) || failure.error_code == "RENDER_CANCELLED" {
      let _ = capcut_state.cancel_stage(chrono::Utc::now().to_rfc3339());
      emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: capcut_state.clone(), progress: Some(45), message: Some("CapCut stage cancelled".to_string()) });
      if let Some(context) = phase6_context.as_mut() {
        *context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Capcut).expect("capcut stage contract") = capcut_state.clone();
        outputs["pipeline_context"] = serde_json::to_value(&*context)?;
        outputs["capcut"] = json!({ "artifact_ids": [], "stage": capcut_state.clone() });
        persist_outputs(task_database, &job.id, PipelineStage::DraftCreating, &serialize_outputs(&outputs)?).await?;
      }
      return Err(failure.into());
    }
    let retryable = should_retry_stage(StageId::Capcut, &failure.error_code, capcut_state.attempt, false);
    capcut_state.fail_stage(StageError::sanitized(failure.error_code.clone(), &failure.error_message, retryable), chrono::Utc::now().to_rfc3339()).map_err(contract_pipeline_error)?;
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: capcut_state.clone(), progress: Some(45), message: capcut_state.error.as_ref().map(|error| error.message.clone()) });
    if retryable {
      capcut_state.retry_stage().map_err(contract_pipeline_error)?;
      invalidate_output_values(&mut outputs, StageId::Capcut);
      emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: capcut_state.clone(), progress: Some(45), message: Some("Retrying CapCut after transient failure".to_string()) });
      if let Some(context) = phase6_context.as_mut() {
        *context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Capcut).expect("capcut stage contract") = capcut_state.clone();
        outputs["pipeline_context"] = serde_json::to_value(&*context)?;
        outputs["capcut"] = json!({ "artifact_ids": [], "stage": capcut_state.clone() });
        persist_outputs(task_database, &job.id, PipelineStage::DraftCreating, &serialize_outputs(&outputs)?).await?;
      }
      continue;
    }
    if let Some(context) = phase6_context.as_mut() {
      *context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Capcut).expect("capcut stage contract") = capcut_state.clone();
      outputs["pipeline_context"] = serde_json::to_value(&*context)?;
      outputs["capcut"] = json!({ "artifact_ids": [], "stage": capcut_state.clone() });
      persist_outputs(task_database, &job.id, PipelineStage::DraftCreating, &serialize_outputs(&outputs)?).await?;
    }
    return Err(failure.into());
  };
  if let Some(context) = phase6_context.as_mut() {
    *context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Capcut).expect("capcut stage contract") = capcut_state;
  }

  let capcut_dir = work_dir.join("capcut");
  std::fs::create_dir_all(&capcut_dir).map_err(|e| PipelineRunError::new(PipelineStage::DraftSaving, "PIPELINE_ERROR", format!("Failed to create capcut dir: {e}")))?;
  let draft_manifest_path = capcut_dir.join("draft_manifest.json");
  let canonical_duration_us = manifest.timeline_duration_us.or_else(|| capcut_input.as_ref().map(|input| input.duration_us));
  let final_draft_path = published.final_path.to_string_lossy().to_string();
  let staging_draft_path = published.staging_path.to_string_lossy().to_string();
  let desktop_root = published.desktop_root.to_string_lossy().to_string();
  let manifest_payload = json!({
    "draftId": draft_id,
    "draftPath": final_draft_path,
    "stagingPath": staging_draft_path,
    "desktopRoot": desktop_root,
    "draftUrl": saved_url,
    "visualTrackCount": manifest.visual_track_count,
    "audioTrackCount": manifest.audio_track_count,
    "captionTrackCount": manifest.caption_track_count,
    "timelineDurationUs": canonical_duration_us,
    "inputArtifactIds": capcut_input.as_ref().map(|input| &input.input_artifact_ids),
    "timelineArtifactId": capcut_input.as_ref().map(|input| &input.timeline_artifact_id),
    "captionsArtifactId": capcut_input.as_ref().map(|input| &input.captions_artifact_id),
    "source": manifest.source,
  });
  std::fs::write(&draft_manifest_path, serde_json::to_string_pretty(&manifest_payload).map_err(|e| PipelineRunError::new(PipelineStage::DraftSaving, "PIPELINE_ERROR", format!("{e}")))?).map_err(|e| PipelineRunError::new(PipelineStage::DraftSaving, "PIPELINE_ERROR", format!("Failed to write manifest: {e}")))?;

  let capcut_artifact = ArtifactStore::register_typed_artifact(&work_dir, job.id.as_str(), StageId::Capcut, "capcut_mate", ArtifactKind::CapcutDraft, &draft_manifest_path, json!({ "draft_id": draft_id, "final_path": final_draft_path, "staging_path": staging_draft_path, "input_artifact_ids": capcut_input.as_ref().map(|input| &input.input_artifact_ids) })).map_err(|e| PipelineRunError::new(PipelineStage::DraftSaving, "PIPELINE_ERROR", format!("{e:?}")))?;

  if let Some(context) = phase6_context.as_mut() {
    let artifact_ref = capcut_artifact.to_artifact_ref(StageId::Capcut).map_err(contract_pipeline_error)?;
    context.artifact_refs.push(artifact_ref);
  }

  outputs["draft_url"] = json!(final_draft_path);
  outputs["capcut_artifact"] = json!(capcut_artifact);
  outputs["draft_manifest"] = manifest_payload;
  persist_outputs(task_database, &job.id, PipelineStage::DraftReady, &serialize_outputs(&outputs)?).await?;

  // 6. Terminal completion, gated by the requested output_mode.
  match output_mode {
    OutputMode::DraftOnly => {
      info!("[JOB][DRAFT_ONLY] output_mode=draft_only — completing at DraftReady without render.");
      outputs["video_url"] = Value::Null;
      outputs["rendering_supported"] = json!(false);
      complete_capcut_stage(app_handle, phase6_context.as_mut(), &job_id_str, vec![capcut_artifact.id.clone()], 100, "Verified CapCut draft ready")?;
      if let Some(context) = phase6_context.as_ref() {
        outputs["pipeline_context"] = serde_json::to_value(context)?;
        outputs["capcut"] = json!({ "artifact_ids": [capcut_artifact.id.clone()], "stage": context.stage_states.iter().find(|state| state.stage_id == StageId::Capcut) });
      }
      finalize_draft_ready(app_handle, task_database, &job.id, &final_draft_path, &serialize_outputs(&outputs)?).await?;
    },
    OutputMode::RenderVideo => {
      check_cancelled(&job_id_str, PipelineStage::Rendering)?;
      emit_stage_progress(app_handle, task_database, &job.id, PipelineStage::DraftReady, PipelineStage::Rendering, 85, "Rendering video").await?;
      let _gpu_permit = dispatcher.acquire_gpu().await;

      info!("[JOB][CAPCUT] Rendering video (output_mode=render_video)...");
      let render_res = async {
        capcut_gen_video(client, &saved_url).await?;
        capcut_poll_gen_video_status(client, &saved_url, Some(Arc::clone(cancel_flag))).await
      }
      .await;

      match render_res {
        Ok(video_url) => {
          let rendered_path = work_dir.join("capcut").join("rendered_video.mp4");
          materialize_rendered_video(client, &video_url, &rendered_path).await.map_err(|error| map_capcut_error(PipelineStage::Rendering, &error))?;
          let rendered = ArtifactStore::register_typed_artifact(&work_dir, job.id.as_str(), StageId::Capcut, "capcut_mate", ArtifactKind::RenderedVideo, &rendered_path, json!({ "draft_id": draft_id, "render_url": video_url })).map_err(|error| PipelineRunError::new(PipelineStage::Rendering, "RENDER_FAILED", error.to_string()))?;
          if let Some(context) = phase6_context.as_mut() {
            context.artifact_refs.push(rendered.to_artifact_ref(StageId::Capcut).map_err(contract_pipeline_error)?);
          }
          complete_capcut_stage(app_handle, phase6_context.as_mut(), &job_id_str, vec![capcut_artifact.id.clone(), rendered.id.clone()], 100, "CapCut render artifact validated")?;
          outputs["video_url"] = json!(video_url);
          outputs["rendering_supported"] = json!(true);

          let maybe_page_id_str = job.maybe_page_id.as_deref().or_else(|| input.get("page_id").and_then(Value::as_str));
          if let Some(page_id) = maybe_page_id_str {
            if let Ok(Some(page)) = get_content_page_by_id(GetContentPageByIdArgs { db: task_database.get_connection(), id: page_id }).await {
              match OutputPathResolver::prepare_output_directory(&page.output_root, &page.name) {
                Ok(output_dir) => {
                  let filename = OutputPathResolver::generate_final_filename(job.id.as_str(), "video", "mp4");
                  match OutputPathResolver::publish_final_file(&rendered_path, &output_dir, &filename) {
                    Ok(published) => {
                      info!("[OutputPolicy] Final video published to {}", published.display());
                      outputs["final_published_path"] = json!(published.to_string_lossy().to_string());
                      outputs["final_output_directory"] = json!(output_dir.to_string_lossy().to_string());
                    },
                    Err(err) => warn!("[OutputPolicy] Failed to publish final video: {err}"),
                  }
                },
                Err(err) => warn!("[OutputPolicy] Failed to prepare output directory: {err}"),
              }
            }
          }

          if let Some(context) = phase6_context.as_ref() {
            outputs["pipeline_context"] = serde_json::to_value(context)?;
            outputs["capcut"] = json!({ "artifact_ids": [capcut_artifact.id.clone(), rendered.id], "stage": context.stage_states.iter().find(|state| state.stage_id == StageId::Capcut) });
          }
          finalize_completed(app_handle, task_database, &job.id, &final_draft_path, &video_url, &serialize_outputs(&outputs)?).await?;
        },
        Err(render_err) => {
          let err_str = format!("{render_err:?}");
          let code = extract_error_code(&err_str);
          if code == "RENDER_CANCELLED" {
            if let Some(context) = phase6_context.as_mut() {
              let cancelled_state = {
                let state = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Capcut).expect("capcut stage contract");
                state.cancel_stage(chrono::Utc::now().to_rfc3339()).map_err(contract_pipeline_error)?;
                state.clone()
              };
              emit_stage_state(app_handle, StageStatePayload { job_id: job_id_str.clone(), stage: cancelled_state.clone(), progress: Some(85), message: Some("CapCut render cancelled; downstream completion blocked".to_string()) });
              outputs["pipeline_context"] = serde_json::to_value(&*context)?;
              outputs["capcut"] = json!({ "artifact_ids": [capcut_artifact.id.clone()], "stage": cancelled_state });
              persist_outputs(task_database, &job.id, PipelineStage::Rendering, &serialize_outputs(&outputs)?).await?;
            }
            return Err(PipelineRunError::new(PipelineStage::Rendering, "RENDER_CANCELLED", err_str).into());
          }
          if allow_draft_fallback {
            warn!("[JOB][RENDER_FALLBACK] Render failed but allow_draft_fallback=true. Completing at DraftReady.");
            outputs["video_url"] = Value::Null;
            outputs["rendering_supported"] = json!(false);
            outputs["render_error"] = json!(err_str);
            complete_capcut_stage(app_handle, phase6_context.as_mut(), &job_id_str, vec![capcut_artifact.id.clone()], 100, "Render unavailable; verified draft fallback ready")?;
            if let Some(context) = phase6_context.as_ref() {
              outputs["pipeline_context"] = serde_json::to_value(context)?;
              outputs["capcut"] = json!({ "artifact_ids": [capcut_artifact.id.clone()], "stage": context.stage_states.iter().find(|state| state.stage_id == StageId::Capcut) });
            }
            finalize_draft_ready(app_handle, task_database, &job.id, &saved_url, &serialize_outputs(&outputs)?).await?;
          } else {
            // User asked for a real video and we could not produce one: fail.
            return Err(PipelineRunError::new(PipelineStage::Rendering, &code, err_str).into());
          }
        },
      }
    },
  }

  Ok(())
}

async fn finalize_completed(app_handle: &AppHandle, task_database: &TaskDatabase, job_id: &PipelineJobId, draft_url: &str, video_url: &str, outputs_string: &str) -> AnyhowResult<()> {
  update_pipeline_job_stage(UpdatePipelineJobStageArgs { db: task_database.get_connection(), pipeline_job_id: job_id, current_stage: PipelineStage::Completed, maybe_stage_outputs: Some(outputs_string) }).await?;
  update_pipeline_job_status(UpdatePipelineJobStatusArgs { db: task_database.get_connection(), pipeline_job_id: job_id, status: TaskStatus::CompleteSuccess }).await?;
  emit_job_complete(app_handle, JobCompletePayload { job_id: job_id.as_str().to_string(), result_type: "video".to_string(), stage: PipelineStage::Completed.to_str().to_string(), progress: 100, draft_url: draft_url.to_string(), video_url: Some(video_url.to_string()), rendering_supported: true });
  Ok(())
}

async fn finalize_draft_ready(app_handle: &AppHandle, task_database: &TaskDatabase, job_id: &PipelineJobId, draft_url: &str, outputs_string: &str) -> AnyhowResult<()> {
  update_pipeline_job_stage(UpdatePipelineJobStageArgs { db: task_database.get_connection(), pipeline_job_id: job_id, current_stage: PipelineStage::DraftReady, maybe_stage_outputs: Some(outputs_string) }).await?;
  update_pipeline_job_status(UpdatePipelineJobStatusArgs { db: task_database.get_connection(), pipeline_job_id: job_id, status: TaskStatus::CompleteSuccess }).await?;
  emit_job_complete(app_handle, JobCompletePayload { job_id: job_id.as_str().to_string(), result_type: "draft".to_string(), stage: PipelineStage::DraftReady.to_str().to_string(), progress: 100, draft_url: draft_url.to_string(), video_url: None, rendering_supported: false });
  Ok(())
}

fn map_capcut_error(stage: PipelineStage, err: &anyhow::Error) -> PipelineRunError {
  let err_str = format!("{err:?}");
  let code = extract_error_code(&err_str);
  PipelineRunError::new(stage, &code, err_str)
}

async fn run_ingest_analyze_stage(app_handle: &AppHandle, local_file: Option<&str>, source_url: Option<&str>, story_url: Option<&str>, work_dir: &std::path::Path, job_id: &str, cancel_flag: &Arc<AtomicBool>, download_config: &PipelineDownloadConfig) -> Result<(IngestAnalyzeResult, StageState), (PipelineRunError, StageState)> {
  let mut state = StageState::pending(StageId::IngestAnalyze);
  loop {
    let is_web = story_url.is_some() || (source_url.is_some() && !is_direct_video_or_media_url(source_url.unwrap()));
    let service = if local_file.is_some() { "vynaro" } else if is_web { "web_story_extractor" } else { "youwee" };
    if let Err(error) = state.start_stage(Some(service.to_string()), chrono::Utc::now().to_rfc3339()) {
      return Err((contract_pipeline_run_error(error), state));
    }
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(15), message: Some(format!("Acquiring content via {service}")) });

    let result = tokio::time::timeout(std::time::Duration::from_secs(stage_policy(StageId::IngestAnalyze).timeout_seconds), async {
      if let Some(path) = local_file {
        ingest_local_source_with_app(app_handle, std::path::Path::new(path), work_dir, job_id, Arc::clone(cancel_flag)).await
      } else if let Some(url) = story_url {
        ingest_web_story_source(url, work_dir, job_id, Arc::clone(cancel_flag)).await
      } else if let Some(url) = source_url {
        if is_direct_video_or_media_url(url) {
          ingest_url_source_with_config(app_handle, url, work_dir, job_id, Arc::clone(cancel_flag), download_config).await
        } else {
          ingest_web_story_source(url, work_dir, job_id, Arc::clone(cancel_flag)).await
        }
      } else {
        Err(IngestAnalyzeError { code: "INGEST_SOURCE_MISSING", message: "ingest_analyze requires a local file, web story URL, or video URL".to_string(), retryable: false, cancelled: false })
      }
    })
    .await
    .unwrap_or_else(|_| Err(IngestAnalyzeError { code: "INGEST_TIMEOUT", message: "Ingest/analyze stage exceeded its production timeout".to_string(), retryable: true, cancelled: false }));

    match result {
      Ok(result) => {
        state.service = Some(service.to_string());
        let ids = result.artifact_refs.iter().map(|artifact| artifact.artifact_id.clone()).collect();
        if let Err(error) = state.complete_stage(ids, chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(25), message: Some("Source artifacts validated".to_string()) });
        return Ok((result, state));
      },
      Err(error) if error.cancelled || cancel_flag.load(Ordering::SeqCst) => {
        if let Err(contract_error) = state.cancel_stage(chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(contract_error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(15), message: Some("Source ingestion cancelled".to_string()) });
        return Err((PipelineRunError::new(PipelineStage::IngestAnalyze, "INGEST_CANCELLED", error.message), state));
      },
      Err(error) => {
        if error.code.starts_with("VYNARO_") {
          state.service = Some("vynaro".to_string());
        }
        if let Err(contract_error) = state.fail_stage(StageError::sanitized(error.code, &error.message, error.retryable), chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(contract_error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(15), message: state.error.as_ref().map(|error| error.message.clone()) });
        if should_retry_ingest(&error, state.attempt) {
          if let Err(contract_error) = state.retry_stage() {
            return Err((contract_pipeline_run_error(contract_error), state));
          }
          emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(15), message: Some("Retrying source ingestion".to_string()) });
          continue;
        }
        return Err((map_ingest_error(error), state));
      },
    }
  }
}

async fn run_research_stage(app_handle: &AppHandle, input: &ResearchInput, work_dir: &std::path::Path, job_id: &str, cancel_flag: &Arc<AtomicBool>) -> Result<(Value, ArtifactRef, StageState), (PipelineRunError, StageState)> {
  let mut state = StageState::pending(StageId::Research);
  state.input_artifact_ids = input.artifact_ids.clone();
  loop {
    if let Err(error) = state.start_stage(Some("mediacrawler".to_string()), chrono::Utc::now().to_rfc3339()) {
      return Err((contract_pipeline_run_error(error), state));
    }
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(27), message: Some("Running MediaCrawler research".to_string()) });
    let operation = tokio::time::timeout(std::time::Duration::from_secs(stage_policy(StageId::Research).timeout_seconds), run_mediacrawler(input, Arc::clone(cancel_flag))).await.unwrap_or_else(|_| Err(ResearchError { code: "MEDIACRAWLER_TIMEOUT".to_string(), message: "Research stage exceeded its production timeout".to_string(), retryable: true, cancelled: false }));
    match operation {
      Ok(ResearchOutcome::Completed(payload)) => {
        let research_dir = work_dir.join("research");
        if let Err(error) = std::fs::create_dir_all(&research_dir) {
          return Err((PipelineRunError::new(PipelineStage::Research, "RESEARCH_ARTIFACT_FAILED", error.to_string()), state));
        }
        let path = research_dir.join("research.json");
        let serialized = match serde_json::to_string_pretty(&payload) {
          Ok(value) => value,
          Err(error) => return Err((PipelineRunError::new(PipelineStage::Research, "RESEARCH_ARTIFACT_INVALID", error.to_string()), state)),
        };
        if let Err(error) = std::fs::write(&path, serialized) {
          return Err((PipelineRunError::new(PipelineStage::Research, "RESEARCH_ARTIFACT_FAILED", error.to_string()), state));
        }
        let stored = match ArtifactStore::register_typed_artifact(work_dir, job_id, StageId::Research, "mediacrawler", ArtifactKind::Research, &path, canonical_metadata(input)) {
          Ok(value) => value,
          Err(error) => return Err((PipelineRunError::new(PipelineStage::Research, "RESEARCH_ARTIFACT_INVALID", error.to_string()), state)),
        };
        let artifact_ref = match stored.to_artifact_ref(StageId::Research) {
          Ok(value) => value,
          Err(error) => return Err((PipelineRunError::new(PipelineStage::Research, "RESEARCH_ARTIFACT_INVALID", error.to_string()), state)),
        };
        if let Err(error) = state.complete_stage(vec![artifact_ref.artifact_id.clone()], chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(29), message: Some("Research artifact validated".to_string()) });
        return Ok((payload, artifact_ref, state));
      },
      Ok(ResearchOutcome::WaitingInput { code, message }) => {
        let action = StageError::sanitized(code.clone(), &message, true);
        if let Err(contract_error) = state.wait_for_input(action, chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(contract_error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(27), message: Some(message.clone()) });
        return Err((PipelineRunError::new(PipelineStage::Research, &code, message), state));
      },
      Err(error) if error.cancelled || cancel_flag.load(Ordering::SeqCst) => {
        let _ = state.cancel_stage(chrono::Utc::now().to_rfc3339());
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(27), message: Some("Research cancelled".to_string()) });
        return Err((map_research_error(error), state));
      },
      Err(error) => {
        if let Err(contract_error) = state.fail_stage(research_stage_error(&error), chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(contract_error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(27), message: state.error.as_ref().map(|error| error.message.clone()) });
        if should_retry_research(&error, state.attempt) && !cancel_flag.load(Ordering::SeqCst) {
          if let Err(contract_error) = state.retry_stage() {
            return Err((contract_pipeline_run_error(contract_error), state));
          }
          emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(27), message: Some("Retrying MediaCrawler research after transient failure".to_string()) });
          continue;
        }
        return Err((map_research_error(error), state));
      },
    }
  }
}

struct StoryScriptStageResult {
  story: Value,
  script_request: Value,
  script: StructuredScript,
  artifact_refs: Vec<ArtifactRef>,
  stage: StageState,
}

struct VoiceStageResult {
  timing: VoiceTiming,
  artifact_refs: Vec<ArtifactRef>,
  stage: StageState,
}

struct MediaTimelineStageResult {
  timeline: Value,
  captions: Value,
  artifact_refs: Vec<ArtifactRef>,
  stage: StageState,
}

async fn run_media_timeline_stage(app_handle: &AppHandle, input: &MediaTimelineInput, work_dir: &std::path::Path, job_id: &str, cancel_flag: &Arc<AtomicBool>) -> Result<MediaTimelineStageResult, (PipelineRunError, StageState)> {
  let mut state = StageState::pending(StageId::MediaTimeline);
  state.input_artifact_ids = input.input_artifact_ids.clone();
  loop {
    if let Err(error) = state.start_stage(Some("openmontage".to_string()), chrono::Utc::now().to_rfc3339()) {
      return Err((contract_pipeline_run_error(error), state));
    }
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(41), message: Some("Composing media timeline in OpenMontage".to_string()) });
    let operation = tokio::time::timeout(std::time::Duration::from_secs(stage_policy(StageId::MediaTimeline).timeout_seconds), run_openmontage(input, Arc::clone(cancel_flag))).await.unwrap_or_else(|_| Err(MediaTimelineError::new("OPENMONTAGE_TIMEOUT", "Media timeline stage exceeded its production timeout", true)));
    match operation {
      Ok(output) => {
        let metadata = json!({ "input_artifact_ids": &input.input_artifact_ids, "duration_seconds": output.timeline.get("durationSeconds") });
        let timeline = ArtifactStore::register_typed_artifact(work_dir, job_id, StageId::MediaTimeline, "openmontage", ArtifactKind::Timeline, &output.timeline_path, metadata.clone()).map_err(|error| media_timeline_stage_failure(&mut state, MediaTimelineError::new("MEDIA_TIMELINE_ARTIFACT_INVALID", error.to_string(), false)))?;
        let captions = ArtifactStore::register_typed_artifact(work_dir, job_id, StageId::MediaTimeline, "openmontage", ArtifactKind::Captions, &output.captions_path, metadata).map_err(|error| media_timeline_stage_failure(&mut state, MediaTimelineError::new("MEDIA_TIMELINE_ARTIFACT_INVALID", error.to_string(), false)))?;
        let refs = vec![timeline.to_artifact_ref(StageId::MediaTimeline).map_err(|error| media_timeline_stage_failure(&mut state, MediaTimelineError::new("MEDIA_TIMELINE_ARTIFACT_INVALID", error.to_string(), false)))?, captions.to_artifact_ref(StageId::MediaTimeline).map_err(|error| media_timeline_stage_failure(&mut state, MediaTimelineError::new("MEDIA_TIMELINE_ARTIFACT_INVALID", error.to_string(), false)))?];
        let ids = refs.iter().map(|artifact| artifact.artifact_id.clone()).collect();
        if let Err(error) = state.complete_stage(ids, chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(43), message: Some("Timeline and captions artifacts validated".to_string()) });
        return Ok(MediaTimelineStageResult { timeline: output.timeline, captions: output.captions, artifact_refs: refs, stage: state });
      },
      Err(error) if error.cancelled || cancel_flag.load(Ordering::SeqCst) => {
        let _ = state.cancel_stage(chrono::Utc::now().to_rfc3339());
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(41), message: Some("Media timeline cancelled".to_string()) });
        return Err((map_media_timeline_error(error), state));
      },
      Err(error) => {
        if let Err(contract_error) = state.fail_stage(StageError::sanitized(error.code.clone(), &error.message, error.retryable), chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(contract_error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(41), message: state.error.as_ref().map(|error| error.message.clone()) });
        if should_retry_media_timeline(&error, state.attempt) && !cancel_flag.load(Ordering::SeqCst) {
          if let Err(contract_error) = state.retry_stage() {
            return Err((contract_pipeline_run_error(contract_error), state));
          }
          emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(41), message: Some("Retrying OpenMontage after transient failure".to_string()) });
          continue;
        }
        return Err((map_media_timeline_error(error), state));
      },
    }
  }
}

fn media_timeline_stage_failure(state: &mut StageState, error: MediaTimelineError) -> (PipelineRunError, StageState) {
  let _ = state.fail_stage(StageError::sanitized(error.code.clone(), &error.message, error.retryable), chrono::Utc::now().to_rfc3339());
  (map_media_timeline_error(error), state.clone())
}

async fn run_voice_stage(app_handle: &AppHandle, input: &VoiceInput, work_dir: &std::path::Path, job_id: &str, cancel_flag: &Arc<AtomicBool>) -> Result<VoiceStageResult, (PipelineRunError, StageState)> {
  let mut state = StageState::pending(StageId::Voice);
  state.input_artifact_ids = vec![input.script_artifact_id.clone()];
  loop {
    if let Err(error) = state.start_stage(Some("omniroute_tts_vynaro".to_string()), chrono::Utc::now().to_rfc3339()) {
      return Err((contract_pipeline_run_error(error), state));
    }
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(37), message: Some("Synthesizing narration through OmniRoute".to_string()) });
    let operation = tokio::time::timeout(std::time::Duration::from_secs(stage_policy(StageId::Voice).timeout_seconds), synthesize_voice_with_runtime(app_handle, input, work_dir, Arc::clone(cancel_flag))).await.unwrap_or_else(|_| Err(VoiceError::new("FFMPEG_TIMEOUT", "Voice media stage exceeded its production timeout", true)));
    match operation {
      Ok(output) => {
        let audio_metadata = json!({ "script_artifact_id": input.script_artifact_id, "model": input.model, "voice": input.voice, "language": input.language, "duration_seconds": output.timing.duration_seconds, "audio_codec": output.audio_codec, "size_bytes": output.size_bytes });
        let timing_metadata = json!({ "script_artifact_id": input.script_artifact_id, "source": "ffprobe", "segment_count": output.timing.segments.len(), "duration_seconds": output.timing.duration_seconds });
        let audio = ArtifactStore::register_typed_artifact(work_dir, job_id, StageId::Voice, "omniroute_tts", ArtifactKind::VoiceAudio, &output.audio_path, audio_metadata).map_err(|error| voice_stage_failure(&mut state, VoiceError::new("VOICE_ARTIFACT_INVALID", error.to_string(), false)))?;
        let timing = ArtifactStore::register_typed_artifact(work_dir, job_id, StageId::Voice, "vynaro_ffprobe", ArtifactKind::VoiceTiming, &output.timing_path, timing_metadata).map_err(|error| voice_stage_failure(&mut state, VoiceError::new("VOICE_ARTIFACT_INVALID", error.to_string(), false)))?;
        let refs = vec![audio.to_artifact_ref(StageId::Voice).map_err(|error| voice_stage_failure(&mut state, VoiceError::new("VOICE_ARTIFACT_INVALID", error.to_string(), false)))?, timing.to_artifact_ref(StageId::Voice).map_err(|error| voice_stage_failure(&mut state, VoiceError::new("VOICE_ARTIFACT_INVALID", error.to_string(), false)))?];
        let ids = refs.iter().map(|artifact| artifact.artifact_id.clone()).collect();
        if let Err(error) = state.complete_stage(ids, chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(40), message: Some("Voice audio and measured timing artifacts validated".to_string()) });
        return Ok(VoiceStageResult { timing: output.timing, artifact_refs: refs, stage: state });
      },
      Err(error) if error.cancelled || cancel_flag.load(Ordering::SeqCst) => {
        let _ = state.cancel_stage(chrono::Utc::now().to_rfc3339());
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(37), message: Some("Voice synthesis cancelled".to_string()) });
        return Err((map_voice_error(error), state));
      },
      Err(error) => {
        if let Err(contract_error) = state.fail_stage(StageError::sanitized(error.code.clone(), &error.message, error.retryable), chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(contract_error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(37), message: state.error.as_ref().map(|error| error.message.clone()) });
        if should_retry_voice(&error, state.attempt) && !cancel_flag.load(Ordering::SeqCst) {
          if let Err(contract_error) = state.retry_stage() {
            return Err((contract_pipeline_run_error(contract_error), state));
          }
          emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(37), message: Some("Retrying voice synthesis after transient failure".to_string()) });
          continue;
        }
        return Err((map_voice_error(error), state));
      },
    }
  }
}

fn voice_stage_failure(state: &mut StageState, error: VoiceError) -> (PipelineRunError, StageState) {
  let _ = state.fail_stage(StageError::sanitized(error.code.clone(), &error.message, error.retryable), chrono::Utc::now().to_rfc3339());
  (map_voice_error(error), state.clone())
}

async fn run_story_script_stage(app_handle: &AppHandle, input: &StoryScriptInput, model: Option<&str>, work_dir: &std::path::Path, job_id: &str, cancel_flag: &Arc<AtomicBool>) -> Result<StoryScriptStageResult, (PipelineRunError, StageState)> {
  let mut state = StageState::pending(StageId::StoryScript);
  state.input_artifact_ids = input.input_artifact_ids.clone();
  loop {
    if let Err(error) = state.start_stage(Some("story_studio_omniroute".to_string()), chrono::Utc::now().to_rfc3339()) {
      return Err((contract_pipeline_run_error(error), state));
    }
    emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(30), message: Some("Planning story in Story Studio".to_string()) });
    let operation = tokio::time::timeout(std::time::Duration::from_secs(stage_policy(StageId::StoryScript).timeout_seconds), execute_story_script_attempt(input, model, work_dir, job_id, cancel_flag)).await.unwrap_or_else(|_| Err(StoryScriptError::new("STORY_STUDIO_TIMEOUT", "Story/script stage exceeded its production timeout", true)));
    match operation {
      Ok((studio, script, artifact_refs)) => {
        let output_ids = artifact_refs.iter().map(|artifact| artifact.artifact_id.clone()).collect();
        if let Err(error) = state.complete_stage(output_ids, chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(35), message: Some("Story, script request, and script artifacts validated".to_string()) });
        return Ok(StoryScriptStageResult { story: studio.story, script_request: studio.script_request, script, artifact_refs, stage: state });
      },
      Err(error) if error.cancelled || cancel_flag.load(Ordering::SeqCst) => {
        let _ = state.cancel_stage(chrono::Utc::now().to_rfc3339());
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(30), message: Some("Story script cancelled".to_string()) });
        return Err((map_story_script_error(error), state));
      },
      Err(error) => {
        if let Err(contract_error) = state.fail_stage(StageError::sanitized(error.code.clone(), &error.message, error.retryable), chrono::Utc::now().to_rfc3339()) {
          return Err((contract_pipeline_run_error(contract_error), state));
        }
        emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(30), message: state.error.as_ref().map(|error| error.message.clone()) });
        if should_retry_story_script(&error, state.attempt) && !cancel_flag.load(Ordering::SeqCst) {
          if let Err(contract_error) = state.retry_stage() {
            return Err((contract_pipeline_run_error(contract_error), state));
          }
          emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(30), message: Some("Retrying Story Studio / OmniRoute after transient failure".to_string()) });
          continue;
        }
        return Err((map_story_script_error(error), state));
      },
    }
  }
}

async fn execute_story_script_attempt(input: &StoryScriptInput, model: Option<&str>, work_dir: &std::path::Path, job_id: &str, cancel_flag: &Arc<AtomicBool>) -> Result<(StoryStudioOutput, StructuredScript, Vec<ArtifactRef>), StoryScriptError> {
  let studio = run_story_studio(input, Arc::clone(cancel_flag)).await?;
  if cancel_flag.load(Ordering::SeqCst) {
    return Err(StoryScriptError::new("STORY_SCRIPT_CANCELLED", "Story script stage cancelled", false));
  }
  let script = tokio::select! {
    result = execute_story_script_request(&studio.script_request, model) => result,
    _ = wait_for_cancel_flag(cancel_flag) => return Err(StoryScriptError::new("STORY_SCRIPT_CANCELLED", "Story script stage cancelled", false)),
  }
  .map_err(|error| {
    let message = format!("{error:?}");
    let code = extract_error_code(&message);
    let retryable = matches!(code.as_str(), "LLM_UNAVAILABLE" | "LLM_TIMEOUT" | "LLM_RATE_LIMITED");
    StoryScriptError::new(code, message, retryable)
  })?;
  if cancel_flag.load(Ordering::SeqCst) {
    return Err(StoryScriptError::new("STORY_SCRIPT_CANCELLED", "Story script stage cancelled", false));
  }

  let dir = work_dir.join("story_script");
  std::fs::create_dir_all(&dir).map_err(|error| StoryScriptError::new("STORY_ARTIFACT_FAILED", error.to_string(), false))?;
  let story_path = dir.join("story.json");
  let request_path = dir.join("script_request.json");
  let script_path = dir.join("script.json");
  let scene_plan_path = dir.join("scene_plan.json");
  write_stage_json(&story_path, &studio.story)?;
  write_stage_json(&request_path, &studio.script_request)?;
  write_stage_json(&script_path, &script)?;
  let scene_plan = build_scene_plan(&script, input.target_duration_seconds).map_err(|error| StoryScriptError::new(error.code, error.message, false))?;
  write_stage_json(&scene_plan_path, &scene_plan)?;

  let items = [(ArtifactKind::Story, story_path.as_path(), "story_studio"), (ArtifactKind::ScriptRequest, request_path.as_path(), "story_studio"), (ArtifactKind::Script, script_path.as_path(), "omniroute"), (ArtifactKind::ScenePlan, scene_plan_path.as_path(), "floword_scene_planner")];
  let mut refs = Vec::with_capacity(items.len());
  for (kind, path, service) in items {
    let stored = ArtifactStore::register_typed_artifact(work_dir, job_id, StageId::StoryScript, service, kind, path, json!({ "input_artifact_ids": &input.input_artifact_ids })).map_err(|error| StoryScriptError::new("STORY_ARTIFACT_INVALID", error.to_string(), false))?;
    refs.push(stored.to_artifact_ref(StageId::StoryScript).map_err(|error| StoryScriptError::new("STORY_ARTIFACT_INVALID", error.to_string(), false))?);
  }
  Ok((studio, script, refs))
}

fn write_stage_json(path: &std::path::Path, value: &impl serde::Serialize) -> Result<(), StoryScriptError> {
  let bytes = serde_json::to_vec_pretty(value).map_err(|error| StoryScriptError::new("STORY_ARTIFACT_INVALID", error.to_string(), false))?;
  std::fs::write(path, bytes).map_err(|error| StoryScriptError::new("STORY_ARTIFACT_FAILED", error.to_string(), false))
}

async fn wait_for_cancel_flag(flag: &Arc<AtomicBool>) {
  while !flag.load(Ordering::SeqCst) {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
  }
}

fn pipeline_context_from_input(
  job_id: &str,
  input: &Value,
  prompt: &str,
  model_id: Option<String>,
  language: &str,
  target_duration_seconds: u32,
  output_mode: OutputMode,
  local_file: Option<String>,
  source_url: Option<String>,
  story_url: Option<String>,
  content_source: Option<String>,
) -> AnyhowResult<PipelineContext> {
  let mut stage_states = PipelineContext::initial_stage_states();
  if let Some(input_state) = stage_states.iter_mut().find(|state| state.stage_id == StageId::Input) {
    input_state.start_stage(Some("rust_worker".to_string()), chrono::Utc::now().to_rfc3339()).map_err(contract_pipeline_error)?;
    input_state.complete_stage(Vec::new(), chrono::Utc::now().to_rfc3339()).map_err(contract_pipeline_error)?;
  }
  Ok(PipelineContext {
    job_id: job_id.to_string(),
    project_id: input.get("project_id").and_then(Value::as_str).map(str::to_string),
    workflow_mode: input.get("workflow_mode").and_then(Value::as_str).unwrap_or("source_based").to_string(),
    content_source,
    prompt: prompt.to_string(),
    model_id,
    voice_id: input.get("voice_id").and_then(Value::as_str).map(str::to_string),
    language: language.to_string(),
    target_duration_seconds,
    output_mode: match output_mode {
      OutputMode::DraftOnly => "draft_only",
      OutputMode::RenderVideo => "render_video",
    }
    .to_string(),
    source_url,
    local_file,
    story_url,
    research_enabled: input.get("research_enabled").and_then(Value::as_bool).unwrap_or(false),
    research_platform: input.get("research_platform").and_then(Value::as_str).map(str::to_string),
    research_query: input.get("research_query").and_then(Value::as_str).map(str::to_string),
    research_mode: input.get("research_mode").and_then(Value::as_str).map(str::to_string),
    xhs_variant: input.get("xhs_variant").and_then(Value::as_str).map(str::to_string),
    artifact_refs: Vec::new(),
    stage_states,
  })
}

fn first_input_string(input: &Value, singular_key: &str, plural_key: &str) -> Option<String> {
  input.get(singular_key).and_then(Value::as_str).or_else(|| input.get(plural_key).and_then(Value::as_array).and_then(|values| values.first()).and_then(Value::as_str)).map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
}

fn load_structured_script(context: &PipelineContext) -> AnyhowResult<StructuredScript> {
  let state = context.stage_states.iter().find(|state| state.stage_id == StageId::StoryScript).ok_or_else(|| anyhow::anyhow!("story_script stage is missing"))?;
  let artifact = state.output_artifact_ids.iter().filter_map(|id| context.artifact_refs.iter().find(|artifact| artifact.artifact_id == *id)).find(|artifact| artifact.kind == ArtifactKind::Script).ok_or_else(|| anyhow::anyhow!("script artifact is missing"))?;
  artifact.validate().map_err(|error| anyhow::anyhow!(error.to_string()))?;
  Ok(serde_json::from_slice(&std::fs::read(&artifact.location)?)?)
}

fn reusable_draft_url(context: &PipelineContext) -> AnyhowResult<String> {
  let state = context.stage_states.iter().find(|state| state.stage_id == StageId::Capcut).ok_or_else(|| anyhow::anyhow!("capcut stage is missing"))?;
  let artifact = state.output_artifact_ids.iter().filter_map(|id| context.artifact_refs.iter().find(|artifact| artifact.artifact_id == *id)).find(|artifact| artifact.kind == ArtifactKind::CapcutDraft).ok_or_else(|| anyhow::anyhow!("capcut_draft artifact is missing"))?;
  artifact.validate().map_err(|error| anyhow::anyhow!(error.to_string()))?;
  let manifest: Value = serde_json::from_slice(&std::fs::read(&artifact.location)?)?;
  manifest.get("draftPath").and_then(Value::as_str).map(str::to_string).ok_or_else(|| anyhow::anyhow!("capcut_draft manifest is missing draftPath"))
}

fn map_ingest_error(error: IngestAnalyzeError) -> PipelineRunError {
  PipelineRunError::new(PipelineStage::IngestAnalyze, error.code, error.message)
}

fn map_research_error(error: ResearchError) -> PipelineRunError {
  PipelineRunError::new(PipelineStage::Research, &error.code, error.message)
}

fn map_story_script_error(error: StoryScriptError) -> PipelineRunError {
  PipelineRunError::new(PipelineStage::ScriptGenerating, &error.code, error.message)
}

fn map_voice_error(error: VoiceError) -> PipelineRunError {
  PipelineRunError::new(PipelineStage::ScriptGenerating, &error.code, error.message)
}

fn map_media_timeline_error(error: MediaTimelineError) -> PipelineRunError {
  PipelineRunError::new(PipelineStage::MediaTimeline, &error.code, error.message)
}

fn log_media_timeline_inputs(context: &PipelineContext, job_id: &str) {
  info!("[OPENMONTAGE][REQUEST] job_id={job_id} stage=media_timeline");
  for kind in [ArtifactKind::SourceVideo, ArtifactKind::Scenes, ArtifactKind::ScenePlan, ArtifactKind::GeneratedVideo, ArtifactKind::Script, ArtifactKind::VoiceAudio, ArtifactKind::VoiceTiming] {
    if let Some(artifact) = context.artifact_refs.iter().find(|artifact| artifact.kind == kind) {
      let path = std::path::Path::new(&artifact.location);
      let metadata = std::fs::metadata(path).ok();
      info!("[OPENMONTAGE][INPUT] kind={} artifact_id={} path={} exists={} size={} file_type={}", kind.as_str(), artifact.artifact_id, path.display(), metadata.is_some(), metadata.as_ref().map(std::fs::Metadata::len).unwrap_or(0), path.extension().and_then(|value| value.to_str()).unwrap_or("unknown"));
    } else {
      info!("[OPENMONTAGE][INPUT] kind={} artifact_id=missing path=missing exists=false size=0 file_type=unknown", kind.as_str());
    }
  }
}

fn complete_capcut_stage(app_handle: &AppHandle, context: Option<&mut PipelineContext>, job_id: &str, artifact_ids: Vec<String>, progress: u8, message: &str) -> AnyhowResult<()> {
  let Some(context) = context else {
    return Ok(());
  };
  let state = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Capcut).ok_or_else(|| contract_pipeline_error("capcut stage is missing"))?;
  state.complete_stage(artifact_ids, chrono::Utc::now().to_rfc3339()).map_err(contract_pipeline_error)?;
  emit_stage_state(app_handle, StageStatePayload { job_id: job_id.to_string(), stage: state.clone(), progress: Some(progress), message: Some(message.to_string()) });
  Ok(())
}

fn should_retry_ingest(error: &IngestAnalyzeError, attempt: u32) -> bool {
  error.retryable && !error.cancelled && attempt < INGEST_MAX_ATTEMPTS
}

fn should_run_initial_service_preflight(has_source_input: bool) -> bool {
  !has_source_input
}

fn validate_workflow_input(
  workflow_mode: &str,
  content_source: ContentSource,
  prompt: &str,
  has_source_input: bool,
  research_enabled: bool,
  research_query: Option<&str>,
) -> Result<(), PipelineRunError> {
  match content_source {
    ContentSource::PromptOnly => {
      if prompt.trim().is_empty() {
        return Err(PipelineRunError::new(PipelineStage::PreflightCheck, "PROMPT_REQUIRED", "Prompt is required for Prompt Only mode".to_string()));
      }
    },
    ContentSource::TrendResearch => {
      if (!research_enabled || research_query.map(|q| q.trim().is_empty()).unwrap_or(true)) && prompt.trim().is_empty() {
        return Err(PipelineRunError::new(PipelineStage::PreflightCheck, "RESEARCH_QUERY_REQUIRED", "Research query or prompt is required for Trend Research mode".to_string()));
      }
    },
    ContentSource::WebStory | ContentSource::VideoUrl | ContentSource::LocalMedia => {
      if !has_source_input && prompt.trim().is_empty() {
        return Err(PipelineRunError::new(PipelineStage::PreflightCheck, "SOURCE_REQUIRED", "Source file or URL is required for this content source".to_string()));
      }
    },
    ContentSource::Auto => {
      let original_creation = matches!(workflow_mode, "original" | "original_creation");
      if original_creation && prompt.trim().is_empty() {
        return Err(PipelineRunError::new(PipelineStage::PreflightCheck, "PROMPT_REQUIRED", "Original Creation requires a prompt".to_string()));
      }
      if !original_creation && !has_source_input && prompt.trim().is_empty() {
        return Err(PipelineRunError::new(PipelineStage::PreflightCheck, "SOURCE_REQUIRED", "This workflow mode requires a source file, URL, or prompt".to_string()));
      }
    },
  }
  Ok(())
}

fn contract_pipeline_error(error: impl std::fmt::Display) -> anyhow::Error {
  contract_pipeline_run_error(error).into()
}

fn contract_pipeline_run_error(error: impl std::fmt::Display) -> PipelineRunError {
  PipelineRunError::new(PipelineStage::PreflightCheck, "INGEST_CONTRACT_FAILED", error.to_string())
}

fn research_contract_pipeline_run_error(error: impl std::fmt::Display) -> PipelineRunError {
  PipelineRunError::new(PipelineStage::Research, "RESEARCH_CONTRACT_FAILED", error.to_string())
}

async fn emit_stage_progress(app_handle: &AppHandle, task_database: &TaskDatabase, job_id: &PipelineJobId, current: PipelineStage, next: PipelineStage, progress: u32, message: &str) -> AnyhowResult<()> {
  update_pipeline_job_stage(UpdatePipelineJobStageArgs { db: task_database.get_connection(), pipeline_job_id: job_id, current_stage: next, maybe_stage_outputs: None }).await?;

  emit_stage_complete(app_handle, StageCompletePayload { job_id: job_id.as_str().to_string(), completed_stage: current.to_str().to_string(), next_stage: next.to_str().to_string(), progress, stage_message: Some(message.to_string()) });

  Ok(())
}

async fn persist_outputs(task_database: &TaskDatabase, job_id: &PipelineJobId, stage: PipelineStage, stage_outputs: &str) -> AnyhowResult<()> {
  update_pipeline_job_stage(UpdatePipelineJobStageArgs { db: task_database.get_connection(), pipeline_job_id: job_id, current_stage: stage, maybe_stage_outputs: Some(stage_outputs) }).await?;
  Ok(())
}

fn serialize_outputs(outputs: &Value) -> AnyhowResult<String> {
  Ok(serde_json::to_string(outputs)?)
}

fn parse_input(job: &PipelineJob) -> Value {
  job.maybe_input_payload.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or_else(|| json!({}))
}

fn parse_stage_outputs(job: &PipelineJob) -> Value {
  job.maybe_stage_outputs.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or_else(|| json!({}))
}

/// Canonical Floword orchestrator runner for Grok image editing stage.
/// Drives the image edit stage with mandatory canonical per-job artifact root.
pub async fn run_grok_image_edit_stage(
  job_id: &str,
  page_id: &str,
  source_image_artifact: ArtifactRef,
  prompt: &str,
  work_dir: &std::path::Path,
  attempt_id: &str,
) -> Result<GrokImageEditOutput, PipelineRunError> {
  let input = GrokImageEditInput {
    job_id: job_id.to_string(),
    page_id: page_id.to_string(),
    source_image_artifact,
    prompt: prompt.to_string(),
    timeout_ms: Some(180000),
    workflow_root: work_dir.to_path_buf(),
  };
  execute_grok_image_edit_stage(input, attempt_id)
    .await
    .map_err(|err| PipelineRunError::new(PipelineStage::ScriptGenerating, "GROK_IMAGE_EDIT_FAILED", err))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn output_mode_parses_render_video_and_defaults_to_draft_only() {
    assert_eq!(OutputMode::parse(Some("render_video")), OutputMode::RenderVideo);
    assert_eq!(OutputMode::parse(Some("draft_only")), OutputMode::DraftOnly);
    assert_eq!(OutputMode::parse(None), OutputMode::DraftOnly);
    assert_eq!(OutputMode::parse(Some("garbage")), OutputMode::DraftOnly);
  }

  #[test]
  fn error_code_extraction_recognizes_render_and_llm_codes() {
    assert_eq!(extract_error_code("boom RENDER_TIMEOUT boom"), "RENDER_TIMEOUT");
    assert_eq!(extract_error_code("LLM_INVALID_RESPONSE: bad json"), "LLM_INVALID_RESPONSE");
    assert_eq!(extract_error_code("nothing matches here"), "PIPELINE_ERROR");
  }

  #[test]
  fn local_file_has_priority_over_url_input() {
    let input = json!({ "local_file": "C:/fixture.mp4", "source_urls": ["https://example.com/video"] });
    assert_eq!(first_input_string(&input, "local_file", "source_files").as_deref(), Some("C:/fixture.mp4"));
  }

  #[test]
  fn ingest_retry_policy_only_retries_structured_retryable_failures() {
    let retryable = IngestAnalyzeError { code: "YOUWEE_DOWNLOAD_FAILED", message: "temporary".to_string(), retryable: true, cancelled: false };
    let terminal = IngestAnalyzeError { code: "LOCAL_SOURCE_INVALID", message: "missing".to_string(), retryable: false, cancelled: false };
    assert!(should_retry_ingest(&retryable, 1));
    assert!(!should_retry_ingest(&retryable, INGEST_MAX_ATTEMPTS));
    assert!(!should_retry_ingest(&terminal, 1));
  }

  #[test]
  fn source_workflow_runs_ingest_before_downstream_service_preflight() {
    assert!(!should_run_initial_service_preflight(true));
    assert!(should_run_initial_service_preflight(false));
  }

  #[test]
  fn original_accepts_prompt_without_source_while_source_modes_fail_early() {
    assert!(validate_workflow_input("original", ContentSource::PromptOnly, "Create a short video", false, false, None).is_ok());
    assert!(validate_workflow_input("original", ContentSource::Auto, "Create a short video", false, false, None).is_ok());
    assert_eq!(validate_workflow_input("source_based", ContentSource::VideoUrl, "", false, false, None).unwrap_err().error_code, "SOURCE_REQUIRED");
    assert_eq!(validate_workflow_input("remix", ContentSource::LocalMedia, "", false, false, None).unwrap_err().error_code, "SOURCE_REQUIRED");
  }

  #[test]
  fn mediacrawler_auth_failure_is_reported_by_research_stage() {
    let error = map_research_error(ResearchError::auth_required("cookies required"));
    assert_eq!(error.stage, PipelineStage::Research);
    assert_eq!(error.error_code, "MEDIACRAWLER_AUTH_REQUIRED");
  }

  #[tokio::test]
  #[ignore = "requires live InkOS Story Studio and OmniRoute runtimes"]
  async fn runtime_story_studio_to_omniroute_registers_canonical_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let input = StoryScriptInput {
      prompt: "Kể lại video ngắn, rõ ràng và hấp dẫn.".to_string(),
      language: "vi".to_string(),
      target_duration_seconds: 8,
      model_id: None,
      input_artifact_ids: vec!["runtime-source-metadata".to_string(), "runtime-scenes".to_string()],
      source_metadata: json!({ "probe": { "duration_seconds": 8, "width": 1280, "height": 720 } }),
      scenes: json!({ "scenes": [
        { "index": 0, "start_seconds": 0, "end_seconds": 4 },
        { "index": 1, "start_seconds": 4, "end_seconds": 8 }
      ] }),
      research: None,
      source_text: None,
      workflow_mode: "source_based".to_string(),
    };
    let cancel = Arc::new(AtomicBool::new(false));

    let (studio, script, artifacts) = execute_story_script_attempt(&input, None, root.path(), "phase3-runtime", &cancel).await.unwrap();

    assert!(studio.story.get("beats").and_then(Value::as_array).is_some_and(|beats| !beats.is_empty()));
    assert_eq!(studio.script_request.get("executor").and_then(Value::as_str), Some("omniroute"));
    assert!(!script.scenes.is_empty());
    assert_eq!(artifacts.len(), 4);
    for kind in [ArtifactKind::Story, ArtifactKind::ScriptRequest, ArtifactKind::Script] {
      let artifact = artifacts.iter().find(|artifact| artifact.kind == kind).unwrap();
      artifact.validate().unwrap();
      let body = std::fs::read(&artifact.location).unwrap();
      serde_json::from_slice::<Value>(&body).unwrap();
    }
  }
}
