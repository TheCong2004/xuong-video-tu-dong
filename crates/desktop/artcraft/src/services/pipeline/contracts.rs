//! Typed business-stage contracts for the canonical Floword Rust pipeline.
//!
//! These types complement the existing low-level `PipelineStage` worker state;
//! they do not create another orchestrator or persistence layer.

use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageId {
  Input,
  IngestAnalyze,
  Research,
  StoryScript,
  Voice,
  MediaTimeline,
  Capcut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StageDependency {
  pub stage_id: StageId,
  pub required: bool,
}

impl StageId {
  pub const ALL: [Self; 7] = [Self::Input, Self::IngestAnalyze, Self::Research, Self::StoryScript, Self::Voice, Self::MediaTimeline, Self::Capcut];

  pub const fn dependencies(self) -> &'static [StageDependency] {
    const NONE: &[StageDependency] = &[];
    const INPUT: &[StageDependency] = &[StageDependency { stage_id: StageId::Input, required: true }];
    const STORY: &[StageDependency] = &[StageDependency { stage_id: StageId::StoryScript, required: true }];
    const VOICE: &[StageDependency] = &[StageDependency { stage_id: StageId::Voice, required: true }];
    const MEDIA_TIMELINE: &[StageDependency] = &[StageDependency { stage_id: StageId::MediaTimeline, required: true }];
    const STORY_INPUTS: &[StageDependency] = &[StageDependency { stage_id: StageId::IngestAnalyze, required: true }, StageDependency { stage_id: StageId::Research, required: false }];

    match self {
      Self::Input => NONE,
      Self::IngestAnalyze => INPUT,
      Self::Research => INPUT,
      Self::StoryScript => STORY_INPUTS,
      Self::Voice => STORY,
      Self::MediaTimeline => VOICE,
      Self::Capcut => MEDIA_TIMELINE,
    }
  }
}

impl Display for StageId {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(match self {
      Self::Input => "input",
      Self::IngestAnalyze => "ingest_analyze",
      Self::Research => "research",
      Self::StoryScript => "story_script",
      Self::Voice => "voice",
      Self::MediaTimeline => "media_timeline",
      Self::Capcut => "capcut",
    })
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
  Pending,
  Running,
  WaitingInput,
  Retrying,
  Completed,
  Skipped,
  Failed,
  Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StageError {
  pub code: String,
  pub message: String,
  pub retryable: bool,
  /// Populated by `StageState::fail_stage`; optional only so legacy persisted
  /// jobs created before structured stage observability still deserialize.
  #[serde(default)]
  pub service: Option<String>,
  #[serde(default)]
  pub stage_id: Option<StageId>,
  #[serde(default)]
  pub timestamp: Option<String>,
}

impl StageError {
  pub fn sanitized(code: impl Into<String>, message: impl AsRef<str>, retryable: bool) -> Self {
    let mut safe = message.as_ref().replace(['\r', '\n'], " ");
    for marker in ["Bearer ", "api_key=", "apikey=", "token="] {
      if let Some(start) = safe.to_ascii_lowercase().find(&marker.to_ascii_lowercase()) {
        let value_start = start + marker.len();
        let value_len = safe[value_start..].find(char::is_whitespace).unwrap_or(safe.len() - value_start);
        safe.replace_range(value_start..value_start + value_len, "[REDACTED]");
      }
    }
    if safe.chars().count() > 512 {
      safe = safe.chars().take(509).collect::<String>() + "...";
    }
    Self { code: code.into(), message: safe, retryable, service: None, stage_id: None, timestamp: None }
  }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StageState {
  pub stage_id: StageId,
  pub status: StageStatus,
  pub attempt: u32,
  pub started_at: Option<String>,
  pub finished_at: Option<String>,
  pub input_artifact_ids: Vec<String>,
  pub output_artifact_ids: Vec<String>,
  pub service: Option<String>,
  pub error: Option<StageError>,
}

impl StageState {
  pub fn pending(stage_id: StageId) -> Self {
    Self { stage_id, status: StageStatus::Pending, attempt: 0, started_at: None, finished_at: None, input_artifact_ids: Vec::new(), output_artifact_ids: Vec::new(), service: None, error: None }
  }

  pub fn start_stage(&mut self, service: Option<String>, started_at: impl Into<String>) -> Result<(), PipelineContractError> {
    if !matches!(self.status, StageStatus::Pending | StageStatus::Retrying) {
      return Err(self.invalid_transition(StageStatus::Running));
    }
    self.status = StageStatus::Running;
    self.attempt += 1;
    self.started_at = Some(started_at.into());
    self.finished_at = None;
    self.service = service;
    self.error = None;
    Ok(())
  }

  pub fn complete_stage(&mut self, output_artifact_ids: Vec<String>, finished_at: impl Into<String>) -> Result<(), PipelineContractError> {
    if self.status != StageStatus::Running {
      return Err(self.invalid_transition(StageStatus::Completed));
    }
    self.status = StageStatus::Completed;
    self.output_artifact_ids = output_artifact_ids;
    self.finished_at = Some(finished_at.into());
    self.error = None;
    Ok(())
  }

  pub fn wait_for_input(&mut self, mut action: StageError, observed_at: impl Into<String>) -> Result<(), PipelineContractError> {
    if self.status != StageStatus::Running {
      return Err(self.invalid_transition(StageStatus::WaitingInput));
    }
    action.service = self.service.clone();
    action.stage_id = Some(self.stage_id);
    action.timestamp = Some(observed_at.into());
    self.status = StageStatus::WaitingInput;
    self.finished_at = None;
    self.error = Some(action);
    Ok(())
  }

  pub fn skip_stage(&mut self, finished_at: impl Into<String>) -> Result<(), PipelineContractError> {
    if self.status != StageStatus::Pending {
      return Err(self.invalid_transition(StageStatus::Skipped));
    }
    self.status = StageStatus::Skipped;
    self.finished_at = Some(finished_at.into());
    Ok(())
  }

  pub fn fail_stage(&mut self, mut error: StageError, finished_at: impl Into<String>) -> Result<(), PipelineContractError> {
    if !matches!(self.status, StageStatus::Running | StageStatus::Retrying) {
      return Err(self.invalid_transition(StageStatus::Failed));
    }
    let timestamp = finished_at.into();
    error.service = self.service.clone();
    error.stage_id = Some(self.stage_id);
    error.timestamp = Some(timestamp.clone());
    self.status = StageStatus::Failed;
    self.finished_at = Some(timestamp);
    self.error = Some(error);
    Ok(())
  }

  pub fn retry_stage(&mut self) -> Result<(), PipelineContractError> {
    if self.status != StageStatus::Failed {
      return Err(self.invalid_transition(StageStatus::Retrying));
    }
    let error = self.error.as_ref().ok_or_else(|| PipelineContractError::NonRetryable { stage_id: self.stage_id, code: "MISSING_STAGE_ERROR".to_string() })?;
    if !error.retryable {
      return Err(PipelineContractError::NonRetryable { stage_id: self.stage_id, code: error.code.clone() });
    }
    self.status = StageStatus::Retrying;
    self.finished_at = None;
    Ok(())
  }

  pub fn cancel_stage(&mut self, finished_at: impl Into<String>) -> Result<(), PipelineContractError> {
    if !matches!(self.status, StageStatus::Pending | StageStatus::Running | StageStatus::WaitingInput | StageStatus::Retrying) {
      return Err(self.invalid_transition(StageStatus::Cancelled));
    }
    self.status = StageStatus::Cancelled;
    self.finished_at = Some(finished_at.into());
    Ok(())
  }

  fn invalid_transition(&self, to: StageStatus) -> PipelineContractError {
    PipelineContractError::InvalidTransition { stage_id: self.stage_id, from: self.status, to }
  }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
  SourceVideo,
  SourceMetadata,
  SourceAudio,
  SourceText,
  Scenes,
  Research,
  Story,
  ScriptRequest,
  Script,
  ScenePlan,
  GeneratedVideo,
  GeneratedImage,
  VoiceAudio,
  VoiceTiming,
  Captions,
  Timeline,
  CapcutDraft,
  RenderedVideo,
}

impl ArtifactKind {
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::SourceVideo => "source_video",
      Self::SourceMetadata => "source_metadata",
      Self::SourceAudio => "source_audio",
      Self::SourceText => "source_text",
      Self::Scenes => "scenes",
      Self::Research => "research",
      Self::Story => "story",
      Self::ScriptRequest => "script_request",
      Self::Script => "script",
      Self::ScenePlan => "scene_plan",
      Self::GeneratedVideo => "generated_video",
      Self::GeneratedImage => "generated_image",
      Self::VoiceAudio => "voice_audio",
      Self::VoiceTiming => "voice_timing",
      Self::Captions => "captions",
      Self::Timeline => "timeline",
      Self::CapcutDraft => "capcut_draft",
      Self::RenderedVideo => "rendered_video",
    }
  }

  const fn requires_json(self) -> bool {
    matches!(self, Self::SourceMetadata | Self::SourceText | Self::Scenes | Self::Research | Self::Story | Self::ScriptRequest | Self::Script | Self::ScenePlan | Self::VoiceTiming | Self::Captions | Self::Timeline | Self::CapcutDraft)
  }
}

impl Display for ArtifactKind {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(self.as_str())
  }
}

impl FromStr for ArtifactKind {
  type Err = PipelineContractError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    match value {
      "source_video" => Ok(Self::SourceVideo),
      "source_metadata" => Ok(Self::SourceMetadata),
      "source_audio" => Ok(Self::SourceAudio),
      "source_text" => Ok(Self::SourceText),
      "scenes" => Ok(Self::Scenes),
      "research" => Ok(Self::Research),
      "story" => Ok(Self::Story),
      "script_request" => Ok(Self::ScriptRequest),
      "script" => Ok(Self::Script),
      "scene_plan" => Ok(Self::ScenePlan),
      "generated_video" => Ok(Self::GeneratedVideo),
      "generated_image" => Ok(Self::GeneratedImage),
      "voice_audio" => Ok(Self::VoiceAudio),
      "voice_timing" => Ok(Self::VoiceTiming),
      "captions" => Ok(Self::Captions),
      "timeline" => Ok(Self::Timeline),
      "capcut_draft" => Ok(Self::CapcutDraft),
      "rendered_video" => Ok(Self::RenderedVideo),
      _ => Err(PipelineContractError::UnknownArtifactKind(value.to_string())),
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentSource {
  Auto,
  PromptOnly,
  TrendResearch,
  WebStory,
  VideoUrl,
  LocalMedia,
}

impl ContentSource {
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Auto => "auto",
      Self::PromptOnly => "prompt_only",
      Self::TrendResearch => "trend_research",
      Self::WebStory => "web_story",
      Self::VideoUrl => "video_url",
      Self::LocalMedia => "local_media",
    }
  }

  pub fn parse_or_default(value: Option<&str>) -> Self {
    match value.map(|s| s.trim()).filter(|s| !s.is_empty()) {
      Some("prompt_only") => Self::PromptOnly,
      Some("trend_research") => Self::TrendResearch,
      Some("web_story") => Self::WebStory,
      Some("video_url") => Self::VideoUrl,
      Some("local_media") => Self::LocalMedia,
      _ => Self::Auto,
    }
  }

  /// Deterministically resolve auto mode based on provided inputs
  pub fn resolve(
    declared: Option<&str>,
    local_file: Option<&str>,
    source_url: Option<&str>,
    story_url: Option<&str>,
    research_enabled: bool,
    research_query: Option<&str>,
  ) -> Self {
    let mode = Self::parse_or_default(declared);
    if mode != Self::Auto {
      return mode;
    }

    if local_file.is_some_and(|p| !p.trim().is_empty()) {
      return Self::LocalMedia;
    }

    if story_url.is_some_and(|u| !u.trim().is_empty()) {
      return Self::WebStory;
    }

    if let Some(url) = source_url.map(|u| u.trim()).filter(|u| !u.is_empty()) {
      if is_direct_video_or_media_url(url) {
        return Self::VideoUrl;
      }
      return Self::WebStory;
    }

    if research_enabled && research_query.is_some_and(|q| !q.trim().is_empty()) {
      return Self::TrendResearch;
    }

    Self::PromptOnly
  }
}

pub fn is_direct_video_or_media_url(url: &str) -> bool {
  let lower = url.to_ascii_lowercase();
  lower.contains("youtube.com")
    || lower.contains("youtu.be")
    || lower.contains("tiktok.com")
    || lower.contains("douyin.com")
    || lower.contains("bilibili.com")
    || lower.contains("kuaishou.com")
    || lower.contains("instagram.com/reel")
    || lower.contains("instagram.com/p/")
    || lower.ends_with(".mp4")
    || lower.ends_with(".mov")
    || lower.ends_with(".webm")
    || lower.ends_with(".mkv")
    || lower.ends_with(".m3u8")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRef {
  pub artifact_id: String,
  pub kind: ArtifactKind,
  pub produced_by_stage: StageId,
  pub location: String,
  pub mime_type: Option<String>,
  pub metadata: Value,
}

impl ArtifactRef {
  pub fn validate(&self) -> Result<(), PipelineContractError> {
    let path = Path::new(&self.location);
    let metadata = std::fs::metadata(path).map_err(|error| PipelineContractError::InvalidArtifact { artifact_id: self.artifact_id.clone(), message: format!("artifact file is unavailable: {error}") })?;
    if !metadata.is_file() {
      return Err(PipelineContractError::InvalidArtifact { artifact_id: self.artifact_id.clone(), message: "artifact location is not a regular file".to_string() });
    }
    if metadata.len() == 0 {
      return Err(PipelineContractError::InvalidArtifact { artifact_id: self.artifact_id.clone(), message: "artifact file is empty".to_string() });
    }
    if self.kind.requires_json() {
      let bytes = std::fs::read(path).map_err(|error| PipelineContractError::InvalidArtifact { artifact_id: self.artifact_id.clone(), message: format!("artifact file cannot be read: {error}") })?;
      serde_json::from_slice::<Value>(&bytes).map_err(|error| PipelineContractError::InvalidArtifact { artifact_id: self.artifact_id.clone(), message: format!("artifact JSON is invalid: {error}") })?;
    }
    Ok(())
  }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PipelineContext {
  pub job_id: String,
  pub project_id: Option<String>,
  pub workflow_mode: String,
  #[serde(default)]
  pub content_source: Option<String>,
  pub prompt: String,
  pub model_id: Option<String>,
  pub voice_id: Option<String>,
  pub language: String,
  pub target_duration_seconds: u32,
  pub output_mode: String,
  pub source_url: Option<String>,
  pub local_file: Option<String>,
  #[serde(default)]
  pub story_url: Option<String>,
  #[serde(default)]
  pub research_enabled: bool,
  #[serde(default)]
  pub research_platform: Option<String>,
  #[serde(default)]
  pub research_query: Option<String>,
  #[serde(default)]
  pub research_mode: Option<String>,
  #[serde(default)]
  pub xhs_variant: Option<String>,
  pub artifact_refs: Vec<ArtifactRef>,
  pub stage_states: Vec<StageState>,
}

impl PipelineContext {
  pub fn initial_stage_states() -> Vec<StageState> {
    StageId::ALL.into_iter().map(StageState::pending).collect()
  }

  pub fn require_artifact(&self, stage_id: StageId, kind: ArtifactKind) -> Result<&ArtifactRef, PipelineContractError> {
    let artifact = self.artifact_refs.iter().find(|artifact| artifact.kind == kind).ok_or(PipelineContractError::MissingArtifact { stage_id, kind })?;
    artifact.validate()?;
    Ok(artifact)
  }

  pub fn require_artifact_id(&self, stage_id: StageId, artifact_id: &str) -> Result<&ArtifactRef, PipelineContractError> {
    let artifact = self.artifact_refs.iter().find(|artifact| artifact.artifact_id == artifact_id).ok_or_else(|| PipelineContractError::MissingArtifactId { stage_id, artifact_id: artifact_id.to_string() })?;
    artifact.validate()?;
    Ok(artifact)
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PipelineContractError {
  InvalidTransition { stage_id: StageId, from: StageStatus, to: StageStatus },
  NonRetryable { stage_id: StageId, code: String },
  MissingArtifact { stage_id: StageId, kind: ArtifactKind },
  MissingArtifactId { stage_id: StageId, artifact_id: String },
  InvalidArtifact { artifact_id: String, message: String },
  UnknownArtifactKind(String),
  MissingStage(StageId),
}

impl Display for PipelineContractError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::InvalidTransition { stage_id, from, to } => write!(formatter, "invalid stage transition for {stage_id}: {from:?} -> {to:?}"),
      Self::NonRetryable { stage_id, code } => write!(formatter, "stage {stage_id} failure is not retryable: {code}"),
      Self::MissingArtifact { stage_id, kind } => write!(formatter, "stage {stage_id} requires missing artifact {kind}"),
      Self::MissingArtifactId { stage_id, artifact_id } => write!(formatter, "stage {stage_id} requires missing artifact id {artifact_id}"),
      Self::InvalidArtifact { artifact_id, message } => write!(formatter, "artifact {artifact_id} is invalid: {message}"),
      Self::UnknownArtifactKind(kind) => write!(formatter, "unknown artifact kind: {kind}"),
      Self::MissingStage(stage_id) => write!(formatter, "pipeline context is missing stage {stage_id}"),
    }
  }
}

impl std::error::Error for PipelineContractError {}

#[cfg(test)]
mod pipeline_contracts {
  use super::*;
  use std::fs;

  fn retryable_error() -> StageError {
    StageError::sanitized("TEMPORARY_UNAVAILABLE", "service temporarily unavailable", true)
  }

  fn context_with_artifacts(artifact_refs: Vec<ArtifactRef>) -> PipelineContext {
    PipelineContext { job_id: "job-1".to_string(), project_id: Some("project-1".to_string()), workflow_mode: "source_based".to_string(), content_source: None, prompt: "Summarize".to_string(), model_id: Some("auto".to_string()), voice_id: None, language: "vi".to_string(), target_duration_seconds: 30, output_mode: "draft_only".to_string(), source_url: None, local_file: None, story_url: None, research_enabled: false, research_platform: None, research_query: None, research_mode: None, xhs_variant: None, artifact_refs, stage_states: PipelineContext::initial_stage_states() }
  }

  #[test]
  fn content_source_resolves_deterministically() {
    assert_eq!(ContentSource::resolve(Some("prompt_only"), None, None, None, false, None), ContentSource::PromptOnly);
    assert_eq!(ContentSource::resolve(None, Some("video.mp4"), None, None, false, None), ContentSource::LocalMedia);
    assert_eq!(ContentSource::resolve(None, None, Some("https://example.com/article/123"), None, false, None), ContentSource::WebStory);
    assert_eq!(ContentSource::resolve(None, None, Some("https://www.youtube.com/watch?v=123"), None, false, None), ContentSource::VideoUrl);
    assert_eq!(ContentSource::resolve(None, None, None, None, true, Some("trend")), ContentSource::TrendResearch);
    assert_eq!(ContentSource::resolve(None, None, None, None, false, None), ContentSource::PromptOnly);
  }

  #[test]
  fn lifecycle_pending_running_completed() {
    let mut state = StageState::pending(StageId::StoryScript);
    state.start_stage(Some("omniroute".to_string()), "2026-01-01T00:00:00Z").unwrap();
    state.complete_stage(vec!["artifact-script".to_string()], "2026-01-01T00:01:00Z").unwrap();

    assert_eq!(state.status, StageStatus::Completed);
    assert_eq!(state.attempt, 1);
    assert_eq!(state.output_artifact_ids, vec!["artifact-script"]);
  }

  #[test]
  fn optional_stage_can_be_skipped() {
    let mut state = StageState::pending(StageId::Research);
    state.skip_stage("2026-01-01T00:00:00Z").unwrap();
    assert_eq!(state.status, StageStatus::Skipped);
  }

  #[test]
  fn running_stage_can_fail_with_structured_error() {
    let mut state = StageState::pending(StageId::Voice);
    state.start_stage(Some("tts".to_string()), "start").unwrap();
    state.fail_stage(retryable_error(), "finish").unwrap();

    assert_eq!(state.status, StageStatus::Failed);
    let error = state.error.as_ref().expect("failed stage must persist its structured error");
    assert_eq!(error.retryable, true);
    assert_eq!(error.service.as_deref(), Some("tts"));
    assert_eq!(error.stage_id, Some(StageId::Voice));
    assert_eq!(error.timestamp.as_deref(), Some("finish"));
  }

  #[test]
  fn running_research_can_wait_for_input_without_failing_downstream() {
    let mut context = context_with_artifacts(Vec::new());
    let research = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Research).unwrap();
    research.start_stage(Some("mediacrawler".to_string()), "start").unwrap();
    research.wait_for_input(StageError::sanitized("RESEARCH_AUTH_REQUIRED", "Waiting for RedNote login", true), "waiting").unwrap();

    assert_eq!(research.status, StageStatus::WaitingInput);
    assert_eq!(research.finished_at, None);
    assert_eq!(research.error.as_ref().map(|error| error.code.as_str()), Some("RESEARCH_AUTH_REQUIRED"));
    assert!(context.stage_states.iter().filter(|state| state.stage_id != StageId::Research).all(|state| state.status == StageStatus::Pending));
  }

  #[test]
  fn canonical_failure_error_serializes_observability_fields() {
    let mut state = StageState::pending(StageId::Research);
    state.start_stage(Some("mediacrawler".to_string()), "start").unwrap();
    state.fail_stage(StageError::sanitized("MEDIACRAWLER_AUTH_REQUIRED", "MediaCrawler cookie login is required", false), "2026-08-09T12:00:00Z").unwrap();

    let value = serde_json::to_value(&state).unwrap();
    let error = &value["error"];
    assert_eq!(error["code"], "MEDIACRAWLER_AUTH_REQUIRED");
    assert_eq!(error["service"], "mediacrawler");
    assert_eq!(error["stage_id"], "research");
    assert_eq!(error["retryable"], false);
    assert_eq!(error["timestamp"], "2026-08-09T12:00:00Z");
  }

  #[test]
  fn retryable_failure_can_retry_and_complete() {
    let mut state = StageState::pending(StageId::Voice);
    state.start_stage(Some("tts".to_string()), "start-1").unwrap();
    state.fail_stage(retryable_error(), "finish-1").unwrap();
    state.retry_stage().unwrap();
    state.start_stage(Some("tts".to_string()), "start-2").unwrap();
    state.complete_stage(vec!["voice-audio".to_string()], "finish-2").unwrap();

    assert_eq!(state.status, StageStatus::Completed);
    assert_eq!(state.attempt, 2);
    assert!(state.error.is_none());
  }

  #[test]
  fn running_stage_can_be_cancelled() {
    let mut state = StageState::pending(StageId::MediaTimeline);
    state.start_stage(Some("openmontage".to_string()), "start").unwrap();
    state.cancel_stage("finish").unwrap();
    assert_eq!(state.status, StageStatus::Cancelled);
  }

  #[test]
  fn artifact_dependency_requires_real_valid_file() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("script.json");
    fs::write(&path, br#"{"scenes":[]}"#).unwrap();
    let artifact = ArtifactRef { artifact_id: "artifact-script".to_string(), kind: ArtifactKind::Script, produced_by_stage: StageId::StoryScript, location: path.to_string_lossy().to_string(), mime_type: Some("application/json".to_string()), metadata: Value::Null };
    let context = context_with_artifacts(vec![artifact]);

    assert_eq!(context.require_artifact(StageId::Voice, ArtifactKind::Script).unwrap().artifact_id, "artifact-script");
  }

  #[test]
  fn missing_artifact_dependency_is_structured() {
    let context = context_with_artifacts(Vec::new());
    let error = context.require_artifact(StageId::Voice, ArtifactKind::Script).unwrap_err();
    assert_eq!(error, PipelineContractError::MissingArtifact { stage_id: StageId::Voice, kind: ArtifactKind::Script });
  }

  #[test]
  fn artifact_handoff_resolves_by_artifact_id() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("scenes.json");
    fs::write(&path, br#"{"scenes":[]}"#).unwrap();
    let artifact = ArtifactRef { artifact_id: "artifact-scenes".to_string(), kind: ArtifactKind::Scenes, produced_by_stage: StageId::IngestAnalyze, location: path.to_string_lossy().to_string(), mime_type: Some("application/json".to_string()), metadata: Value::Null };
    let context = context_with_artifacts(vec![artifact]);

    assert_eq!(context.require_artifact_id(StageId::StoryScript, "artifact-scenes").unwrap().kind, ArtifactKind::Scenes);
  }

  #[test]
  fn stage_state_serialization_round_trips() {
    let mut state = StageState::pending(StageId::IngestAnalyze);
    state.input_artifact_ids.push("input-1".to_string());
    state.start_stage(Some("youwee".to_string()), "start").unwrap();

    let json = serde_json::to_string(&state).unwrap();
    let decoded: StageState = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, state);
    assert!(json.contains("ingest_analyze"));
    assert!(json.contains("running"));
  }

  #[test]
  fn historical_pipeline_context_defaults_new_research_fields() {
    let context = context_with_artifacts(Vec::new());
    let mut value = serde_json::to_value(context).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("content_source");
    object.remove("story_url");
    object.remove("research_platform");
    object.remove("research_query");
    object.remove("research_mode");
    object.remove("xhs_variant");

    let decoded: PipelineContext = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.content_source, None);
    assert_eq!(decoded.story_url, None);
    assert_eq!(decoded.research_platform, None);
    assert_eq!(decoded.research_query, None);
    assert_eq!(decoded.research_mode, None);
    assert_eq!(decoded.xhs_variant, None);
  }

  #[test]
  fn dependency_graph_marks_research_optional_for_story_script() {
    assert_eq!(StageId::Research.dependencies(), &[StageDependency { stage_id: StageId::Input, required: true }]);
    assert_eq!(StageId::StoryScript.dependencies(), &[StageDependency { stage_id: StageId::IngestAnalyze, required: true }, StageDependency { stage_id: StageId::Research, required: false }]);
  }

  #[test]
  fn stage_error_redacts_secrets_and_bounds_event_message() {
    let error = StageError::sanitized("UPSTREAM", format!("Bearer secret-token\napi_key=private {}", "x".repeat(600)), true);
    assert!(!error.message.contains("secret-token"));
    assert!(!error.message.contains("private"));
    assert!(!error.message.contains('\n'));
    assert!(error.message.chars().count() <= 512);
  }
}
