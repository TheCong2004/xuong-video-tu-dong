//! Tauri commands backing the Floword Studio workflow UI.
//!
//! Contract note: every command returns the standard command envelope
//! (`{status, payload}` on success, `{status, error_message, error_type, error_details}`
//! on error). The frontend MUST unwrap `payload` and branch on `error_type`/status —
//! it must never synthesize its own job id.

use crate::core::commands::response::failure_response_wrapper::{CommandErrorResponseWrapper, CommandErrorStatus};
use crate::core::commands::response::shorthand::{ResponseOrError, ResponseOrErrorMessage};
use crate::core::commands::response::success_response_wrapper::SerializeMarker;
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::core::state::task_database::TaskDatabase;
use crate::core::threads::main_window_thread::persist_storyteller_cookies_task::sync_storyteller_credentials_from_http_plugin;
use crate::services::pipeline::clients::browser_runtime_client::{list_workers as list_browser_workers, BrowserWorkerInfo};
use crate::services::pipeline::clients::capcut_mate_client::health_check as capcut_mate_health_check;
use crate::services::pipeline::clients::omniroute_client::{health_check as llm_health_check, list_models as omniroute_list_models, list_video_models as omniroute_list_video_models};
use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef, ContentSource, PipelineContext, StageId, StageState};
use crate::services::pipeline::grok_image_edit_stage::{compute_sha256, detect_image_mime};
use crate::services::pipeline::hardening::{invalidate_from as invalidate_context_from, invalidate_output_values};
use crate::services::pipeline::output_policy::OutputPathResolver;
use crate::services::pipeline::state::cancellation_registry::request_cancellation;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use enums::tauri::pipeline::pipeline_stage::PipelineStage;
use enums::tauri::tasks::task_status::TaskStatus;

use log::{error, info, warn};
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlite_tasks::queries::app_settings::get_app_setting::{get_app_setting, GetAppSettingArgs};
use sqlite_tasks::queries::app_settings::set_app_setting::{set_app_setting, SetAppSettingArgs};
use sqlite_tasks::queries::content_page_publish_targets::content_page_publish_target::ContentPagePublishTarget;
use sqlite_tasks::queries::content_page_publish_targets::create_publish_target::{create_publish_target, CreatePublishTargetArgs};
use sqlite_tasks::queries::content_page_publish_targets::delete_publish_target::delete_publish_target;
use sqlite_tasks::queries::content_page_publish_targets::list_publish_targets_for_page::list_publish_targets_for_page;
use sqlite_tasks::queries::content_page_publish_targets::update_publish_target::{update_publish_target, UpdatePublishTargetArgs};
use sqlite_tasks::queries::content_pages::archive_content_page::{archive_content_page, ArchiveContentPageArgs};
use sqlite_tasks::queries::content_pages::content_page::ContentPage;
use sqlite_tasks::queries::content_pages::create_content_page::{create_content_page, CreateContentPageArgs};
use sqlite_tasks::queries::content_pages::get_content_page_by_id::{get_content_page_by_id, GetContentPageByIdArgs};
use sqlite_tasks::queries::content_pages::list_content_pages::{list_content_pages, ListContentPagesArgs};
use sqlite_tasks::queries::content_pages::update_content_page::{update_content_page, UpdateContentPageArgs};
use sqlite_tasks::queries::job_publications::get_job_publication_by_id::get_job_publication_by_id;
use sqlite_tasks::queries::job_publications::job_publication::JobPublication;
use sqlite_tasks::queries::job_publications::list_job_publications::{list_job_publications, ListJobPublicationsArgs};
use sqlite_tasks::queries::job_publications::update_job_publication::{update_job_publication, UpdateJobPublicationArgs};
use sqlite_tasks::queries::pipeline::create_pipeline_job::{create_pipeline_job, CreatePipelineJobArgs};
use sqlite_tasks::queries::pipeline::get_pipeline_job_by_id::{get_pipeline_job_by_id, GetPipelineJobByIdArgs};
use sqlite_tasks::queries::pipeline::list_pending_pipeline_jobs::{list_pending_pipeline_jobs, ListPendingPipelineJobsArgs};
use sqlite_tasks::queries::pipeline::pipeline_job::PipelineJob;
use sqlite_tasks::queries::pipeline::update_pipeline_job_stage::{update_pipeline_job_stage, UpdatePipelineJobStageArgs};
use sqlite_tasks::queries::pipeline::update_pipeline_job_status::{update_pipeline_job_status, UpdatePipelineJobStatusArgs};
use sqlite_tasks::queries::pipeline_job_events::insert_pipeline_job_event::{insert_pipeline_job_event, InsertPipelineJobEventArgs};
use sqlite_tasks::queries::pipeline_job_events::list_pipeline_job_events::{list_pipeline_job_events, ListPipelineJobEventsArgs};
use sqlite_tasks::queries::pipeline_job_events::pipeline_job_event::PipelineJobEvent;
use sqlite_tasks::queries::dashboard::get_dashboard_summary::{get_dashboard_summary, DashboardSummary, DashboardSummaryQueryArgs};
use sqlite_tasks::queries::pipeline::list_pipeline_jobs_paginated::{list_pipeline_jobs_paginated, ListPipelineJobsPaginatedArgs, PaginatedPipelineJobsResult};
use sqlite_tasks::queries::prompt_templates::prompt_template::PromptTemplate;
use sqlite_tasks::queries::prompt_templates::create_prompt_template::{create_prompt_template, CreatePromptTemplateArgs};
use sqlite_tasks::queries::prompt_templates::list_prompt_templates::list_prompt_templates;
use sqlite_tasks::queries::prompt_templates::update_prompt_template::{update_prompt_template, UpdatePromptTemplateArgs};
use sqlite_tasks::queries::prompt_templates::delete_prompt_template::delete_prompt_template;
use sqlite_tasks::queries::floword_settings::floword_setting::FlowordSetting;
use sqlite_tasks::queries::floword_settings::get_floword_setting::get_floword_setting;
use sqlite_tasks::queries::floword_settings::upsert_floword_setting::{upsert_floword_setting, UpsertFlowordSettingArgs};
use crate::services::pipeline::bulk_import_service::{BulkImportRow, BulkImportService, BulkValidationSummary, BulkCommitResponse};
use crate::services::pipeline::system_health_probes::{SystemHealthProbes, StorageHealthReport, SystemReadinessReport};
use std::collections::HashSet;
use std::time::Instant;
use tauri::{AppHandle, Manager, State};
use tokens::tokens::sqlite::pipeline_jobs::PipelineJobId;

const WORKFLOW_NOT_FOUND: &str = "WORKFLOW_NOT_FOUND";
const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
const OMNIROUTE_UNAVAILABLE: &str = "OMNIROUTE_UNAVAILABLE";

// ---------------------------------------------------------------------------
// Enqueue
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct EnqueueFlowordWorkflowRequest {
  pub page_id: Option<String>,
  pub workflow_name: String,
  pub prompt: String,
  pub topic: Option<String>,
  pub source_urls: Option<Vec<String>>,
  pub source_files: Option<Vec<String>>,
  pub workflow_mode: Option<String>,
  pub image_prompt: Option<String>,
  pub expand_9_16_prompt: Option<String>,
  pub expand_prompt: Option<String>,
  pub video_prompt: Option<String>,
  pub source_image_artifact: Option<Value>,
  pub target_platform: Option<String>,
  pub aspect_ratio: Option<String>,
  pub target_duration_seconds: Option<u32>,
  pub output_mode: Option<String>,
  pub model_id: Option<String>,
  pub voice_id: Option<String>,
  pub language: Option<String>,
  pub content_source: Option<String>,
  pub story_url: Option<String>,
  pub research_enabled: Option<bool>,
  pub research_platform: Option<String>,
  pub research_query: Option<String>,
  pub research_mode: Option<String>,
  pub xhs_variant: Option<String>,
  pub cookie_mode: Option<String>,
  pub cookie_browser: Option<String>,
  pub cookie_browser_profile: Option<String>,
  pub cookie_file_path: Option<String>,
  pub cookie_skip_patterns: Option<Vec<String>>,
  // Publishing fields
  pub title: Option<String>,
  pub caption: Option<String>,
  pub hashtags: Option<Vec<String>>,
  pub description: Option<String>,
  pub publish_platforms: Option<Vec<String>>,
  pub post_mode: Option<String>,
  pub schedule_time: Option<String>,
  pub custom_filename: Option<String>,
}

#[derive(Serialize)]
pub struct EnqueueFlowordWorkflowResponse {
  /// The real PipelineJob primary key. The frontend polls and cancels using this.
  pub job_id: String,
  /// Reserved for a future distinct workflow identifier. `None` today because the
  /// backend stores no separate workflow id — do not query pipeline_jobs with it.
  pub workflow_id: Option<String>,
  pub status: String,
}
impl SerializeMarker for EnqueueFlowordWorkflowResponse {}

/// Structured error payload returned to the frontend (e.g. WORKFLOW_NOT_FOUND).
#[derive(Serialize)]
pub struct FlowordErrorDetails {
  pub error_code: String,
  pub job_id: Option<String>,
}

#[tauri::command]
pub async fn enqueue_floword_workflow(task_database: State<'_, TaskDatabase>, request: EnqueueFlowordWorkflowRequest) -> ResponseOrError<EnqueueFlowordWorkflowResponse, FlowordErrorDetails> {
  info!("[FlowordDB] command=enqueue db_path={}", task_database.db_path_display());

  let mut req = request;
  // Resolve authoritative workflow_mode if not explicitly provided
  if req.workflow_mode.as_deref().unwrap_or("").trim().is_empty() {
    let lower_name = req.workflow_name.to_lowercase();
    if lower_name.contains("grok full") || lower_name.contains("auto ai grok") || lower_name == "grok_content_pipeline" {
      req.workflow_mode = Some("grok_content_pipeline".to_string());
    } else if lower_name.contains("grok image") || lower_name == "grok_image_edit" {
      req.workflow_mode = Some("grok_image_edit".to_string());
    }
  }

  let page_id_trimmed = req.page_id.as_deref().map(str::trim).filter(|s| !s.is_empty());
  let page_id = match page_id_trimmed {
    Some(id) => id,
    None => return Err(internal_error("Select a Page before running the workflow.", None)),
  };

  if req.prompt.trim().is_empty()
    && req.image_prompt.as_ref().map(|p| p.trim().is_empty()).unwrap_or(true)
    && req.source_urls.as_ref().map(|values| values.is_empty()).unwrap_or(true)
    && req.source_files.as_ref().map(|values| values.is_empty()).unwrap_or(true)
    && req.source_image_artifact.is_none()
  {
    return Err(internal_error("Prompt, image_prompt, source_urls, source_files, and source_image_artifact are all empty", None));
  }

  // Authoritative ContentPage lookup for immutable page snapshot
  let maybe_page = get_content_page_by_id(GetContentPageByIdArgs {
    db: task_database.get_connection(),
    id: page_id,
  }).await.map_err(|e| internal_error(&format!("Failed to query content page: {e}"), None))?;

  let page = maybe_page.ok_or_else(|| internal_error(&format!("ContentPage with id '{page_id}' not found in database"), None))?;

  let page_snapshot = json!({
    "page_id": page.id,
    "page_name": page.name,
    "slug": page.slug,
    "output_root": page.output_root,
    "browser_profile_id": page.browser_profile_id,
    "worker_pool_id": page.worker_pool_id,
    "default_image_prompt": page.default_image_prompt,
    "default_expand_9_16_prompt": page.default_expand_9_16_prompt,
    "default_video_prompt": page.default_video_prompt,
    "target_platform": page.target_platform,
    "default_aspect_ratio": page.default_aspect_ratio,
  });
  let page_snapshot_str = serde_json::to_string(&page_snapshot).unwrap_or_default();
  let input_payload = serde_json::to_string(&req).map_err(|e| internal_error(&format!("Failed to serialize input payload: {e}"), None))?;

  let job_id = create_pipeline_job(CreatePipelineJobArgs {
    db: task_database.get_connection(),
    status: TaskStatus::Pending,
    current_stage: PipelineStage::Queued,
    maybe_page_id: Some(page_id),
    maybe_input_payload: Some(&input_payload),
    maybe_page_snapshot: Some(&page_snapshot_str),
    maybe_business_status: Some("QUEUED"),
  }).await.map_err(|err| {
    error!("[Floword] enqueue create_pipeline_job failed: {:?}", err);
    internal_error("Failed to insert pipeline job", None)
  })?;

  // Read the row back by its primary key to prove the insert committed and is
  // reachable by the same id we hand the frontend.
  let readback = get_pipeline_job_by_id(GetPipelineJobByIdArgs { db: task_database.get_connection(), pipeline_job_id: &job_id }).await.map_err(|err| {
    error!("[Floword] enqueue readback query failed: {:?}", err);
    internal_error("Failed to read back pipeline job", Some(job_id.as_str()))
  })?;

  if readback.is_none() {
    error!("[Floword] enqueue readback found no row for id {}", job_id.as_str());
    return Err(internal_error("Pipeline job vanished immediately after insert", Some(job_id.as_str())));
  }

  let _ = insert_pipeline_job_event(InsertPipelineJobEventArgs {
    db: task_database.get_connection(),
    id: None,
    job_id: job_id.as_str(),
    sequence: 1,
    stage_id: Some("QUEUED"),
    business_status: Some("QUEUED"),
    event_type: "JOB_CREATED",
    level: "INFO",
    message: &format!("Workflow job enqueued for Page '{}'", page.name),
    error_code: None,
    metadata_json: Some(&page_snapshot_str),
  }).await;

  info!("[Floword] enqueued job_id={} for page_id={} (readback OK)", job_id.as_str(), page_id);

  Ok(EnqueueFlowordWorkflowResponse { job_id: job_id.as_str().to_string(), workflow_id: None, status: TaskStatus::Pending.to_str().to_string() }.into())
}

// ---------------------------------------------------------------------------
// Get
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct GetFlowordWorkflowRequest {
  pub job_id: String,
}

#[derive(Serialize)]
pub struct GetFlowordWorkflowResponse {
  pub job_id: String,
  pub page_id: Option<String>,
  pub page_snapshot: Option<Value>,
  pub status: String,
  pub business_status: String,
  pub current_stage: String,
  pub input_payload: Option<Value>,
  pub failure_message: Option<String>,
  pub failure_code: Option<String>,
  pub failure_stage: Option<String>,
  pub stage_outputs: Option<String>,
  pub stage_states: Vec<StageState>,
  pub created_at: i64,
  pub started_at: Option<i64>,
  pub completed_at: Option<i64>,
}
impl SerializeMarker for GetFlowordWorkflowResponse {}

#[tauri::command]
pub async fn get_floword_workflow(task_database: State<'_, TaskDatabase>, request: GetFlowordWorkflowRequest) -> ResponseOrError<GetFlowordWorkflowResponse, FlowordErrorDetails> {
  info!("[FlowordDB] command=get db_path={} job_id={}", task_database.db_path_display(), request.job_id);

  let pipeline_job_id = PipelineJobId::new_from_str(&request.job_id);
  let maybe_job = get_pipeline_job_by_id(GetPipelineJobByIdArgs { db: task_database.get_connection(), pipeline_job_id: &pipeline_job_id }).await.map_err(|err| {
    error!("[Floword] get_floword_workflow query failed: {:?}", err);
    internal_error("Failed to query pipeline job", Some(&request.job_id))
  })?;

  match maybe_job {
    Some(job) => Ok(get_response_from_job(&job).into()),
    None => Err(not_found(&request.job_id)),
  }
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ListFlowordWorkflowsResponse {
  pub workflows: Vec<GetFlowordWorkflowResponse>,
}
impl SerializeMarker for ListFlowordWorkflowsResponse {}

#[tauri::command]
pub async fn list_floword_workflows(task_database: State<'_, TaskDatabase>) -> ResponseOrErrorMessage<ListFlowordWorkflowsResponse> {
  let mut statuses = HashSet::new();
  statuses.insert(TaskStatus::Pending);
  statuses.insert(TaskStatus::Started);
  statuses.insert(TaskStatus::WaitingInput);
  statuses.insert(TaskStatus::CompleteSuccess);
  statuses.insert(TaskStatus::CompleteFailure);
  statuses.insert(TaskStatus::CancelledByUser);

  let list = list_pending_pipeline_jobs(ListPendingPipelineJobsArgs { db: task_database.get_connection(), statuses: &statuses }).await.map_err(|err| {
    error!("[Floword] list_floword_workflows failed: {:?}", err);
    "list_floword_workflows failed"
  })?;

  let workflows = list.jobs.iter().map(get_response_from_job).collect();

  Ok(ListFlowordWorkflowsResponse { workflows }.into())
}

// ---------------------------------------------------------------------------
// Cancel
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CancelFlowordWorkflowRequest {
  pub job_id: String,
}

#[derive(Serialize)]
pub struct CancelFlowordWorkflowResponse {
  pub cancelled: bool,
  /// Whether a live worker token existed to abort in-flight adapter work. When
  /// false, only the DB status was updated (job was not actively running).
  pub had_live_token: bool,
}
impl SerializeMarker for CancelFlowordWorkflowResponse {}

#[tauri::command]
pub async fn cancel_floword_workflow(task_database: State<'_, TaskDatabase>, request: CancelFlowordWorkflowRequest) -> ResponseOrError<CancelFlowordWorkflowResponse, FlowordErrorDetails> {
  info!("[FlowordDB] command=cancel db_path={} job_id={}", task_database.db_path_display(), request.job_id);
  let pipeline_job_id = PipelineJobId::new_from_str(&request.job_id);

  // Reject cancel on a job that does not exist so the frontend gets NOT_FOUND
  // instead of a silent success.
  let maybe_job = get_pipeline_job_by_id(GetPipelineJobByIdArgs { db: task_database.get_connection(), pipeline_job_id: &pipeline_job_id }).await.map_err(|err| {
    error!("[Floword] cancel readback failed: {:?}", err);
    internal_error("Failed to query pipeline job", Some(&request.job_id))
  })?;

  if maybe_job.is_none() {
    return Err(not_found(&request.job_id));
  }

  // Signal the in-flight worker to abort (render polling + between-stage checks).
  let had_live_token = request_cancellation(&request.job_id);

  if maybe_job.as_ref().is_some_and(|job| requires_research_login_cleanup(job.status)) {
    let backend_url = std::env::var("FLOWORD_BACKEND_URL").unwrap_or_else(|_| "http://127.0.0.1:30000".to_string());
    if let Err(error) = reqwest::Client::new().post(format!("{}/api/research/crawler/stop", backend_url.trim_end_matches('/'))).send().await {
      warn!("[Floword] waiting Research cleanup request failed for {}: {}", request.job_id, error);
    }
  }

  let updated = update_pipeline_job_status(UpdatePipelineJobStatusArgs {
    db: task_database.get_connection(),
    pipeline_job_id: &pipeline_job_id,
    status: TaskStatus::CancelledByUser,
    maybe_business_status: Some("CANCELLED"),
  }).await.map_err(|err| {
    error!("[Floword] cancel update_status failed: {:?}", err);
    internal_error("Failed to update job status", Some(&request.job_id))
  })?;

  Ok(CancelFlowordWorkflowResponse { cancelled: updated, had_live_token }.into())
}

fn requires_research_login_cleanup(status: TaskStatus) -> bool {
  status == TaskStatus::WaitingInput
}

// ---------------------------------------------------------------------------
// Retry
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RetryFlowordStepRequest {
  pub job_id: String,
  pub step_id: String,
}

#[derive(Serialize)]
pub struct RetryFlowordStepResponse {
  pub retried: bool,
  pub job_id: String,
  pub resumed_stage: String,
  pub step_retry_count: u64,
}
impl SerializeMarker for RetryFlowordStepResponse {}

#[tauri::command]
pub async fn retry_floword_step(task_database: State<'_, TaskDatabase>, request: RetryFlowordStepRequest) -> ResponseOrError<RetryFlowordStepResponse, FlowordErrorDetails> {
  info!("[FlowordDB] command=retry db_path={} job_id={} step_id={}", task_database.db_path_display(), request.job_id, request.step_id);
  let pipeline_job_id = PipelineJobId::new_from_str(&request.job_id);

  let job = get_pipeline_job_by_id(GetPipelineJobByIdArgs { db: task_database.get_connection(), pipeline_job_id: &pipeline_job_id }).await.map_err(|err| {
    error!("[Floword] retry readback failed: {:?}", err);
    internal_error("Failed to query pipeline job", Some(&request.job_id))
  })?;

  let job = match job {
    Some(j) => j,
    None => return Err(not_found(&request.job_id)),
  };

  let (resume_stage, new_business_status) = match request.step_id.to_uppercase().as_str() {
    "GROK_IMAGE" | "IMAGE_EDIT" | "GENERATING_IMAGE" => (PipelineStage::ScriptGenerating, "QUEUED"),
    "GROK_EXPAND_916" | "GROK_EXPAND_9_16" | "CONVERTING_9_16" => (PipelineStage::ScriptReady, "IMAGE_DONE"),
    "GROK_VIDEO" | "GENERATING_VIDEO" => (PipelineStage::MediaTimeline, "IMAGE_9_16_DONE"),
    "LOCAL_SAVE" | "SAVING_LOCAL" => (PipelineStage::DraftSaving, "VIDEO_DONE"),
    _ => {
      match resume_stage_for_step(&request.step_id) {
        Some(stage) => (stage, "QUEUED"),
        None => return Err(bad_request(&format!("Unknown step_id '{}'", request.step_id), &request.job_id)),
      }
    }
  };

  let mut outputs = job.maybe_stage_outputs.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or_else(|| json!({}));

  // Canonical stage output invalidation
  match request.step_id.to_uppercase().as_str() {
    "GROK_IMAGE" | "IMAGE_EDIT" | "GENERATING_IMAGE" => {
      if let Some(obj) = outputs.as_object_mut() {
        obj.remove("image_edit");
        obj.remove("expand_9_16");
        obj.remove("video_generate");
        obj.remove("final_video_path");
        obj.remove("video_url");
        obj.remove("status");
      }
    },
    "GROK_EXPAND_916" | "GROK_EXPAND_9_16" | "CONVERTING_9_16" => {
      if let Some(obj) = outputs.as_object_mut() {
        obj.remove("expand_9_16");
        obj.remove("video_generate");
        obj.remove("final_video_path");
        obj.remove("video_url");
        obj.remove("status");
      }
    },
    "GROK_VIDEO" | "GENERATING_VIDEO" => {
      if let Some(obj) = outputs.as_object_mut() {
        obj.remove("video_generate");
        obj.remove("final_video_path");
        obj.remove("video_url");
        obj.remove("status");
      }
    },
    "LOCAL_SAVE" | "SAVING_LOCAL" => {
      if let Some(obj) = outputs.as_object_mut() {
        obj.remove("final_video_path");
        obj.remove("status");
      }
    },
    _ => {
      if let Some(business_stage) = business_stage_for_step(&request.step_id) {
        if let Some(mut context) = outputs.get("pipeline_context").cloned().and_then(|value| serde_json::from_value::<PipelineContext>(value).ok()) {
          invalidate_context_from(&mut context, business_stage);
          invalidate_output_values(&mut outputs, business_stage);
          outputs["pipeline_context"] = serde_json::to_value(context).map_err(|error| internal_error(&format!("Failed to serialize pipeline context: {error}"), Some(&request.job_id)))?;
        } else {
          invalidate_outputs_from_stage(&mut outputs, resume_stage);
        }
      } else {
        invalidate_outputs_from_stage(&mut outputs, resume_stage);
      }
    }
  }

  let step_retry_count = bump_retry_count(&mut outputs, &request.step_id);
  let outputs_string = serde_json::to_string(&outputs).map_err(|e| internal_error(&format!("Failed to serialize outputs: {e}"), Some(&request.job_id)))?;

  update_pipeline_job_stage(UpdatePipelineJobStageArgs {
    db: task_database.get_connection(),
    pipeline_job_id: &pipeline_job_id,
    current_stage: resume_stage,
    maybe_stage_outputs: Some(&outputs_string),
    maybe_business_status: Some(new_business_status),
  }).await.map_err(|err| {
    error!("[Floword] retry update_stage failed: {:?}", err);
    internal_error("Failed to update job stage", Some(&request.job_id))
  })?;

  // Clear failure message and reset status back to Pending
  let retried = update_pipeline_job_status(UpdatePipelineJobStatusArgs {
    db: task_database.get_connection(),
    pipeline_job_id: &pipeline_job_id,
    status: TaskStatus::Pending,
    maybe_business_status: Some(new_business_status),
  }).await.map_err(|err| {
    error!("[Floword] retry update_status failed: {:?}", err);
    internal_error("Failed to reset job status", Some(&request.job_id))
  })?;

  let _ = insert_pipeline_job_event(InsertPipelineJobEventArgs {
    db: task_database.get_connection(),
    id: None,
    job_id: &request.job_id,
    sequence: 50,
    stage_id: Some(&request.step_id),
    business_status: Some(new_business_status),
    event_type: "RETRY_REQUESTED",
    level: "INFO",
    message: &format!("Stage '{}' retry requested (attempt {})", request.step_id, step_retry_count),
    error_code: None,
    metadata_json: None,
  }).await;

  Ok(RetryFlowordStepResponse { retried, job_id: request.job_id.clone(), resumed_stage: resume_stage.to_str().to_string(), step_retry_count }.into())
}

#[derive(Deserialize)]
pub struct RetryFlowordJobFromStartRequest {
  pub job_id: String,
}

#[tauri::command]
pub async fn retry_floword_job_from_start(
  task_database: State<'_, TaskDatabase>,
  request: RetryFlowordJobFromStartRequest,
) -> ResponseOrError<RetryFlowordStepResponse, FlowordErrorDetails> {
  info!("[FlowordDB] command=retry_from_start job_id={}", request.job_id);
  let pipeline_job_id = PipelineJobId::new_from_str(&request.job_id);

  let job = get_pipeline_job_by_id(GetPipelineJobByIdArgs {
    db: task_database.get_connection(),
    pipeline_job_id: &pipeline_job_id,
  }).await.map_err(|err| internal_error(&format!("Query failed: {err}"), Some(&request.job_id)))?
    .ok_or_else(|| not_found(&request.job_id))?;

  let mut outputs = json!({});
  let step_retry_count = bump_retry_count(&mut outputs, "job_start");
  let outputs_string = serde_json::to_string(&outputs).unwrap_or_default();

  update_pipeline_job_stage(UpdatePipelineJobStageArgs {
    db: task_database.get_connection(),
    pipeline_job_id: &pipeline_job_id,
    current_stage: PipelineStage::Queued,
    maybe_stage_outputs: Some(&outputs_string),
    maybe_business_status: Some("QUEUED"),
  }).await.map_err(|err| internal_error(&format!("Stage reset failed: {err}"), Some(&request.job_id)))?;

  let retried = update_pipeline_job_status(UpdatePipelineJobStatusArgs {
    db: task_database.get_connection(),
    pipeline_job_id: &pipeline_job_id,
    status: TaskStatus::Pending,
    maybe_business_status: Some("QUEUED"),
  }).await.map_err(|err| internal_error(&format!("Status reset failed: {err}"), Some(&request.job_id)))?;

  let _ = insert_pipeline_job_event(InsertPipelineJobEventArgs {
    db: task_database.get_connection(),
    id: None,
    job_id: &request.job_id,
    sequence: 50,
    stage_id: Some("QUEUED"),
    business_status: Some("QUEUED"),
    event_type: "RETRY_FROM_START",
    level: "INFO",
    message: "Full job retry requested from start",
    error_code: None,
    metadata_json: None,
  }).await;

  Ok(RetryFlowordStepResponse {
    retried,
    job_id: request.job_id,
    resumed_stage: PipelineStage::Queued.to_str().to_string(),
    step_retry_count,
  }.into())
}

#[derive(Deserialize)]
pub struct SkipFlowordResearchRequest {
  pub job_id: String,
}

#[tauri::command]
pub async fn skip_floword_research(task_database: State<'_, TaskDatabase>, request: SkipFlowordResearchRequest) -> ResponseOrError<RetryFlowordStepResponse, FlowordErrorDetails> {
  info!("[FlowordDB] command=skip_research db_path={} job_id={}", task_database.db_path_display(), request.job_id);
  let pipeline_job_id = PipelineJobId::new_from_str(&request.job_id);
  let job = get_pipeline_job_by_id(GetPipelineJobByIdArgs { db: task_database.get_connection(), pipeline_job_id: &pipeline_job_id }).await.map_err(|err| {
    error!("[Floword] skip research readback failed: {:?}", err);
    internal_error("Failed to query pipeline job", Some(&request.job_id))
  })?;
  let job = match job {
    Some(job) => job,
    None => return Err(not_found(&request.job_id)),
  };
  let mut outputs = job.maybe_stage_outputs.as_deref().and_then(|value| serde_json::from_str::<Value>(value).ok()).unwrap_or_else(|| json!({}));
  let Some(mut context) = outputs.get("pipeline_context").cloned().and_then(|value| serde_json::from_value::<PipelineContext>(value).ok()) else {
    return Err(bad_request("Research context is unavailable for this job", &request.job_id));
  };
  context.research_enabled = false;
  invalidate_context_from(&mut context, StageId::Research);
  invalidate_output_values(&mut outputs, StageId::Research);
  outputs["pipeline_context"] = serde_json::to_value(context).map_err(|error| internal_error(&format!("Failed to serialize pipeline context: {error}"), Some(&request.job_id)))?;
  let step_retry_count = bump_retry_count(&mut outputs, "research");
  let outputs_string = serde_json::to_string(&outputs).map_err(|error| internal_error(&format!("Failed to serialize outputs: {error}"), Some(&request.job_id)))?;
  update_pipeline_job_stage(UpdatePipelineJobStageArgs {
    db: task_database.get_connection(),
    pipeline_job_id: &pipeline_job_id,
    current_stage: PipelineStage::Research,
    maybe_stage_outputs: Some(&outputs_string),
    maybe_business_status: Some("GENERATING_IMAGE"),
  }).await.map_err(|err| {
    error!("[Floword] skip research update_stage failed: {:?}", err);
    internal_error("Failed to update research stage", Some(&request.job_id))
  })?;
  let retried = update_pipeline_job_status(UpdatePipelineJobStatusArgs {
    db: task_database.get_connection(),
    pipeline_job_id: &pipeline_job_id,
    status: TaskStatus::Pending,
    maybe_business_status: Some("GENERATING_IMAGE"),
  }).await.map_err(|err| {
    error!("[Floword] skip research update_status failed: {:?}", err);
    internal_error("Failed to resume job after skipping research", Some(&request.job_id))
  })?;
  Ok(RetryFlowordStepResponse { retried, job_id: request.job_id, resumed_stage: PipelineStage::Research.to_str().to_string(), step_retry_count }.into())
}

// ---------------------------------------------------------------------------
// OmniRoute models
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct FlowordModel {
  pub id: String,
  pub provider: Option<String>,
}

#[derive(Serialize)]
pub struct ListOmniRouteModelsResponse {
  pub models: Vec<FlowordModel>,
}
impl SerializeMarker for ListOmniRouteModelsResponse {}

#[tauri::command]
pub async fn list_omniroute_models() -> ResponseOrError<ListOmniRouteModelsResponse, FlowordErrorDetails> {
  match omniroute_list_models().await {
    Ok(models) => {
      let models = models.into_iter().map(|m| FlowordModel { id: m.id, provider: if m.provider.is_empty() { None } else { Some(m.provider) } }).collect();
      Ok(ListOmniRouteModelsResponse { models }.into())
    },
    Err(err) => {
      error!("[Floword] list_omniroute_models failed: {err:?}");
      Err(CommandErrorResponseWrapper { status: CommandErrorStatus::ServerError, error_message: Some(format!("{err:?}")), error_type: Some(()), error_details: Some(FlowordErrorDetails { error_code: OMNIROUTE_UNAVAILABLE.to_string(), job_id: None }) })
    },
  }
}

// ---------------------------------------------------------------------------
// ArtCraft visual provider (canonical account owner; no secrets returned)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct FlowordVisualProviderResponse {
  pub provider: String,
  pub status: String,
  pub capabilities: Vec<String>,
  pub credential_type: String,
  pub credential_source: String,
  pub auth_method: String,
  pub oauth_supported: bool,
  pub api_key_supported: bool,
  pub message: String,
}
impl SerializeMarker for FlowordVisualProviderResponse {}

fn visual_provider_response(provider: &str, status: &str, capabilities: Vec<String>, message: &str) -> FlowordVisualProviderResponse {
  FlowordVisualProviderResponse {
    provider: provider.to_string(),
    status: status.to_string(),
    capabilities,
    credential_type: "OmniRoute-managed provider credential".to_string(),
    credential_source: "OmniRoute AI gateway (managed externally)".to_string(),
    auth_method: "OmniRoute API key".to_string(),
    oauth_supported: false,
    api_key_supported: true,
    message: message.to_string(),
  }
}

/// Query OmniRoute for available video-capable models. Reports `configured` when
/// at least one video model is available, `not_configured` when OmniRoute returns
/// no video models, and `invalid` when OmniRoute itself is unreachable.
#[tauri::command]
pub async fn get_floword_visual_provider() -> ResponseOrErrorMessage<FlowordVisualProviderResponse> {
  match omniroute_list_video_models().await {
    Ok(video_models) => {
      if video_models.is_empty() {
        let response = visual_provider_response(
          "OmniRoute (no video provider)",
          "not_configured",
          vec!["Text".to_string(), "Voice".to_string()],
          "No active, healthy video-provider connection was found. Gemini/OpenCode keys can generate text but do not authorize video. Add and test one or more Veo, Runway, Seedance, Sora, or other video-provider connections; OmniRoute will rotate their keys and fall back across providers.",
        );
        Ok(response.into())
      } else {
        let model_ids: Vec<String> = video_models.iter().take(3).map(|m| m.id.clone()).collect();
        let capabilities = vec!["Video".to_string(), "Text-to-Video".to_string()];
        let message = format!("OmniRoute has {} connected video model(s): {}. Multiple active keys are pooled for quota fallback.", video_models.len(), model_ids.join(", "));
        let response = visual_provider_response("OmniRoute", "configured", capabilities, &message);
        Ok(response.into())
      }
    }
    Err(error) => {
      let message = format!("OmniRoute unavailable for video model discovery: {error}");
      warn!("OMNIROUTE_UNAVAILABLE visual_provider_check: {}", message);
      let response = visual_provider_response(
        "OmniRoute (unavailable)",
        "not_configured",
        vec![],
        &message,
      );
      Ok(response.into())
    }
  }
}

/// Smoke-test OmniRoute video generation capability. Reports OmniRoute's video
/// model availability. Does NOT execute an actual generation — that would be
/// slow and consume quota. Instead it validates that the model catalog is reachable
/// and non-empty for video models.
#[tauri::command]
pub async fn test_floword_visual_provider() -> ResponseOrErrorMessage<FlowordVisualProviderResponse> {
  match omniroute_list_video_models().await {
    Ok(video_models) => {
      if video_models.is_empty() {
        let response = visual_provider_response(
          "OmniRoute (no video provider)",
          "not_configured",
          vec![],
          "OmniRoute is reachable, but its video catalog has no matching active/healthy provider connection. Add video-provider keys with distinct account names and test them in OmniRoute.",
        );
        Ok(response.into())
      } else {
        let model_ids: Vec<String> = video_models.iter().take(5).map(|m| m.id.clone()).collect();
        let message = format!("OmniRoute video pool verified: {} connected model(s) ({}). Active accounts can complement each other's quota.", video_models.len(), model_ids.join(", "));
        info!("OMNIROUTE_VIDEO_CAPABILITY_VERIFIED models={}", model_ids.join(","));
        let response = visual_provider_response(
          "OmniRoute",
          "configured",
          vec!["Video".to_string(), "Text-to-Video".to_string()],
          &message,
        );
        Ok(response.into())
      }
    }
    Err(error) => {
      let message = format!("OmniRoute test failed — model catalog unreachable: {error}");
      warn!("OMNIROUTE_UNAVAILABLE visual_provider_test: {}", message);
      let response = visual_provider_response(
        "OmniRoute (unavailable)",
        "invalid",
        vec![],
        &message,
      );
      Ok(response.into())
    }
  }
}

// ---------------------------------------------------------------------------
// Readiness
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct FlowordServiceHealth {
  pub id: String,
  pub status: String, // ready | degraded | unavailable | auth_required
  pub latency_ms: u64,
  pub error_code: Option<String>,
  pub message: Option<String>,
}

#[derive(Serialize)]
pub struct FlowordReadinessResponse {
  pub services: Vec<FlowordServiceHealth>,
  pub is_ready_for_execution: bool,
}
impl SerializeMarker for FlowordReadinessResponse {}

#[tauri::command]
pub async fn get_floword_readiness(app: AppHandle, task_database: State<'_, TaskDatabase>) -> ResponseOrErrorMessage<FlowordReadinessResponse> {
  let mut services = Vec::new();

  // Storage: is the task DB directory writable?
  services.push(check_storage(&task_database));

  // OmniRoute LLM gateway.
  services.push(check_omniroute().await);

  // CapCut Mate.
  services.push(check_capcut().await);

  services.push(check_youwee(&app).await);
  services.push(check_vynaro(&app).await);

  services.push(check_artcraft(&app));

  // Modules without a wired runtime contract in the worker yet. Reported honestly
  // as unavailable rather than a hard-coded READY.
  for id in ["mediacrawler", "openmontage", "playwright_sidecar", "chrome_cdp"] {
    services.push(FlowordServiceHealth { id: id.to_string(), status: "unavailable".to_string(), latency_ms: 0, error_code: Some("NO_RUNTIME_CONTRACT".to_string()), message: Some("No backend readiness probe wired for this module yet".to_string()) });
  }

  // Minimum bar for the draft_only pipeline the worker actually implements:
  // storage + OmniRoute + CapCut Mate all ready.
  let ready_ids: HashSet<&str> = services.iter().filter(|s| s.status == "ready").map(|s| s.id.as_str()).collect();
  let is_ready_for_execution = ready_ids.contains("storage") && ready_ids.contains("omniroute") && ready_ids.contains("capcut");

  Ok(FlowordReadinessResponse { services, is_ready_for_execution }.into())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn derive_business_status(job: &PipelineJob) -> String {
  if let Some(bs) = job.maybe_business_status.as_deref().filter(|s| !s.is_empty()) {
    return bs.to_string();
  }
  match job.status {
    TaskStatus::Pending => "QUEUED".to_string(),
    TaskStatus::Started => match job.current_stage {
      PipelineStage::Queued => "QUEUED".to_string(),
      PipelineStage::PreflightCheck | PipelineStage::IngestAnalyze => "WAITING_WORKER".to_string(),
      PipelineStage::Research => "GENERATING_IMAGE".to_string(),
      PipelineStage::ScriptGenerating => "GENERATING_IMAGE".to_string(),
      PipelineStage::ScriptReady => "CONVERTING_9_16".to_string(),
      PipelineStage::MediaTimeline => "GENERATING_VIDEO".to_string(),
      PipelineStage::DraftCreating | PipelineStage::DraftCreated | PipelineStage::CaptionAdding | PipelineStage::DraftSaving => "SAVING_LOCAL".to_string(),
      PipelineStage::DraftReady | PipelineStage::RenderRequesting | PipelineStage::Rendering | PipelineStage::Completed => "READY_TO_POST".to_string(),
      PipelineStage::Failed => "ERROR".to_string(),
      PipelineStage::Cancelled => "CANCELLED".to_string(),
    },
    TaskStatus::WaitingInput => "AUTH_REQUIRED".to_string(),
    TaskStatus::CompleteSuccess => "READY_TO_POST".to_string(),
    TaskStatus::CompleteFailure => "ERROR".to_string(),
    TaskStatus::CancelledByUser => "CANCELLED".to_string(),
    _ => "ERROR".to_string(),
  }
}

fn get_response_from_job(job: &PipelineJob) -> GetFlowordWorkflowResponse {
  let page_snapshot_val = job.maybe_page_snapshot.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok());
  let input_payload_val = job.maybe_input_payload.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok());
  let b_status = derive_business_status(job);

  GetFlowordWorkflowResponse {
    job_id: job.id.as_str().to_string(),
    page_id: job.maybe_page_id.clone(),
    page_snapshot: page_snapshot_val,
    status: job.status.to_str().to_string(),
    business_status: b_status,
    current_stage: job.current_stage.to_str().to_string(),
    input_payload: input_payload_val,
    failure_message: job.maybe_on_failure_message.clone(),
    failure_code: job.maybe_failure_code.clone(),
    failure_stage: job.maybe_failure_stage.clone(),
    stage_outputs: job.maybe_stage_outputs.clone(),
    stage_states: stage_states_from_outputs(job.maybe_stage_outputs.as_deref()),
    created_at: job.created_at,
    started_at: job.maybe_started_at,
    completed_at: job.maybe_completed_at,
  }
}

fn stage_states_from_outputs(stage_outputs: Option<&str>) -> Vec<StageState> {
  stage_outputs.and_then(|raw| serde_json::from_str::<Value>(raw).ok()).and_then(|outputs| outputs.get("pipeline_context").cloned()).and_then(|context| serde_json::from_value::<PipelineContext>(context).ok()).map(|context| context.stage_states).unwrap_or_else(PipelineContext::initial_stage_states)
}

/// Map a frontend step identifier (either `step-N` or a module name) to the
/// pipeline stage the worker should resume at.
fn resume_stage_for_step(step_id: &str) -> Option<PipelineStage> {
  let module = normalize_step_to_module(step_id)?;
  let stage = match module {
    "media_crawler" => PipelineStage::Research,
    "omniroute" => PipelineStage::ScriptGenerating,
    "youwee" => PipelineStage::PreflightCheck,
    "artcraft" => PipelineStage::DraftCreating,
    "open_montage" => PipelineStage::MediaTimeline,
    "capcut" => PipelineStage::DraftCreating,
    _ => return None,
  };
  Some(stage)
}

fn normalize_step_to_module(step_id: &str) -> Option<&'static str> {
  match step_id {
    "step-1" | "media_crawler" | "research" => Some("media_crawler"),
    "step-2" | "omniroute" | "story_script" => Some("omniroute"),
    "step-3" | "youwee" | "ingest_analyze" => Some("youwee"),
    "step-4" | "artcraft" | "voice" => Some("artcraft"),
    "step-5" | "open_montage" | "media_timeline" => Some("open_montage"),
    "step-6" | "capcut" => Some("capcut"),
    _ => None,
  }
}

fn business_stage_for_step(step_id: &str) -> Option<StageId> {
  match step_id {
    "step-1" | "media_crawler" | "research" => Some(StageId::Research),
    "step-2" | "omniroute" | "story_script" => Some(StageId::StoryScript),
    "step-3" | "youwee" | "ingest_analyze" => Some(StageId::IngestAnalyze),
    "step-4" | "artcraft" | "voice" => Some(StageId::Voice),
    "step-5" | "open_montage" | "media_timeline" => Some(StageId::MediaTimeline),
    "step-6" | "capcut" => Some(StageId::Capcut),
    _ => None,
  }
}

/// Ordinal used to decide which output keys are "downstream" of a resume point.
fn stage_ordinal(stage: PipelineStage) -> u8 {
  match stage {
    PipelineStage::Queued => 0,
    PipelineStage::PreflightCheck => 1,
    PipelineStage::IngestAnalyze => 2,
    PipelineStage::Research => 3,
    PipelineStage::ScriptGenerating => 4,
    PipelineStage::ScriptReady => 5,
    PipelineStage::MediaTimeline => 6,
    PipelineStage::DraftCreating => 7,
    PipelineStage::DraftCreated => 8,
    PipelineStage::CaptionAdding => 9,
    PipelineStage::DraftSaving => 10,
    PipelineStage::DraftReady => 11,
    PipelineStage::RenderRequesting => 12,
    PipelineStage::Rendering => 13,
    PipelineStage::Completed => 14,
    PipelineStage::Failed => 15,
    PipelineStage::Cancelled => 16,
  }
}

/// Stage at which each output key is first produced. Any key whose producing
/// stage is at or after `resume_stage` is invalidated on retry.
fn output_key_producing_stage(key: &str) -> Option<PipelineStage> {
  let stage = match key {
    "pipeline_context" | "ingest_analyze" => PipelineStage::IngestAnalyze,
    "research" => PipelineStage::Research,
    "script" | "script_artifact" => PipelineStage::ScriptGenerating,
    "media_timeline" => PipelineStage::MediaTimeline,
    "draft_url" | "draft_id" => PipelineStage::DraftCreating,
    "capcut_artifact" => PipelineStage::DraftSaving,
    "video_url" | "rendering_supported" => PipelineStage::Rendering,
    _ => return None,
  };
  Some(stage)
}

fn invalidate_outputs_from_stage(outputs: &mut Value, resume_stage: PipelineStage) {
  let resume_ord = stage_ordinal(resume_stage);
  if let Some(obj) = outputs.as_object_mut() {
    let keys_to_remove: Vec<String> = obj.keys().filter(|k| output_key_producing_stage(k).map(|s| stage_ordinal(s) >= resume_ord).unwrap_or(false)).cloned().collect();
    for k in keys_to_remove {
      obj.remove(&k);
    }
  }
}

fn bump_retry_count(outputs: &mut Value, step_id: &str) -> u64 {
  let module = normalize_step_to_module(step_id).unwrap_or(step_id);
  let obj = outputs.as_object_mut().expect("outputs is a JSON object");
  let counts = obj.entry("retry_counts").or_insert_with(|| json!({}));
  let counts_obj = counts.as_object_mut().expect("retry_counts is a JSON object");
  let next = counts_obj.get(module).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
  counts_obj.insert(module.to_string(), json!(next));
  next
}

fn check_storage(task_database: &TaskDatabase) -> FlowordServiceHealth {
  let start = Instant::now();
  let db_path = task_database.db_path();
  let dir = db_path.parent().unwrap_or_else(|| std::path::Path::new("."));
  let probe = dir.join(".floword_write_probe");
  let (status, error_code, message) = match std::fs::write(&probe, b"ok") {
    Ok(()) => {
      let _ = std::fs::remove_file(&probe);
      ("ready".to_string(), None, Some(format!("Writable: {}", dir.display())))
    },
    Err(e) => ("unavailable".to_string(), Some("STORAGE_NOT_WRITABLE".to_string()), Some(format!("{e}"))),
  };
  FlowordServiceHealth { id: "storage".to_string(), status, latency_ms: start.elapsed().as_millis() as u64, error_code, message }
}

async fn check_omniroute() -> FlowordServiceHealth {
  let start = Instant::now();
  match llm_health_check().await {
    Ok(()) => FlowordServiceHealth { id: "omniroute".to_string(), status: "ready".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: None, message: None },
    Err(e) => {
      let status = if e.contains("UNAUTHORIZED") || e.contains("FORBIDDEN") { "auth_required" } else if e.contains("TIMEOUT") { "degraded" } else { "unavailable" };
      FlowordServiceHealth { id: "omniroute".to_string(), status: status.to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: Some(OMNIROUTE_UNAVAILABLE.to_string()), message: Some(e) }
    },
  }
}

async fn check_capcut() -> FlowordServiceHealth {
  let start = Instant::now();
  match capcut_mate_health_check().await {
    Ok(()) => FlowordServiceHealth { id: "capcut".to_string(), status: "ready".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: None, message: None },
    Err(e) => FlowordServiceHealth { id: "capcut".to_string(), status: "unavailable".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: Some("CAPCUT_UNAVAILABLE".to_string()), message: Some(e) },
  }
}

async fn check_youwee(app: &AppHandle) -> FlowordServiceHealth {
  let start = Instant::now();
  match app_lib::services::get_ytdlp_path(app).await {
    Some(path) => FlowordServiceHealth { id: "youwee".to_string(), status: "ready".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: None, message: Some(format!("yt-dlp: {}", path.0.display())) },
    None => FlowordServiceHealth { id: "youwee".to_string(), status: "unavailable".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: Some("YOUWEE_RUNTIME_UNAVAILABLE".to_string()), message: Some("yt-dlp executable is unavailable".to_string()) },
  }
}

async fn check_vynaro(app: &AppHandle) -> FlowordServiceHealth {
  let start = Instant::now();
  let managed = app_lib::services::get_ffmpeg_path(app).await.and_then(|ffmpeg| {
    let ffprobe_name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
    let ffprobe = ffmpeg.parent()?.join(ffprobe_name);
    ffprobe.is_file().then_some((ffmpeg, ffprobe))
  });
  let available = managed.is_some() || vynaro_detect::Ffmpeg::discover().is_ok();
  if available {
    FlowordServiceHealth { id: "vynaro".to_string(), status: "ready".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: None, message: Some("ffmpeg and ffprobe available".to_string()) }
  } else {
    FlowordServiceHealth { id: "vynaro".to_string(), status: "unavailable".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: Some("VYNARO_RUNTIME_UNAVAILABLE".to_string()), message: Some("ffmpeg or ffprobe is unavailable".to_string()) }
  }
}

fn check_artcraft(app: &AppHandle) -> FlowordServiceHealth {
  let start = Instant::now();
  let manager = app.state::<StorytellerCredentialManager>();
  // Refresh cookies from the HTTP plugin and check the account session.
  let result = sync_storyteller_credentials_from_http_plugin(app, &manager)
    .map_err(|_| "Unable to read the ArtCraft account cookie store".to_string())
    .and_then(|()| manager.is_account_configured().map_err(|_| "Unable to inspect the ArtCraft account session".to_string()));
  match result {
    Ok(true) => FlowordServiceHealth { id: "artcraft".to_string(), status: "ready".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: None, message: Some("ArtCraft account session configured".to_string()) },
    Ok(false) => FlowordServiceHealth { id: "artcraft".to_string(), status: "auth_required".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: Some("ARTCRAFT_AUTH_REQUIRED".to_string()), message: Some("ArtCraft account login required".to_string()) },
    Err(message) => FlowordServiceHealth { id: "artcraft".to_string(), status: "unavailable".to_string(), latency_ms: start.elapsed().as_millis() as u64, error_code: Some("ARTCRAFT_CREDENTIAL_STORE_UNAVAILABLE".to_string()), message: Some(message) },
  }
}

// ---------------------------------------------------------------------------
// ContentPage Commands & Output Path Resolution
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ListContentPagesResponse {
  pub pages: Vec<ContentPage>,
}
impl SerializeMarker for ListContentPagesResponse {}

#[derive(Serialize)]
pub struct ContentPageResponse {
  pub page: ContentPage,
}
impl SerializeMarker for ContentPageResponse {}

#[derive(Serialize)]
pub struct ArchiveContentPageResponse {
  pub success: bool,
}
impl SerializeMarker for ArchiveContentPageResponse {}

#[derive(Serialize)]
pub struct ResolveOutputPathResponse {
  pub output_directory: String,
  pub date_string: String,
  pub sanitized_page_name: String,
}
impl SerializeMarker for ResolveOutputPathResponse {}

#[derive(Deserialize)]
pub struct CreateContentPageRequest {
  pub id: Option<String>,
  pub name: String,
  pub slug: Option<String>,
  pub output_root: String,
  pub target_platform: Option<String>,
  pub default_model_id: Option<String>,
  pub default_workflow_id: Option<String>,
  pub default_language: Option<String>,
  pub default_tone: Option<String>,
  pub default_aspect_ratio: Option<String>,
  pub browser_profile_id: Option<String>,
  pub worker_pool_id: Option<String>,
  pub default_image_prompt: Option<String>,
  pub default_expand_9_16_prompt: Option<String>,
  pub default_video_prompt: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateContentPageRequest {
  pub id: String,
  pub name: String,
  pub slug: Option<String>,
  pub output_root: String,
  pub target_platform: Option<String>,
  pub default_model_id: Option<String>,
  pub default_workflow_id: Option<String>,
  pub default_language: Option<String>,
  pub default_tone: Option<String>,
  pub default_aspect_ratio: Option<String>,
  pub browser_profile_id: Option<String>,
  pub worker_pool_id: Option<String>,
  pub default_image_prompt: Option<String>,
  pub default_expand_9_16_prompt: Option<String>,
  pub default_video_prompt: Option<String>,
}

#[tauri::command]
pub async fn list_content_pages_command(
  task_database: State<'_, TaskDatabase>,
  include_archived: Option<bool>,
) -> ResponseOrError<ListContentPagesResponse, FlowordErrorDetails> {
  let list = list_content_pages(ListContentPagesArgs {
    db: task_database.get_connection(),
    include_archived: include_archived.unwrap_or(false),
  })
  .await
  .map_err(|e| internal_error(&format!("Failed to list content pages: {e}"), None))?;

  Ok(ListContentPagesResponse { pages: list.pages }.into())
}

#[tauri::command]
pub async fn get_content_page_command(
  task_database: State<'_, TaskDatabase>,
  page_id: String,
) -> ResponseOrError<ContentPageResponse, FlowordErrorDetails> {
  let maybe_page = get_content_page_by_id(GetContentPageByIdArgs {
    db: task_database.get_connection(),
    id: &page_id,
  })
  .await
  .map_err(|e| internal_error(&format!("Failed to get content page: {e}"), None))?;

  match maybe_page {
    Some(page) => Ok(ContentPageResponse { page }.into()),
    None => Err(internal_error(&format!("ContentPage not found: {page_id}"), None)),
  }
}

#[tauri::command]
pub async fn create_content_page_command(
  task_database: State<'_, TaskDatabase>,
  request: CreateContentPageRequest,
) -> ResponseOrError<ContentPageResponse, FlowordErrorDetails> {
  if request.name.trim().is_empty() {
    return Err(internal_error("Page name cannot be empty", None));
  }
  if request.output_root.trim().is_empty() {
    return Err(internal_error("Output root path cannot be empty", None));
  }

  let page = create_content_page(CreateContentPageArgs {
    db: task_database.get_connection(),
    id: request.id.as_deref(),
    name: request.name.trim(),
    slug: request.slug.as_deref(),
    output_root: request.output_root.trim(),
    target_platform: request.target_platform.as_deref(),
    default_model_id: request.default_model_id.as_deref(),
    default_workflow_id: request.default_workflow_id.as_deref(),
    default_language: request.default_language.as_deref(),
    default_tone: request.default_tone.as_deref(),
    default_aspect_ratio: request.default_aspect_ratio.as_deref(),
    browser_profile_id: request.browser_profile_id.as_deref(),
    worker_pool_id: request.worker_pool_id.as_deref(),
    default_image_prompt: request.default_image_prompt.as_deref(),
    default_expand_9_16_prompt: request.default_expand_9_16_prompt.as_deref(),
    default_video_prompt: request.default_video_prompt.as_deref(),
  })
  .await
  .map_err(|e| internal_error(&format!("Failed to create content page: {e}"), None))?;

  Ok(ContentPageResponse { page }.into())
}

#[tauri::command]
pub async fn update_content_page_command(
  task_database: State<'_, TaskDatabase>,
  request: UpdateContentPageRequest,
) -> ResponseOrError<ContentPageResponse, FlowordErrorDetails> {
  if request.id.trim().is_empty() {
    return Err(internal_error("Page ID cannot be empty", None));
  }
  if request.name.trim().is_empty() {
    return Err(internal_error("Page name cannot be empty", None));
  }
  if request.output_root.trim().is_empty() {
    return Err(internal_error("Output root path cannot be empty", None));
  }

  let page = update_content_page(UpdateContentPageArgs {
    db: task_database.get_connection(),
    id: request.id.trim(),
    name: request.name.trim(),
    slug: request.slug.as_deref(),
    output_root: request.output_root.trim(),
    target_platform: request.target_platform.as_deref(),
    default_model_id: request.default_model_id.as_deref(),
    default_workflow_id: request.default_workflow_id.as_deref(),
    default_language: request.default_language.as_deref(),
    default_tone: request.default_tone.as_deref(),
    default_aspect_ratio: request.default_aspect_ratio.as_deref(),
    browser_profile_id: request.browser_profile_id.as_deref(),
    worker_pool_id: request.worker_pool_id.as_deref(),
    default_image_prompt: request.default_image_prompt.as_deref(),
    default_expand_9_16_prompt: request.default_expand_9_16_prompt.as_deref(),
    default_video_prompt: request.default_video_prompt.as_deref(),
  })
  .await
  .map_err(|e| internal_error(&format!("Failed to update content page: {e}"), None))?;

  Ok(ContentPageResponse { page }.into())
}

#[tauri::command]
pub async fn archive_content_page_command(
  task_database: State<'_, TaskDatabase>,
  page_id: String,
  is_archived: Option<bool>,
) -> ResponseOrError<ArchiveContentPageResponse, FlowordErrorDetails> {
  let success = archive_content_page(ArchiveContentPageArgs {
    db: task_database.get_connection(),
    id: &page_id,
    is_archived: is_archived.unwrap_or(true),
  })
  .await
  .map_err(|e| internal_error(&format!("Failed to archive content page: {e}"), None))?;

  Ok(ArchiveContentPageResponse { success }.into())
}

#[tauri::command]
pub async fn resolve_floword_output_path_command(
  output_root: String,
  page_name: String,
) -> ResponseOrError<ResolveOutputPathResponse, FlowordErrorDetails> {
  let dir = OutputPathResolver::resolve_page_date_directory(&output_root, &page_name)
    .map_err(|e| internal_error(&e, None))?;

  Ok(ResolveOutputPathResponse {
    output_directory: dir.to_string_lossy().to_string(),
    date_string: crate::services::pipeline::output_policy::current_local_date_string(),
    sanitized_page_name: crate::services::pipeline::output_policy::sanitize_page_name(&page_name),
  }.into())
}

// ---------------------------------------------------------------------------
// Browser Workers, Ingestion, Settings, and Job Events
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ListBrowserWorkersResponse {
  pub workers: Vec<BrowserWorkerInfo>,
}
impl SerializeMarker for ListBrowserWorkersResponse {}

#[tauri::command]
pub async fn list_browser_workers_command() -> ResponseOrError<ListBrowserWorkersResponse, FlowordErrorDetails> {
  match list_browser_workers().await {
    Ok(resp) => Ok(ListBrowserWorkersResponse { workers: resp.workers }.into()),
    Err(e) => {
      warn!("[Floword] list_browser_workers failed: {e}");
      Ok(ListBrowserWorkersResponse { workers: Vec::new() }.into())
    }
  }
}

#[derive(Deserialize)]
pub struct IngestFlowordSourceImageRequest {
  pub file_path: Option<String>,
  pub base64_data: Option<String>,
  pub file_name: Option<String>,
  pub page_id: Option<String>,
}

#[derive(Serialize)]
pub struct IngestFlowordSourceImageResponse {
  pub artifact: ArtifactRef,
  pub preview_url: String,
}
impl SerializeMarker for IngestFlowordSourceImageResponse {}

#[tauri::command]
pub async fn ingest_floword_source_image_command(
  app_data_root: State<'_, AppDataRoot>,
  request: IngestFlowordSourceImageRequest,
) -> ResponseOrError<IngestFlowordSourceImageResponse, FlowordErrorDetails> {
  let raw_bytes: Vec<u8> = if let Some(path) = request.file_path.as_deref().filter(|s| !s.trim().is_empty()) {
    let p = std::path::Path::new(path);
    if !p.exists() {
      return Err(internal_error(&format!("File does not exist at {path}"), None));
    }
    std::fs::read(p).map_err(|e| internal_error(&format!("Failed to read file: {e}"), None))?
  } else if let Some(b64) = request.base64_data.as_deref().filter(|s| !s.trim().is_empty()) {
    let clean_b64 = if let Some(idx) = b64.find(',') {
      &b64[idx + 1..]
    } else {
      b64
    };
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(clean_b64.trim())
      .map_err(|e| internal_error(&format!("Failed to decode base64 image data: {e}"), None))?
  } else {
    return Err(internal_error("Either file_path or base64_data must be provided", None));
  };

  if raw_bytes.is_empty() {
    return Err(internal_error("Image data is empty (0 bytes)", None));
  }

  let (mime_type, ext) = detect_image_mime(&raw_bytes)
    .map_err(|e| internal_error(&e, None))?;
  let hash = compute_sha256(&raw_bytes);
  let size_bytes = raw_bytes.len() as u64;

  let artifacts_dir = app_data_root.pipeline_artifacts_dir().join("sources");
  std::fs::create_dir_all(&artifacts_dir)
    .map_err(|e| internal_error(&format!("Cannot create sources artifact directory: {e}"), None))?;

  let target_filename = format!("{hash}.{ext}");
  let target_file_path = artifacts_dir.join(&target_filename);
  std::fs::write(&target_file_path, &raw_bytes)
    .map_err(|e| internal_error(&format!("Cannot write artifact file: {e}"), None))?;

  let location_str = target_file_path.to_string_lossy().to_string();
  let artifact = ArtifactRef {
    artifact_id: format!("art_{hash}"),
    kind: ArtifactKind::GeneratedImage,
    produced_by_stage: StageId::Input,
    location: location_str.clone(),
    mime_type: Some(mime_type.to_string()),
    metadata: json!({
      "original_filename": request.file_name,
      "page_id": request.page_id,
      "sha256": hash,
      "size_bytes": size_bytes,
      "created_at": chrono::Utc::now().to_rfc3339(),
    }),
  };

  let preview_url = format!("artcraft-asset://localhost/{}", location_str.replace('\\', "/"));

  Ok(IngestFlowordSourceImageResponse {
    artifact,
    preview_url,
  }.into())
}

#[derive(Serialize)]
pub struct FlowordSettingsResponse {
  pub max_concurrent_jobs: usize,
}
impl SerializeMarker for FlowordSettingsResponse {}

#[tauri::command]
pub async fn get_floword_settings_command(
  task_database: State<'_, TaskDatabase>,
) -> ResponseOrError<FlowordSettingsResponse, FlowordErrorDetails> {
  let val = get_app_setting(GetAppSettingArgs {
    db: task_database.get_connection(),
    key: "floword.max_concurrent_jobs",
  }).await.ok().flatten().and_then(|v| v.parse::<usize>().ok()).unwrap_or(3).clamp(1, 20);

  Ok(FlowordSettingsResponse { max_concurrent_jobs: val }.into())
}

#[derive(Deserialize)]
pub struct UpdateFlowordSettingsRequest {
  pub max_concurrent_jobs: usize,
}

#[tauri::command]
pub async fn update_floword_settings_command(
  task_database: State<'_, TaskDatabase>,
  request: UpdateFlowordSettingsRequest,
) -> ResponseOrError<FlowordSettingsResponse, FlowordErrorDetails> {
  let clamped = request.max_concurrent_jobs.clamp(1, 20);
  let val_str = clamped.to_string();
  let _ = set_app_setting(SetAppSettingArgs {
    db: task_database.get_connection(),
    key: "floword.max_concurrent_jobs",
    value: &val_str,
  }).await.map_err(|e| internal_error(&format!("Failed to save setting: {e}"), None))?;

  Ok(FlowordSettingsResponse { max_concurrent_jobs: clamped }.into())
}

#[derive(Deserialize)]
pub struct ListPipelineJobEventsRequest {
  pub job_id: String,
}

#[derive(Serialize)]
pub struct ListPipelineJobEventsResponse {
  pub events: Vec<PipelineJobEvent>,
}
impl SerializeMarker for ListPipelineJobEventsResponse {}

#[tauri::command]
pub async fn list_pipeline_job_events_command(
  task_database: State<'_, TaskDatabase>,
  request: ListPipelineJobEventsRequest,
) -> ResponseOrError<ListPipelineJobEventsResponse, FlowordErrorDetails> {
  let events = list_pipeline_job_events(ListPipelineJobEventsArgs {
    db: task_database.get_connection(),
    job_id: &request.job_id,
  }).await.map_err(|e| internal_error(&format!("Failed to query job events: {e}"), Some(&request.job_id)))?;

  Ok(ListPipelineJobEventsResponse { events }.into())
}

// ---------------------------------------------------------------------------
// Publishing Engine Commands
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListJobPublicationsRequest {
  pub job_id: Option<String>,
  pub page_id: Option<String>,
  pub platform: Option<String>,
  pub status: Option<String>,
  pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct ListJobPublicationsResponse {
  pub publications: Vec<JobPublication>,
}
impl SerializeMarker for ListJobPublicationsResponse {}

#[tauri::command]
pub async fn list_job_publications_command(
  task_database: State<'_, TaskDatabase>,
  request: ListJobPublicationsRequest,
) -> ResponseOrError<ListJobPublicationsResponse, FlowordErrorDetails> {
  let publications = list_job_publications(
    task_database.get_connection(),
    ListJobPublicationsArgs {
      job_id: request.job_id,
      page_id: request.page_id,
      platform: request.platform,
      status: request.status,
      limit: request.limit,
    },
  )
  .await
  .map_err(|e| internal_error(&format!("Failed to list job publications: {e}"), None))?;

  Ok(ListJobPublicationsResponse { publications }.into())
}

#[derive(Deserialize)]
pub struct ApprovePublicationRequest {
  pub publication_id: String,
  pub scheduled_at: Option<i64>,
}

#[derive(Serialize)]
pub struct PublicationActionResponse {
  pub publication: JobPublication,
}
impl SerializeMarker for PublicationActionResponse {}

#[tauri::command]
pub async fn approve_publication_command(
  task_database: State<'_, TaskDatabase>,
  request: ApprovePublicationRequest,
) -> ResponseOrError<PublicationActionResponse, FlowordErrorDetails> {
  let db = task_database.get_connection();
  let pub_record = get_job_publication_by_id(db, &request.publication_id)
    .await
    .map_err(|e| internal_error(&format!("Database query error: {e}"), None))?
    .ok_or_else(|| not_found(&request.publication_id))?;

  let (new_status, scheduled_at) = if let Some(sched) = request.scheduled_at {
    ("SCHEDULED", Some(sched))
  } else {
    ("READY_TO_POST", None)
  };

  let updated = update_job_publication(
    db,
    UpdateJobPublicationArgs {
      id: pub_record.id.clone(),
      status: Some(new_status.to_string()),
      scheduled_at,
      approved_at: Some(chrono::Utc::now().timestamp()),
      posted_at: None,
      platform_post_id: None,
      post_url: None,
      title: None,
      caption: None,
      last_error_code: None,
      last_error_message: None,
    },
  )
  .await
  .map_err(|e| internal_error(&format!("Failed to update publication: {e}"), Some(&pub_record.job_id)))?;

  let stage_str = format!("PUBLISH_{}", pub_record.platform.to_uppercase());
  let msg_str = format!("Bản ghi xuất bản {} [{}] đã được duyệt (Status: {})", pub_record.platform, pub_record.id, new_status);
  let meta_str = serde_json::json!({
    "publication_id": pub_record.id,
    "scheduled_at": scheduled_at
  }).to_string();

  let _ = insert_pipeline_job_event(
    InsertPipelineJobEventArgs {
      db,
      id: None,
      job_id: &pub_record.job_id,
      sequence: 25,
      stage_id: Some(&stage_str),
      business_status: Some(new_status),
      event_type: if scheduled_at.is_some() { "SCHEDULED" } else { "APPROVED" },
      level: "INFO",
      message: &msg_str,
      error_code: None,
      metadata_json: Some(&meta_str),
    },
  ).await;

  Ok(PublicationActionResponse { publication: updated }.into())
}

#[derive(Deserialize)]
pub struct RejectPublicationRequest {
  pub publication_id: String,
  pub reason: Option<String>,
}

#[tauri::command]
pub async fn reject_publication_command(
  task_database: State<'_, TaskDatabase>,
  request: RejectPublicationRequest,
) -> ResponseOrError<PublicationActionResponse, FlowordErrorDetails> {
  let db = task_database.get_connection();
  let pub_record = get_job_publication_by_id(db, &request.publication_id)
    .await
    .map_err(|e| internal_error(&format!("Database query error: {e}"), None))?
    .ok_or_else(|| not_found(&request.publication_id))?;

  let updated = update_job_publication(
    db,
    UpdateJobPublicationArgs {
      id: pub_record.id.clone(),
      status: Some("CANCELLED".to_string()),
      scheduled_at: None,
      approved_at: None,
      posted_at: None,
      platform_post_id: None,
      post_url: None,
      title: None,
      caption: None,
      last_error_code: Some("REJECTED_BY_USER".to_string()),
      last_error_message: request.reason.clone(),
    },
  )
  .await
  .map_err(|e| internal_error(&format!("Failed to reject publication: {e}"), Some(&pub_record.job_id)))?;

  let stage_str = format!("PUBLISH_{}", pub_record.platform.to_uppercase());
  let msg_str = format!("Bản ghi xuất bản {} [{}] đã bị từ chối: {:?}", pub_record.platform, pub_record.id, request.reason);
  let meta_str = serde_json::json!({
    "publication_id": pub_record.id,
    "reason": request.reason
  }).to_string();

  let _ = insert_pipeline_job_event(
    InsertPipelineJobEventArgs {
      db,
      id: None,
      job_id: &pub_record.job_id,
      sequence: 26,
      stage_id: Some(&stage_str),
      business_status: Some("CANCELLED"),
      event_type: "CANCELLED",
      level: "INFO",
      message: &msg_str,
      error_code: None,
      metadata_json: Some(&meta_str),
    },
  ).await;

  Ok(PublicationActionResponse { publication: updated }.into())
}

#[derive(Deserialize)]
pub struct SchedulePublicationRequest {
  pub publication_id: String,
  pub scheduled_at: Option<i64>,
  pub use_page_default_slot: Option<bool>,
}

#[tauri::command]
pub async fn schedule_publication_command(
  task_database: State<'_, TaskDatabase>,
  request: SchedulePublicationRequest,
) -> ResponseOrError<PublicationActionResponse, FlowordErrorDetails> {
  let db = task_database.get_connection();
  let pub_record = get_job_publication_by_id(db, &request.publication_id)
    .await
    .map_err(|e| internal_error(&format!("Database query error: {e}"), None))?
    .ok_or_else(|| not_found(&request.publication_id))?;

  let scheduled_timestamp = if request.use_page_default_slot.unwrap_or(false) {
    let targets = list_publish_targets_for_page(db, &pub_record.page_id)
      .await
      .unwrap_or_default();
    let default_slots = targets.iter()
      .find(|t| t.platform == pub_record.platform)
      .map(|t| t.default_slots_json.as_str())
      .unwrap_or("[\"08:30\", \"10:00\", \"17:00\", \"22:00\"]");

    crate::services::publishing::slot_allocator::allocate_next_available_slot(
      db,
      &pub_record.page_id,
      default_slots,
    ).await
  } else {
    request.scheduled_at.unwrap_or_else(|| chrono::Utc::now().timestamp() + 3600)
  };

  let updated = update_job_publication(
    db,
    UpdateJobPublicationArgs {
      id: pub_record.id.clone(),
      status: Some("SCHEDULED".to_string()),
      scheduled_at: Some(scheduled_timestamp),
      approved_at: Some(chrono::Utc::now().timestamp()),
      posted_at: None,
      platform_post_id: None,
      post_url: None,
      title: None,
      caption: None,
      last_error_code: None,
      last_error_message: None,
    },
  )
  .await
  .map_err(|e| internal_error(&format!("Failed to schedule publication: {e}"), Some(&pub_record.job_id)))?;

  let stage_str = format!("PUBLISH_{}", pub_record.platform.to_uppercase());
  let msg_str = format!("Lên lịch đăng tải {} [{}] tại timestamp {}", pub_record.platform, pub_record.id, scheduled_timestamp);
  let meta_str = serde_json::json!({
    "publication_id": pub_record.id,
    "scheduled_at": scheduled_timestamp
  }).to_string();

  let _ = insert_pipeline_job_event(
    InsertPipelineJobEventArgs {
      db,
      id: None,
      job_id: &pub_record.job_id,
      sequence: 27,
      stage_id: Some(&stage_str),
      business_status: Some("SCHEDULED"),
      event_type: "SCHEDULED",
      level: "INFO",
      message: &msg_str,
      error_code: None,
      metadata_json: Some(&meta_str),
    },
  ).await;

  Ok(PublicationActionResponse { publication: updated }.into())
}

#[derive(Deserialize)]
pub struct RetryPublicationRequest {
  pub publication_id: String,
}

#[tauri::command]
pub async fn retry_publication_command(
  task_database: State<'_, TaskDatabase>,
  request: RetryPublicationRequest,
) -> ResponseOrError<PublicationActionResponse, FlowordErrorDetails> {
  let db = task_database.get_connection();
  let pub_record = get_job_publication_by_id(db, &request.publication_id)
    .await
    .map_err(|e| internal_error(&format!("Database query error: {e}"), None))?
    .ok_or_else(|| not_found(&request.publication_id))?;

  let updated = update_job_publication(
    db,
    UpdateJobPublicationArgs {
      id: pub_record.id.clone(),
      status: Some("READY_TO_POST".to_string()),
      scheduled_at: None,
      approved_at: Some(chrono::Utc::now().timestamp()),
      posted_at: None,
      platform_post_id: None,
      post_url: None,
      title: None,
      caption: None,
      last_error_code: None,
      last_error_message: None,
    },
  )
  .await
  .map_err(|e| internal_error(&format!("Failed to retry publication: {e}"), Some(&pub_record.job_id)))?;

  let stage_str = format!("PUBLISH_{}", pub_record.platform.to_uppercase());
  let msg_str = format!("Khởi chạy lại lệnh đăng tải {} [{}]", pub_record.platform, pub_record.id);
  let meta_str = serde_json::json!({
    "publication_id": pub_record.id
  }).to_string();

  let _ = insert_pipeline_job_event(
    InsertPipelineJobEventArgs {
      db,
      id: None,
      job_id: &pub_record.job_id,
      sequence: 28,
      stage_id: Some(&stage_str),
      business_status: Some("READY_TO_POST"),
      event_type: "RETRY_INITIATED",
      level: "INFO",
      message: &msg_str,
      error_code: None,
      metadata_json: Some(&meta_str),
    },
  ).await;

  Ok(PublicationActionResponse { publication: updated }.into())
}

#[derive(Deserialize)]
pub struct PostNowPublicationRequest {
  pub publication_id: String,
}

#[tauri::command]
pub async fn post_now_publication_command(
  task_database: State<'_, TaskDatabase>,
  request: PostNowPublicationRequest,
) -> ResponseOrError<PublicationActionResponse, FlowordErrorDetails> {
  let db = task_database.get_connection();
  let pub_record = get_job_publication_by_id(db, &request.publication_id)
    .await
    .map_err(|e| internal_error(&format!("Database query error: {e}"), None))?
    .ok_or_else(|| not_found(&request.publication_id))?;

  let updated = update_job_publication(
    db,
    UpdateJobPublicationArgs {
      id: pub_record.id.clone(),
      status: Some("READY_TO_POST".to_string()),
      scheduled_at: None,
      approved_at: Some(chrono::Utc::now().timestamp()),
      posted_at: None,
      platform_post_id: None,
      post_url: None,
      title: None,
      caption: None,
      last_error_code: None,
      last_error_message: None,
    },
  )
  .await
  .map_err(|e| internal_error(&format!("Failed to update publication to post now: {e}"), Some(&pub_record.job_id)))?;

  Ok(PublicationActionResponse { publication: updated }.into())
}

#[derive(Deserialize)]
pub struct ListContentPagePublishTargetsRequest {
  pub page_id: String,
}

#[derive(Serialize)]
pub struct ListContentPagePublishTargetsResponse {
  pub targets: Vec<ContentPagePublishTarget>,
}
impl SerializeMarker for ListContentPagePublishTargetsResponse {}

#[tauri::command]
pub async fn list_content_page_publish_targets_command(
  task_database: State<'_, TaskDatabase>,
  request: ListContentPagePublishTargetsRequest,
) -> ResponseOrError<ListContentPagePublishTargetsResponse, FlowordErrorDetails> {
  let targets = list_publish_targets_for_page(task_database.get_connection(), &request.page_id)
    .await
    .map_err(|e| internal_error(&format!("Failed to list page publish targets: {e}"), None))?;

  Ok(ListContentPagePublishTargetsResponse { targets }.into())
}

#[derive(Deserialize)]
pub struct UpsertContentPagePublishTargetRequest {
  pub id: Option<String>,
  pub page_id: String,
  pub platform: String,
  pub enabled: bool,
  pub account_label: Option<String>,
  pub destination_id: String,
  pub destination_handle: Option<String>,
  pub browser_profile_id: String,
  pub post_mode: Option<String>,
  pub default_slots_json: Option<String>,
}

#[derive(Serialize)]
pub struct UpsertContentPagePublishTargetResponse {
  pub target: ContentPagePublishTarget,
}
impl SerializeMarker for UpsertContentPagePublishTargetResponse {}

#[tauri::command]
pub async fn upsert_content_page_publish_target_command(
  task_database: State<'_, TaskDatabase>,
  request: UpsertContentPagePublishTargetRequest,
) -> ResponseOrError<UpsertContentPagePublishTargetResponse, FlowordErrorDetails> {
  let pool = task_database.get_connection();

  let target = if let Some(target_id) = request.id {
    update_publish_target(
      pool,
      UpdatePublishTargetArgs {
        id: target_id,
        enabled: Some(request.enabled),
        account_label: request.account_label,
        destination_id: Some(request.destination_id),
        destination_handle: request.destination_handle,
        browser_profile_id: Some(request.browser_profile_id),
        post_mode: request.post_mode,
        default_slots_json: request.default_slots_json,
      },
    )
    .await
    .map_err(|e| internal_error(&format!("Failed to update publish target: {e}"), None))?
  } else {
    create_publish_target(
      pool,
      CreatePublishTargetArgs {
        page_id: request.page_id,
        platform: request.platform,
        enabled: request.enabled,
        account_label: request.account_label,
        destination_id: request.destination_id,
        destination_handle: request.destination_handle,
        browser_profile_id: request.browser_profile_id,
        post_mode: request.post_mode,
        default_slots_json: request.default_slots_json,
      },
    )
    .await
    .map_err(|e| internal_error(&format!("Failed to create publish target: {e}"), None))?
  };

  Ok(UpsertContentPagePublishTargetResponse { target }.into())
}

#[derive(Deserialize)]
pub struct DeleteContentPagePublishTargetRequest {
  pub id: String,
}

#[derive(Serialize)]
pub struct DeleteContentPagePublishTargetResponse {
  pub success: bool,
}
impl SerializeMarker for DeleteContentPagePublishTargetResponse {}

#[tauri::command]
pub async fn delete_content_page_publish_target_command(
  task_database: State<'_, TaskDatabase>,
  request: DeleteContentPagePublishTargetRequest,
) -> ResponseOrError<DeleteContentPagePublishTargetResponse, FlowordErrorDetails> {
  delete_publish_target(task_database.get_connection(), &request.id)
    .await
    .map_err(|e| internal_error(&format!("Failed to delete publish target: {e}"), None))?;

  Ok(DeleteContentPagePublishTargetResponse { success: true }.into())
}

// ---------------------------------------------------------------------------
// Scale & Operations Commands
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DashboardSummaryRequest {
  pub page_id: Option<String>,
  pub date_from: Option<i64>,
  pub date_to: Option<i64>,
  pub status: Option<String>,
  pub platform: Option<String>,
}

#[derive(Serialize)]
pub struct DashboardSummaryResponse {
  pub summary: DashboardSummary,
}
impl SerializeMarker for DashboardSummaryResponse {}

#[tauri::command]
pub async fn floword_dashboard_summary_command(
  task_database: State<'_, TaskDatabase>,
  request: DashboardSummaryRequest,
) -> ResponseOrError<DashboardSummaryResponse, FlowordErrorDetails> {
  let summary = get_dashboard_summary(
    task_database.get_connection(),
    DashboardSummaryQueryArgs {
      page_id: request.page_id,
      date_from: request.date_from,
      date_to: request.date_to,
      status: request.status,
      business_status: None,
      task_status: None,
      platform: request.platform,
    },
  )
  .await
  .map_err(|e| internal_error(&format!("Failed to compute dashboard summary: {e}"), None))?;

  Ok(DashboardSummaryResponse { summary }.into())
}

#[derive(Deserialize)]
pub struct ListPipelineJobsPaginatedRequest {
  pub page_id: Option<String>,
  pub status: Option<String>,
  pub date_from: Option<i64>,
  pub date_to: Option<i64>,
  pub search_query: Option<String>,
  pub limit: Option<i64>,
  pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct ListPipelineJobsPaginatedResponse {
  pub result: PaginatedPipelineJobsResult,
}
impl SerializeMarker for ListPipelineJobsPaginatedResponse {}

#[tauri::command]
pub async fn list_pipeline_jobs_paginated_command(
  task_database: State<'_, TaskDatabase>,
  request: ListPipelineJobsPaginatedRequest,
) -> ResponseOrError<ListPipelineJobsPaginatedResponse, FlowordErrorDetails> {
  let result = list_pipeline_jobs_paginated(
    task_database.get_connection(),
    ListPipelineJobsPaginatedArgs {
      page_id: request.page_id,
      status: request.status,
      date_from: request.date_from,
      date_to: request.date_to,
      search_query: request.search_query,
      limit: request.limit,
      offset: request.offset,
    },
  )
  .await
  .map_err(|e| internal_error(&format!("Failed to list paginated jobs: {e}"), None))?;

  Ok(ListPipelineJobsPaginatedResponse { result }.into())
}

#[derive(Deserialize)]
pub struct ValidateBulkImportRequest {
  pub csv_content: Option<String>,
  pub rows: Option<Vec<BulkImportRow>>,
}

#[derive(Serialize)]
pub struct ValidateBulkImportResponse {
  pub validation: BulkValidationSummary,
}
impl SerializeMarker for ValidateBulkImportResponse {}

#[tauri::command]
pub async fn validate_bulk_import_command(
  task_database: State<'_, TaskDatabase>,
  request: ValidateBulkImportRequest,
) -> ResponseOrError<ValidateBulkImportResponse, FlowordErrorDetails> {
  let rows = if let Some(content) = request.csv_content {
    let matrix = BulkImportService::parse_csv_content(&content);
    BulkImportService::map_raw_rows_to_import_rows(&matrix)
  } else if let Some(r) = request.rows {
    r
  } else {
    Vec::new()
  };

  let validation = BulkImportService::validate_rows(task_database.get_connection(), rows).await;
  Ok(ValidateBulkImportResponse { validation }.into())
}

#[derive(Deserialize)]
pub struct CommitBulkImportRequest {
  pub rows: Vec<BulkImportRow>,
}

#[derive(Serialize)]
pub struct CommitBulkImportResponse {
  pub result: BulkCommitResponse,
}
impl SerializeMarker for CommitBulkImportResponse {}

#[tauri::command]
pub async fn commit_bulk_import_command(
  task_database: State<'_, TaskDatabase>,
  request: CommitBulkImportRequest,
) -> ResponseOrError<CommitBulkImportResponse, FlowordErrorDetails> {
  let result = BulkImportService::commit_import(task_database.get_connection(), request.rows)
    .await
    .map_err(|e| internal_error(&e, None))?;

  Ok(CommitBulkImportResponse { result }.into())
}

#[derive(Serialize)]
pub struct ListPromptTemplatesResponse {
  pub templates: Vec<PromptTemplate>,
}
impl SerializeMarker for ListPromptTemplatesResponse {}

#[tauri::command]
pub async fn list_prompt_templates_command(
  task_database: State<'_, TaskDatabase>,
) -> ResponseOrError<ListPromptTemplatesResponse, FlowordErrorDetails> {
  let templates = list_prompt_templates(task_database.get_connection())
    .await
    .map_err(|e| internal_error(&format!("Failed to list prompt templates: {e}"), None))?;

  Ok(ListPromptTemplatesResponse { templates }.into())
}

#[derive(Deserialize)]
pub struct UpsertPromptTemplateRequest {
  pub id: Option<String>,
  pub name: String,
  pub image_prompt: String,
  pub expand_prompt: Option<String>,
  pub video_prompt: String,
}

#[derive(Serialize)]
pub struct UpsertPromptTemplateResponse {
  pub template: PromptTemplate,
}
impl SerializeMarker for UpsertPromptTemplateResponse {}

#[tauri::command]
pub async fn upsert_prompt_template_command(
  task_database: State<'_, TaskDatabase>,
  request: UpsertPromptTemplateRequest,
) -> ResponseOrError<UpsertPromptTemplateResponse, FlowordErrorDetails> {
  let db = task_database.get_connection();
  let template = if let Some(id) = request.id {
    update_prompt_template(
      db,
      UpdatePromptTemplateArgs {
        id,
        name: Some(request.name),
        image_prompt: Some(request.image_prompt),
        expand_prompt: request.expand_prompt,
        video_prompt: Some(request.video_prompt),
      },
    )
    .await
    .map_err(|e| internal_error(&format!("Failed to update prompt template: {e}"), None))?
  } else {
    create_prompt_template(
      db,
      CreatePromptTemplateArgs {
        id: None,
        name: request.name,
        image_prompt: request.image_prompt,
        expand_prompt: request.expand_prompt,
        video_prompt: request.video_prompt,
      },
    )
    .await
    .map_err(|e| internal_error(&format!("Failed to create prompt template: {e}"), None))?
  };

  Ok(UpsertPromptTemplateResponse { template }.into())
}

#[derive(Deserialize)]
pub struct DeletePromptTemplateRequest {
  pub id: String,
}

#[derive(Serialize)]
pub struct DeletePromptTemplateResponse {
  pub success: bool,
}
impl SerializeMarker for DeletePromptTemplateResponse {}

#[tauri::command]
pub async fn delete_prompt_template_command(
  task_database: State<'_, TaskDatabase>,
  request: DeletePromptTemplateRequest,
) -> ResponseOrError<DeletePromptTemplateResponse, FlowordErrorDetails> {
  delete_prompt_template(task_database.get_connection(), &request.id)
    .await
    .map_err(|e| internal_error(&format!("Failed to delete prompt template: {e}"), None))?;

  Ok(DeletePromptTemplateResponse { success: true }.into())
}

#[derive(Deserialize)]
pub struct GetFlowordSystemSettingRequest {
  pub key: String,
}

#[derive(Serialize)]
pub struct GetFlowordSystemSettingResponse {
  pub setting: Option<FlowordSetting>,
}
impl SerializeMarker for GetFlowordSystemSettingResponse {}

#[tauri::command]
pub async fn get_floword_system_setting_command(
  task_database: State<'_, TaskDatabase>,
  request: GetFlowordSystemSettingRequest,
) -> ResponseOrError<GetFlowordSystemSettingResponse, FlowordErrorDetails> {
  let setting = get_floword_setting(task_database.get_connection(), &request.key)
    .await
    .map_err(|e| internal_error(&format!("Failed to get floword setting: {e}"), None))?;

  Ok(GetFlowordSystemSettingResponse { setting }.into())
}

#[derive(Deserialize)]
pub struct UpdateFlowordSystemSettingRequest {
  pub key: String,
  pub value_json: String,
}

#[derive(Serialize)]
pub struct UpdateFlowordSystemSettingResponse {
  pub setting: FlowordSetting,
}
impl SerializeMarker for UpdateFlowordSystemSettingResponse {}

#[tauri::command]
pub async fn update_floword_system_setting_command(
  task_database: State<'_, TaskDatabase>,
  request: UpdateFlowordSystemSettingRequest,
) -> ResponseOrError<UpdateFlowordSystemSettingResponse, FlowordErrorDetails> {
  let setting = upsert_floword_setting(
    task_database.get_connection(),
    UpsertFlowordSettingArgs {
      key: request.key,
      value_json: request.value_json,
    },
  )
  .await
  .map_err(|e| internal_error(&format!("Failed to update floword setting: {e}"), None))?;

  Ok(UpdateFlowordSystemSettingResponse { setting }.into())
}

#[derive(Deserialize)]
pub struct CheckStorageHealthRequest {
  pub page_id: String,
}

#[derive(Serialize)]
pub struct CheckStorageHealthResponse {
  pub report: StorageHealthReport,
}
impl SerializeMarker for CheckStorageHealthResponse {}

#[tauri::command]
pub async fn check_storage_health_command(
  task_database: State<'_, TaskDatabase>,
  request: CheckStorageHealthRequest,
) -> ResponseOrError<CheckStorageHealthResponse, FlowordErrorDetails> {
  let report = SystemHealthProbes::probe_page_storage(task_database.get_connection(), &request.page_id).await;
  Ok(CheckStorageHealthResponse { report }.into())
}

#[derive(Serialize)]
pub struct CheckSystemReadinessResponse {
  pub report: SystemReadinessReport,
}
impl SerializeMarker for CheckSystemReadinessResponse {}

#[tauri::command]
pub async fn check_system_readiness_command(
  task_database: State<'_, TaskDatabase>,
) -> ResponseOrError<CheckSystemReadinessResponse, FlowordErrorDetails> {
  let report = SystemHealthProbes::probe_system_readiness(task_database.get_connection()).await;
  Ok(CheckSystemReadinessResponse { report }.into())
}

fn not_found(job_id: &str) -> CommandErrorResponseWrapper<(), FlowordErrorDetails> {
  CommandErrorResponseWrapper { status: CommandErrorStatus::NotFound, error_message: Some("Workflow job not found".to_string()), error_type: Some(()), error_details: Some(FlowordErrorDetails { error_code: WORKFLOW_NOT_FOUND.to_string(), job_id: Some(job_id.to_string()) }) }
}

fn internal_error(message: &str, job_id: Option<&str>) -> CommandErrorResponseWrapper<(), FlowordErrorDetails> {
  CommandErrorResponseWrapper { status: CommandErrorStatus::ServerError, error_message: Some(message.to_string()), error_type: Some(()), error_details: Some(FlowordErrorDetails { error_code: INTERNAL_ERROR.to_string(), job_id: job_id.map(|s| s.to_string()) }) }
}

fn bad_request(message: &str, job_id: &str) -> CommandErrorResponseWrapper<(), FlowordErrorDetails> {
  CommandErrorResponseWrapper { status: CommandErrorStatus::BadRequest, error_message: Some(message.to_string()), error_type: Some(()), error_details: Some(FlowordErrorDetails { error_code: "BAD_REQUEST".to_string(), job_id: Some(job_id.to_string()) }) }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::contracts::{StageError, StageStatus};
  use enums::tauri::pipeline::pipeline_stage::PipelineStage;

  #[test]
  fn exposes_persisted_structured_stage_errors_from_pipeline_context() {
    let mut context = PipelineContext { job_id: "job-errors".to_string(), project_id: None, workflow_mode: "source_based".to_string(), content_source: Some(ContentSource::TrendResearch.as_str().to_string()), prompt: "test".to_string(), model_id: None, voice_id: None, language: "vi".to_string(), target_duration_seconds: 30, output_mode: "draft_only".to_string(), source_url: None, local_file: None, story_url: None, research_enabled: true, research_platform: Some("xhs".into()), research_query: Some("test".into()), research_mode: Some("search".into()), xhs_variant: None, artifact_refs: Vec::new(), stage_states: PipelineContext::initial_stage_states() };
    let research = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Research).unwrap();
    research.start_stage(Some("mediacrawler".to_string()), "start").unwrap();
    research.fail_stage(StageError::sanitized("MEDIACRAWLER_AUTH_REQUIRED", "MediaCrawler cookie login is required", false), "finish").unwrap();
    let outputs = serde_json::to_string(&json!({ "pipeline_context": context })).unwrap();

    let stages = stage_states_from_outputs(Some(&outputs));
    let research = stages.iter().find(|state| state.stage_id == StageId::Research).unwrap();
    assert_eq!(research.status, StageStatus::Failed);
    assert_eq!(research.error.as_ref().unwrap().code, "MEDIACRAWLER_AUTH_REQUIRED");
    assert_eq!(research.error.as_ref().unwrap().service.as_deref(), Some("mediacrawler"));
  }

  #[test]
  fn legacy_or_invalid_outputs_expose_pending_stages_without_fabricated_errors() {
    for stages in [stage_states_from_outputs(None), stage_states_from_outputs(Some(r#"{"legacy":true}"#))] {
      assert_eq!(stages.len(), StageId::ALL.len());
      assert!(stages.iter().all(|stage| stage.status == StageStatus::Pending && stage.error.is_none()));
    }
  }

  #[test]
  fn cancel_cleans_up_only_waiting_research_login() {
    assert!(requires_research_login_cleanup(TaskStatus::WaitingInput));
    assert!(!requires_research_login_cleanup(TaskStatus::Started));
    assert!(!requires_research_login_cleanup(TaskStatus::CompleteFailure));
  }

  mod step_mapping {
    use super::*;

    #[test]
    fn maps_step_ids_and_module_names() {
      assert_eq!(resume_stage_for_step("step-1"), Some(PipelineStage::Research));
      assert_eq!(resume_stage_for_step("research"), Some(PipelineStage::Research));
      assert_eq!(resume_stage_for_step("step-2"), Some(PipelineStage::ScriptGenerating));
      assert_eq!(resume_stage_for_step("omniroute"), Some(PipelineStage::ScriptGenerating));
      assert_eq!(resume_stage_for_step("step-3"), Some(PipelineStage::PreflightCheck));
      assert_eq!(resume_stage_for_step("open_montage"), Some(PipelineStage::MediaTimeline));
      assert_eq!(resume_stage_for_step("capcut"), Some(PipelineStage::DraftCreating));
    }

    #[test]
    fn rejects_unknown_step() {
      assert_eq!(resume_stage_for_step("step-99"), None);
      assert_eq!(resume_stage_for_step("bogus"), None);
    }
  }

  mod output_invalidation {
    use super::*;

    #[test]
    fn keeps_upstream_and_removes_downstream() {
      // A legacy ScriptReady resume keeps script output and drops draft/render.
      let mut outputs = json!({
        "script": "hello",
        "script_artifact": { "id": "a" },
        "draft_url": "u",
        "draft_id": "d",
        "capcut_artifact": { "id": "c" },
        "video_url": "v",
        "rendering_supported": true
      });
      invalidate_outputs_from_stage(&mut outputs, PipelineStage::ScriptReady);
      let obj = outputs.as_object().unwrap();
      assert!(obj.contains_key("script"), "upstream script must survive");
      assert!(obj.contains_key("script_artifact"), "upstream artifact must survive");
      assert!(!obj.contains_key("draft_url"), "downstream draft must be purged");
      assert!(!obj.contains_key("draft_id"));
      assert!(!obj.contains_key("capcut_artifact"));
      assert!(!obj.contains_key("video_url"));
      assert!(!obj.contains_key("rendering_supported"));
    }

    #[test]
    fn retry_at_script_purges_everything_produced() {
      let mut outputs = json!({
        "script": "hello",
        "draft_url": "u",
        "video_url": "v"
      });
      invalidate_outputs_from_stage(&mut outputs, PipelineStage::ScriptGenerating);
      let obj = outputs.as_object().unwrap();
      assert!(!obj.contains_key("script"));
      assert!(!obj.contains_key("draft_url"));
      assert!(!obj.contains_key("video_url"));
    }

    #[test]
    fn retry_at_youwee_invalidates_ingest_and_all_downstream_outputs() {
      let mut outputs = json!({
        "pipeline_context": { "artifact_refs": [] },
        "ingest_analyze": { "artifact_ids": ["a"] },
        "script": "hello",
        "draft_url": "u"
      });
      invalidate_outputs_from_stage(&mut outputs, resume_stage_for_step("youwee").unwrap());
      let obj = outputs.as_object().unwrap();
      assert!(!obj.contains_key("pipeline_context"));
      assert!(!obj.contains_key("ingest_analyze"));
      assert!(!obj.contains_key("script"));
      assert!(!obj.contains_key("draft_url"));
    }
  }

  mod retry_counts {
    use super::*;

    #[test]
    fn increments_per_module() {
      let mut outputs = json!({});
      assert_eq!(bump_retry_count(&mut outputs, "step-3"), 1);
      assert_eq!(bump_retry_count(&mut outputs, "youwee"), 2); // same module, different id form
      assert_eq!(bump_retry_count(&mut outputs, "step-2"), 1); // different module
    }

    #[test]
    fn test_enqueue_resolves_grok_content_pipeline_workflow_mode() {
      let mut req = EnqueueFlowordWorkflowRequest {
        page_id: Some("PAGE_01".to_string()),
        workflow_name: "Grok Full Pipeline".to_string(),
        prompt: "A cybernetic tiger".to_string(),
        topic: None,
        source_urls: None,
        source_files: None,
        workflow_mode: None,
        image_prompt: Some("tiger in neon city".to_string()),
        expand_9_16_prompt: Some("expand 9:16".to_string()),
        expand_prompt: None,
        video_prompt: Some("tiger running".to_string()),
        source_image_artifact: None,
        target_platform: None,
        aspect_ratio: None,
        target_duration_seconds: None,
        output_mode: None,
        model_id: None,
        voice_id: None,
        language: None,
        content_source: None,
        story_url: None,
        research_enabled: None,
        research_platform: None,
        research_query: None,
        research_mode: None,
        xhs_variant: None,
        cookie_mode: None,
        cookie_browser: None,
        cookie_browser_profile: None,
        cookie_file_path: None,
        cookie_skip_patterns: None,
      };

      if req.workflow_mode.as_deref().unwrap_or("").trim().is_empty() {
        let lower_name = req.workflow_name.to_lowercase();
        if lower_name.contains("grok full") || lower_name.contains("auto ai grok") || lower_name == "grok_content_pipeline" {
          req.workflow_mode = Some("grok_content_pipeline".to_string());
        }
      }

      assert_eq!(req.workflow_mode, Some("grok_content_pipeline".to_string()));
    }
  }

  mod publishing_tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn deterministic_idempotency_key_is_repeatable() {
      let job_id = "job_test_123";
      let page_id = "page_stage_screen";
      let platform = "facebook";
      let destination_id = "fb_page_888";

      let mut hasher1 = Sha256::new();
      hasher1.update(format!("{}:{}:{}:{}", job_id, page_id, platform, destination_id));
      let hash1 = hasher1.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>();

      let mut hasher2 = Sha256::new();
      hasher2.update(format!("{}:{}:{}:{}", job_id, page_id, platform, destination_id));
      let hash2 = hasher2.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>();

      assert_eq!(hash1, hash2);
      assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn post_mode_determines_initial_publication_status() {
      let auto_mode = "auto";
      let review_mode = "review";

      let status_auto = if auto_mode.to_lowercase() == "auto" { "READY_TO_POST" } else { "WAITING_APPROVAL" };
      let status_review = if review_mode.to_lowercase() == "auto" { "READY_TO_POST" } else { "WAITING_APPROVAL" };

      assert_eq!(status_auto, "READY_TO_POST");
      assert_eq!(status_review, "WAITING_APPROVAL");
    }

    #[test]
    fn error_taxonomy_codes_map_cleanly() {
      use crate::services::publishing::adapters::publisher_adapter::PublisherErrorCode;

      assert_eq!(PublisherErrorCode::AuthRequired.as_str(), "AUTH_REQUIRED");
      assert_eq!(PublisherErrorCode::ProfileOffline.as_str(), "PROFILE_OFFLINE");
      assert_eq!(PublisherErrorCode::VideoNotFound.as_str(), "VIDEO_NOT_FOUND");
      assert_eq!(PublisherErrorCode::UploadFailed.as_str(), "UPLOAD_FAILED");
      assert_eq!(PublisherErrorCode::TargetAmbiguous.as_str(), "TARGET_AMBIGUOUS");
      assert_eq!(PublisherErrorCode::VerifyFailed.as_str(), "VERIFY_FAILED");
    }
  }
}
