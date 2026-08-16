//! Phase 2 research stage. The Rust worker owns lifecycle/retry/cancel; the
//! Python gateway is only a thin adapter around the real MediaCrawler runtime.

use crate::services::pipeline::contracts::{PipelineContext, PipelineContractError, StageError, StageId, StageState};
use reqwest::{Client, StatusCode};
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_BACKEND_URL: &str = "http://127.0.0.1:30000";
pub const RESEARCH_MAX_ATTEMPTS: u32 = 2;
const RESEARCH_VERIFY_TIMEOUT_SECONDS: u64 = 75;
const RESEARCH_HTTP_TIMEOUT_BUFFER_SECONDS: u64 = 15;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResearchInput {
  pub platform: String,
  pub query: String,
  pub mode: String,
  pub artifact_ids: Vec<String>,
  pub xhs_variant: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResearchPreparation {
  Skipped,
  Ready(ResearchInput),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResearchOutcome {
  Completed(Value),
  WaitingInput { code: String, message: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResearchError {
  pub code: String,
  pub message: String,
  pub retryable: bool,
  pub cancelled: bool,
}

impl ResearchError {
  pub fn auth_required(message: impl Into<String>) -> Self {
    Self { code: "MEDIACRAWLER_AUTH_REQUIRED".to_string(), message: message.into(), retryable: false, cancelled: false }
  }

  fn runtime(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
    Self { code: code.into(), message: message.into(), retryable, cancelled: false }
  }

  fn cancelled() -> Self {
    Self { code: "RESEARCH_CANCELLED".to_string(), message: "MediaCrawler operation cancelled".to_string(), retryable: false, cancelled: true }
  }
}

#[derive(Debug, Serialize)]
struct ResearchRequest<'a> {
  platform: &'a str,
  query: &'a str,
  mode: &'a str,
  input_artifact_ids: &'a [String],
  #[serde(skip_serializing_if = "Option::is_none")]
  xhs_variant: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
  detail: Option<Value>,
}

pub fn prepare_research(context: &mut PipelineContext, finished_at: &str) -> Result<ResearchPreparation, PipelineContractError> {
  if !context.research_enabled {
    let state = research_state_mut(context)?;
    state.skip_stage(finished_at.to_string())?;
    return Ok(ResearchPreparation::Skipped);
  }

  let ingest_ids = context.stage_states.iter().find(|state| state.stage_id == StageId::IngestAnalyze).map(|state| state.output_artifact_ids.clone()).unwrap_or_default();
  for artifact_id in &ingest_ids {
    context.require_artifact_id(StageId::Research, artifact_id)?;
  }
  research_state_mut(context)?.input_artifact_ids = ingest_ids.clone();

  Ok(ResearchPreparation::Ready(ResearchInput {
    platform: context.research_platform.clone().unwrap_or_default(),
    query: context.research_query.clone().unwrap_or_default(),
    mode: context.research_mode.clone().unwrap_or_else(|| "search".to_string()),
    artifact_ids: ingest_ids,
    xhs_variant: context.xhs_variant.clone(),
  }))
}

pub fn research_state_mut(context: &mut PipelineContext) -> Result<&mut StageState, PipelineContractError> {
  context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Research).ok_or(PipelineContractError::InvalidTransition { stage_id: StageId::Research, from: crate::services::pipeline::contracts::StageStatus::Pending, to: crate::services::pipeline::contracts::StageStatus::Running })
}

pub fn should_retry(error: &ResearchError, attempt: u32) -> bool {
  error.retryable && !error.cancelled && attempt < RESEARCH_MAX_ATTEMPTS
}

pub fn stage_error(error: &ResearchError) -> StageError {
  StageError::sanitized(error.code.clone(), &error.message, error.retryable)
}

pub fn research_http_timeout_seconds_from(login_seconds: u64, crawl_seconds: u64) -> u64 {
  RESEARCH_VERIFY_TIMEOUT_SECONDS.saturating_add(login_seconds).saturating_add(crawl_seconds).saturating_add(RESEARCH_HTTP_TIMEOUT_BUFFER_SECONDS)
}

pub async fn run_mediacrawler(input: &ResearchInput, cancel_flag: Arc<AtomicBool>) -> Result<ResearchOutcome, ResearchError> {
  if input.query.trim().is_empty() {
    return Err(ResearchError::runtime("MEDIACRAWLER_QUERY_INVALID", "Research query is empty", false));
  }
  if input.platform.trim().is_empty() {
    return Err(ResearchError::runtime("MEDIACRAWLER_PLATFORM_UNSUPPORTED", "Research platform is required", false));
  }
  let base_url = env::var("FLOWORD_BACKEND_URL").unwrap_or_else(|_| DEFAULT_BACKEND_URL.to_string());
  let url = format!("{}/api/research/operation", base_url.trim_end_matches('/'));
  let stop_url = format!("{}/api/research/crawler/stop", base_url.trim_end_matches('/'));
  let crawl_timeout_secs = env::var("RESEARCH_TIMEOUT_SECONDS").ok().and_then(|value| value.parse::<u64>().ok()).unwrap_or(180);
  let login_timeout_secs = env::var("RESEARCH_LOGIN_TIMEOUT_SECONDS").ok().and_then(|value| value.parse::<u64>().ok()).unwrap_or(180);
  let request_timeout_secs = research_http_timeout_seconds_from(login_timeout_secs, crawl_timeout_secs);
  let client = Client::builder().timeout(Duration::from_secs(request_timeout_secs)).build().map_err(|error| ResearchError::runtime("MEDIACRAWLER_UNAVAILABLE", error.to_string(), true))?;
  let request = ResearchRequest {
    platform: &input.platform,
    query: &input.query,
    mode: &input.mode,
    input_artifact_ids: &input.artifact_ids,
    xhs_variant: input.xhs_variant.as_deref(),
  };

  let response = tokio::select! {
    response = client.post(&url).json(&request).send() => response,
    _ = wait_for_cancel(&cancel_flag) => {
      let _ = client.post(&stop_url).send().await;
      return Err(ResearchError::cancelled());
    }
  }
  .map_err(|error| if error.is_timeout() { ResearchError::runtime("MEDIACRAWLER_TIMEOUT", "MediaCrawler operation timed out", true) } else { ResearchError::runtime("MEDIACRAWLER_UNAVAILABLE", format!("MediaCrawler connection failed: {error}"), true) })?;

  let status = response.status();
  let text = response.text().await.map_err(|error| ResearchError::runtime("MEDIACRAWLER_UNAVAILABLE", format!("Failed to read MediaCrawler response: {error}"), true))?;

  if status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::UNAUTHORIZED || error_code(&text).as_deref() == Some("RESEARCH_AUTH_REQUIRED") {
    let message = error_message(&text).unwrap_or_else(|| "MediaCrawler session required".to_string());
    return Err(ResearchError::auth_required(message));
  }

  if !status.is_success() {
    let code = error_code(&text).unwrap_or_else(|| "MEDIACRAWLER_UNAVAILABLE".to_string());
    let message = error_message(&text).unwrap_or_else(|| format!("MediaCrawler returned status {status}"));
    return Err(ResearchError::runtime(code, message, status.is_server_error()));
  }

  let value: Value = serde_json::from_str(&text).map_err(|error| ResearchError::runtime("MEDIACRAWLER_PAYLOAD_INVALID", format!("Invalid MediaCrawler response JSON: {error}"), false))?;
  if value.get("status").and_then(Value::as_str).is_some_and(|status| status.eq_ignore_ascii_case("waiting_input")) {
    let code = value.get("code").and_then(Value::as_str).unwrap_or("RESEARCH_AUTH_REQUIRED").to_string();
    let message = value.get("message").and_then(Value::as_str).unwrap_or("Waiting for interactive research authentication").to_string();
    return Ok(ResearchOutcome::WaitingInput { code, message });
  }
  Ok(ResearchOutcome::Completed(value))
}

async fn wait_for_cancel(cancel_flag: &Arc<AtomicBool>) {
  while !cancel_flag.load(Ordering::SeqCst) {
    tokio::time::sleep(Duration::from_millis(100)).await;
  }
}

fn error_message(body: &str) -> Option<String> {
  serde_json::from_str::<ErrorEnvelope>(body).ok().and_then(|envelope| envelope.detail).and_then(|detail| detail.get("message").and_then(Value::as_str).map(str::to_string))
}

fn error_code(body: &str) -> Option<String> {
  serde_json::from_str::<ErrorEnvelope>(body).ok().and_then(|envelope| envelope.detail).and_then(|detail| detail.get("code").and_then(Value::as_str).map(str::to_string))
}

pub fn canonical_metadata(input: &ResearchInput) -> Value {
  json!({
    "platform": input.platform,
    "query": input.query,
    "mode": input.mode,
    "input_artifact_ids": input.artifact_ids,
    "xhs_variant": input.xhs_variant.as_deref().unwrap_or("mainland"),
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef, ContentSource, StageStatus};
  use std::fs;

  fn context(research_enabled: bool, artifacts: Vec<ArtifactRef>) -> PipelineContext {
    PipelineContext {
      job_id: "job-research".to_string(),
      project_id: None,
      workflow_mode: "source_based".to_string(),
      content_source: Some(ContentSource::LocalMedia.as_str().to_string()),
      prompt: "xu huong video ngan".to_string(),
      model_id: None,
      voice_id: None,
      language: "vi".to_string(),
      target_duration_seconds: 30,
      output_mode: "draft_only".to_string(),
      source_url: None,
      local_file: Some("fixture.mp4".to_string()),
      story_url: None,
      research_enabled,
      research_platform: Some("xhs".to_string()),
      research_query: Some("xu huong video ngan".to_string()),
      research_mode: Some("search".to_string()),
      xhs_variant: None,
      artifact_refs: artifacts,
      stage_states: PipelineContext::initial_stage_states(),
    }
  }

  #[test]
  fn disabled_research_is_skipped_without_runtime_request() {
    let mut context = context(false, Vec::new());
    let outcome = prepare_research(&mut context, "finished").unwrap();
    assert_eq!(outcome, ResearchPreparation::Skipped);
    assert_eq!(context.stage_states.iter().find(|state| state.stage_id == StageId::Research).unwrap().status, StageStatus::Skipped);
  }

  #[test]
  fn enabled_research_resolves_ingest_inputs_by_artifact_id() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("scenes.json");
    fs::write(&path, br#"{"scenes":[]}"#).unwrap();
    let artifact = ArtifactRef { artifact_id: "art-scenes".to_string(), kind: ArtifactKind::Scenes, produced_by_stage: StageId::IngestAnalyze, location: path.to_string_lossy().to_string(), mime_type: Some("application/json".to_string()), metadata: Value::Null };
    let mut context = context(true, vec![artifact]);
    context.stage_states.iter_mut().find(|state| state.stage_id == StageId::IngestAnalyze).unwrap().output_artifact_ids = vec!["art-scenes".to_string()];
    let ResearchPreparation::Ready(input) = prepare_research(&mut context, "finished").unwrap() else {
      panic!("research should be ready")
    };
    assert_eq!(input.artifact_ids, vec!["art-scenes"]);
    assert_eq!(input.platform, "xhs");
    assert_eq!(input.query, "xu huong video ngan");
    assert_eq!(input.mode, "search");
  }

  #[test]
  fn enabled_research_without_source_or_ingest_artifacts_is_ready() {
    let mut context = context(true, Vec::new());
    context.local_file = None;
    let ResearchPreparation::Ready(input) = prepare_research(&mut context, "finished").unwrap() else {
      panic!("research should be independent of source ingest")
    };
    assert!(input.artifact_ids.is_empty());
    assert_eq!(input.platform, "xhs");
  }

  #[test]
  fn auth_required_is_terminal_and_never_retried() {
    let error = ResearchError::auth_required("login required");
    assert_eq!(error.code, "MEDIACRAWLER_AUTH_REQUIRED");
    assert!(!error.retryable);
    assert!(!should_retry(&error, 1));
  }

  #[test]
  fn e2e_research_artifact_flow_registers_in_artifact_store_and_hands_off_to_story_script() {
    use crate::services::pipeline::artifact_store::ArtifactStore;
    use crate::services::pipeline::story_script::prepare_story_script;

    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join("job-e2e-research");
    fs::create_dir_all(&work_dir).unwrap();

    let mut ctx = PipelineContext {
      job_id: "job-e2e-research".to_string(),
      project_id: None,
      workflow_mode: "original".to_string(),
      content_source: Some(ContentSource::TrendResearch.as_str().to_string()),
      prompt: "AI video editing".to_string(),
      model_id: None,
      voice_id: None,
      language: "vi".to_string(),
      target_duration_seconds: 30,
      output_mode: "draft_only".to_string(),
      source_url: None,
      local_file: None,
      story_url: None,
      research_enabled: true,
      research_platform: Some("xhs".to_string()),
      research_query: Some("AI video editing".to_string()),
      research_mode: Some("search".to_string()),
      xhs_variant: None,
      artifact_refs: Vec::new(),
      stage_states: PipelineContext::initial_stage_states(),
    };

    // 1. Prepare research stage: should be Ready without source inputs
    let prep = prepare_research(&mut ctx, "2026-08-10T18:00:00Z").unwrap();
    let ResearchPreparation::Ready(input) = prep else {
      panic!("Research must be ready for source-independent flow");
    };
    assert_eq!(input.platform, "xhs");
    assert_eq!(input.query, "AI video editing");
    assert_eq!(input.mode, "search");
    assert!(input.artifact_ids.is_empty());

    // 2. Simulate research completion and write canonical research.json
    let research_dir = work_dir.join("research");
    fs::create_dir_all(&research_dir).unwrap();
    let research_file = research_dir.join("research.json");
    let sample_payload = json!({
      "status": "completed",
      "service": "mediacrawler",
      "platform": "xhs",
      "query": "AI video editing",
      "mode": "search",
      "input_artifact_ids": [],
      "files": ["data/xhs/json/search_contents.json"],
      "record_count": 20,
      "records": [{"note_id": "test_note_1", "title": "AI Video Editing"}],
      "enrichment": {
        "status": "partial",
        "comments_enabled": true,
        "reason": "comments_timeout"
      }
    });
    fs::write(&research_file, serde_json::to_string_pretty(&sample_payload).unwrap()).unwrap();

    // 3. Register artifact in canonical ArtifactStore
    let stored = ArtifactStore::register_typed_artifact(
      &work_dir,
      &ctx.job_id,
      StageId::Research,
      "mediacrawler",
      ArtifactKind::Research,
      &research_file,
      canonical_metadata(&input),
    )
    .expect("ArtifactStore must register valid research artifact");

    assert_eq!(stored.artifact_type, "research");
    assert_eq!(stored.producer, "mediacrawler");
    assert!(stored.size_bytes > 0);
    assert!(!stored.sha256.is_empty());

    let artifact_ref = stored.to_artifact_ref(StageId::Research).unwrap();
    ctx.artifact_refs.push(artifact_ref.clone());

    let stage = research_state_mut(&mut ctx).unwrap();
    stage.start_stage(Some("mediacrawler".to_string()), "2026-08-10T18:01:00Z").unwrap();
    stage.complete_stage(vec![artifact_ref.artifact_id.clone()], "2026-08-10T18:02:00Z").unwrap();

    // 4. Verify handoff to next stage (StoryScript)
    let story_input = prepare_story_script(&ctx).expect("StoryScript must accept completed research artifact");
    assert_eq!(story_input.prompt, "AI video editing");
    assert_eq!(story_input.workflow_mode, "original");
    assert!(story_input.input_artifact_ids.contains(&artifact_ref.artifact_id));
    assert!(story_input.research.is_some());
    let r = story_input.research.unwrap();
    assert_eq!(r.get("status").and_then(Value::as_str), Some("completed"));
    assert_eq!(r.get("record_count").and_then(Value::as_u64), Some(20));
    assert_eq!(
      r.get("enrichment").and_then(|e| e.get("status")).and_then(Value::as_str),
      Some("partial")
    );
  }

  #[test]
  fn xhs_mainland_is_default_variant() {
    let mut ctx = context(true, Vec::new());
    ctx.xhs_variant = None;
    let prep = prepare_research(&mut ctx, "2026-08-10T18:00:00Z").unwrap();
    let ResearchPreparation::Ready(input) = prep else {
      panic!("Research must be ready");
    };
    assert_eq!(input.xhs_variant, None);
    let meta = canonical_metadata(&input);
    assert_eq!(meta.get("xhs_variant").and_then(Value::as_str), Some("mainland"));
  }

  #[test]
  fn xhs_international_variant_is_preserved() {
    let mut ctx = context(true, Vec::new());
    ctx.xhs_variant = Some("international".to_string());
    let prep = prepare_research(&mut ctx, "2026-08-10T18:00:00Z").unwrap();
    let ResearchPreparation::Ready(input) = prep else {
      panic!("Research must be ready");
    };
    assert_eq!(input.xhs_variant, Some("international".to_string()));
    let meta = canonical_metadata(&input);
    assert_eq!(meta.get("xhs_variant").and_then(Value::as_str), Some("international"));
  }

  #[test]
  fn http_timeout_covers_interactive_login_and_crawl() {
    assert_eq!(research_http_timeout_seconds_from(180, 180), 450);
  }
}
