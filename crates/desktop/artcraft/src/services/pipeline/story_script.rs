use crate::services::pipeline::contracts::{ArtifactKind, PipelineContext, PipelineContractError, StageId, StageStatus};
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_STORY_STUDIO_BASE_URL: &str = "http://127.0.0.1:4569";
pub const STORY_SCRIPT_MAX_ATTEMPTS: u32 = 2;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryScriptInput {
  pub prompt: String,
  pub language: String,
  pub target_duration_seconds: u32,
  pub model_id: Option<String>,
  pub input_artifact_ids: Vec<String>,
  pub source_metadata: Value,
  pub scenes: Value,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub research: Option<Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub source_text: Option<Value>,
  pub workflow_mode: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryStudioOutput {
  pub story: Value,
  pub script_request: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoryScriptError {
  pub code: String,
  pub message: String,
  pub retryable: bool,
  pub cancelled: bool,
}

impl StoryScriptError {
  pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
    Self { code: code.into(), message: message.into(), retryable, cancelled: false }
  }

  fn cancelled() -> Self {
    Self { code: "STORY_SCRIPT_CANCELLED".to_string(), message: "Story script stage cancelled".to_string(), retryable: false, cancelled: true }
  }
}

pub fn prepare_story_script(context: &PipelineContext) -> Result<StoryScriptInput, PipelineContractError> {
  let mut input_artifact_ids = Vec::new();
  let mut source_metadata = serde_json::json!({});
  let mut scenes = serde_json::json!({});
  let mut source_text = None;

  let ingest_state = context.stage_states.iter().find(|state| state.stage_id == StageId::IngestAnalyze);
  if let Some(state) = ingest_state {
    if state.status == StageStatus::Completed {
      // Check for source_text (Web Story)
      if let Some(text_ref) = context.artifact_refs.iter().find(|a| a.kind == ArtifactKind::SourceText) {
        text_ref.validate()?;
        input_artifact_ids.push(text_ref.artifact_id.clone());
        source_text = Some(read_json(text_ref)?);
      }
      // Check for source_metadata & scenes (Video Ingest)
      if let Some(meta_ref) = context.artifact_refs.iter().find(|a| a.kind == ArtifactKind::SourceMetadata) {
        if meta_ref.validate().is_ok() {
          input_artifact_ids.push(meta_ref.artifact_id.clone());
          source_metadata = read_json(meta_ref).unwrap_or(serde_json::json!({}));
        }
      }
      if let Some(scenes_ref) = context.artifact_refs.iter().find(|a| a.kind == ArtifactKind::Scenes) {
        if scenes_ref.validate().is_ok() {
          input_artifact_ids.push(scenes_ref.artifact_id.clone());
          scenes = read_json(scenes_ref).unwrap_or(serde_json::json!({}));
        }
      }
    }
  }

  let research_state = context.stage_states.iter().find(|state| state.stage_id == StageId::Research);
  let research = match research_state.map(|state| state.status) {
    Some(StageStatus::Completed) => {
      let id = research_state.and_then(|state| state.output_artifact_ids.first()).ok_or(PipelineContractError::MissingArtifact { stage_id: StageId::StoryScript, kind: ArtifactKind::Research })?;
      let artifact = context.require_artifact_id(StageId::StoryScript, id)?;
      if artifact.kind != ArtifactKind::Research {
        return Err(PipelineContractError::MissingArtifact { stage_id: StageId::StoryScript, kind: ArtifactKind::Research });
      }
      input_artifact_ids.push(id.clone());
      Some(read_json(artifact)?)
    },
    Some(StageStatus::Skipped) => None,
    _ => None,
  };

  Ok(StoryScriptInput { prompt: context.prompt.clone(), language: context.language.clone(), target_duration_seconds: context.target_duration_seconds, model_id: context.model_id.clone(), input_artifact_ids, source_metadata, scenes, research, source_text, workflow_mode: context.workflow_mode.clone() })
}

fn read_json(artifact: &crate::services::pipeline::contracts::ArtifactRef) -> Result<Value, PipelineContractError> {
  let bytes = std::fs::read(&artifact.location).map_err(|error| PipelineContractError::InvalidArtifact { artifact_id: artifact.artifact_id.clone(), message: error.to_string() })?;
  serde_json::from_slice(&bytes).map_err(|error| PipelineContractError::InvalidArtifact { artifact_id: artifact.artifact_id.clone(), message: error.to_string() })
}

pub async fn run_story_studio(input: &StoryScriptInput, cancel_flag: Arc<AtomicBool>) -> Result<StoryStudioOutput, StoryScriptError> {
  let url = story_studio_url();
  let client = Client::builder().timeout(Duration::from_secs(60)).build().map_err(|error| StoryScriptError::new("STORY_STUDIO_UNAVAILABLE", error.to_string(), true))?;
  let response = tokio::select! {
    result = client.post(url).json(input).send() => result,
    _ = wait_for_cancel(&cancel_flag) => return Err(StoryScriptError::cancelled()),
  }
  .map_err(|error| StoryScriptError::new(if error.is_timeout() { "STORY_STUDIO_TIMEOUT" } else { "STORY_STUDIO_UNAVAILABLE" }, error.to_string(), true))?;
  let status = response.status();
  let body = response.text().await.unwrap_or_default();
  if !status.is_success() {
    return Err(StoryScriptError::new(if status.is_server_error() { "STORY_STUDIO_UNAVAILABLE" } else { "STORY_INPUT_INVALID" }, format!("Story Studio HTTP {}: {body}", status.as_u16()), status.is_server_error()));
  }
  let output: StoryStudioOutput = serde_json::from_str(&body).map_err(|error| StoryScriptError::new("STORY_STUDIO_INVALID_RESPONSE", error.to_string(), false))?;
  validate_story_output(&output)?;
  Ok(output)
}

fn story_studio_url() -> String {
  let base_url = env::var("STORY_STUDIO_BASE_URL").or_else(|_| env::var("INKOS_BASE_URL")).unwrap_or_else(|_| DEFAULT_STORY_STUDIO_BASE_URL.to_string());
  format!("{}/api/v1/floword/story-plan", base_url.trim_end_matches('/'))
}

pub fn validate_story_output(output: &StoryStudioOutput) -> Result<(), StoryScriptError> {
  let beats = output.story.get("beats").and_then(Value::as_array);
  let messages = output.script_request.get("messages").and_then(Value::as_array);
  if beats.map(|value| value.is_empty()).unwrap_or(true) || messages.map(|value| value.is_empty()).unwrap_or(true) || output.script_request.get("executor").and_then(Value::as_str) != Some("omniroute") {
    return Err(StoryScriptError::new("STORY_STUDIO_INVALID_RESPONSE", "Story Studio did not return a valid story and OmniRoute request", false));
  }
  Ok(())
}

pub fn should_retry(error: &StoryScriptError, attempt: u32) -> bool {
  error.retryable && !error.cancelled && attempt < STORY_SCRIPT_MAX_ATTEMPTS
}

async fn wait_for_cancel(flag: &Arc<AtomicBool>) {
  while !flag.load(Ordering::SeqCst) {
    tokio::time::sleep(Duration::from_millis(100)).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef, ContentSource, PipelineContext, StageId, StageStatus};
  use std::fs;

  fn artifact(root: &std::path::Path, id: &str, kind: ArtifactKind, body: &[u8], stage: StageId) -> ArtifactRef {
    let path = root.join(format!("{id}.json"));
    fs::write(&path, body).unwrap();
    ArtifactRef { artifact_id: id.to_string(), kind, produced_by_stage: stage, location: path.to_string_lossy().to_string(), mime_type: Some("application/json".to_string()), metadata: Value::Null }
  }

  fn context(artifacts: Vec<ArtifactRef>, research: StageStatus) -> PipelineContext {
    let content_source = if research == StageStatus::Skipped { ContentSource::PromptOnly } else { ContentSource::TrendResearch };
    let mut context = PipelineContext { job_id: "job-story".to_string(), project_id: None, workflow_mode: "source_based".to_string(), content_source: Some(content_source.as_str().to_string()), prompt: "Kể lại video".to_string(), model_id: Some("auto".to_string()), voice_id: None, language: "vi".to_string(), target_duration_seconds: 30, output_mode: "draft_only".to_string(), source_url: None, local_file: None, story_url: None, research_enabled: research != StageStatus::Skipped, research_platform: Some("xhs".into()), research_query: Some("trend".into()), research_mode: Some("search".into()), xhs_variant: None, artifact_refs: artifacts, stage_states: PipelineContext::initial_stage_states() };
    let ingest = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::IngestAnalyze).unwrap();
    ingest.status = StageStatus::Completed;
    ingest.output_artifact_ids = vec!["metadata".to_string(), "scenes".to_string()];
    let research_state = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Research).unwrap();
    research_state.status = research;
    if research == StageStatus::Completed {
      research_state.output_artifact_ids = vec!["research".to_string()]
    }
    context
  }

  #[test]
  fn skipped_research_still_prepares_story_inputs_by_artifact_id() {
    let temp = tempfile::tempdir().unwrap();
    let metadata = artifact(temp.path(), "metadata", ArtifactKind::SourceMetadata, br#"{"probe":{"duration_seconds":10}}"#, StageId::IngestAnalyze);
    let scenes = artifact(temp.path(), "scenes", ArtifactKind::Scenes, br#"{"scenes":[{"index":0,"start_seconds":0,"end_seconds":10}]}"#, StageId::IngestAnalyze);
    let input = prepare_story_script(&context(vec![metadata, scenes], StageStatus::Skipped)).unwrap();
    assert_eq!(input.input_artifact_ids, vec!["metadata", "scenes"]);
    assert!(input.research.is_none());
  }

  #[test]
  fn completed_research_is_included_in_story_input() {
    let temp = tempfile::tempdir().unwrap();
    let metadata = artifact(temp.path(), "metadata", ArtifactKind::SourceMetadata, br#"{"probe":{"duration_seconds":10}}"#, StageId::IngestAnalyze);
    let scenes = artifact(temp.path(), "scenes", ArtifactKind::Scenes, br#"{"scenes":[{"index":0,"start_seconds":0,"end_seconds":10}]}"#, StageId::IngestAnalyze);
    let research = artifact(temp.path(), "research", ArtifactKind::Research, br#"{"records":[{"title":"trend"}]}"#, StageId::Research);
    let input = prepare_story_script(&context(vec![metadata, scenes, research], StageStatus::Completed)).unwrap();
    assert_eq!(input.input_artifact_ids, vec!["metadata", "scenes", "research"]);
    assert!(input.research.is_some());
  }

  #[test]
  fn story_studio_uses_the_real_inkos_floword_route() {
    assert_eq!(story_studio_url(), "http://127.0.0.1:4569/api/v1/floword/story-plan");
  }

  #[test]
  fn original_creation_prepares_story_without_source_artifacts() {
    let mut context = context(Vec::new(), StageStatus::Skipped);
    context.workflow_mode = "original".to_string();
    context.prompt = "Tạo video từ prompt".to_string();
    context.stage_states.iter_mut().find(|state| state.stage_id == StageId::IngestAnalyze).unwrap().status = StageStatus::Skipped;
    let input = prepare_story_script(&context).unwrap();
    assert!(input.input_artifact_ids.is_empty());
    assert_eq!(input.source_metadata, serde_json::json!({}));
    assert_eq!(input.scenes, serde_json::json!({}));
  }

  #[test]
  fn web_story_prepares_story_with_source_text() {
    let temp = tempfile::tempdir().unwrap();
    let source_text = artifact(temp.path(), "source_text", ArtifactKind::SourceText, br#"{"title":"AI News","text":"Article content here"}"#, StageId::IngestAnalyze);
    let mut ctx = context(vec![source_text], StageStatus::Skipped);
    ctx.stage_states.iter_mut().find(|state| state.stage_id == StageId::IngestAnalyze).unwrap().output_artifact_ids = vec!["source_text".to_string()];
    let input = prepare_story_script(&ctx).unwrap();
    assert_eq!(input.input_artifact_ids, vec!["source_text"]);
    assert!(input.source_text.is_some());
    assert_eq!(input.source_text.unwrap()["title"], "AI News");
  }
}
