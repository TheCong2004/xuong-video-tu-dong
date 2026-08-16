use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef, PipelineContext, PipelineContractError, StageId};
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_BACKEND_URL: &str = "http://127.0.0.1:30000";
pub const MEDIA_TIMELINE_MAX_ATTEMPTS: u32 = 2;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactInput {
  pub artifact_id: String,
  pub path: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duration_seconds: Option<f64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scene_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTimelineInput {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub source_video: Option<ArtifactInput>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub visual_assets: Vec<ArtifactInput>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scene_plan: Option<Value>,
  pub scenes: Value,
  pub script: Value,
  pub voice_audio: ArtifactInput,
  pub voice_timing: Value,
  pub output_dir: String,
  pub input_artifact_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTimelineOutput {
  pub timeline: Value,
  pub captions: Value,
  pub timeline_path: PathBuf,
  pub captions_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaTimelineError {
  pub code: String,
  pub message: String,
  pub retryable: bool,
  pub cancelled: bool,
}

impl MediaTimelineError {
  pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
    Self { code: code.into(), message: message.into(), retryable, cancelled: false }
  }

  fn cancelled() -> Self {
    Self { code: "MEDIA_TIMELINE_CANCELLED".to_string(), message: "Media timeline stage cancelled".to_string(), retryable: false, cancelled: true }
  }
}

pub fn prepare_media_timeline(context: &PipelineContext, work_dir: &Path) -> Result<MediaTimelineInput, PipelineContractError> {
  let original_creation = matches!(context.workflow_mode.as_str(), "original" | "original_creation");
  let (source_video, visual_assets, scene_plan, scenes) = if original_creation {
    let plan = stage_artifact(context, StageId::StoryScript, ArtifactKind::ScenePlan)?;
    let assets = context.artifact_refs.iter().filter(|artifact| artifact.kind == ArtifactKind::GeneratedVideo).map(artifact_input).collect::<Vec<_>>();
    if assets.is_empty() {
      return Err(PipelineContractError::MissingArtifact { stage_id: StageId::MediaTimeline, kind: ArtifactKind::GeneratedVideo });
    }
    (None, assets, Some(read_json(plan)?), serde_json::json!({}))
  } else {
    let source = stage_artifact(context, StageId::IngestAnalyze, ArtifactKind::SourceVideo)?;
    let scenes = stage_artifact(context, StageId::IngestAnalyze, ArtifactKind::Scenes)?;
    (Some(artifact_input(source)), Vec::new(), None, read_json(scenes)?)
  };
  let script = stage_artifact(context, StageId::StoryScript, ArtifactKind::Script)?;
  let voice_audio = stage_artifact(context, StageId::Voice, ArtifactKind::VoiceAudio)?;
  let voice_timing = stage_artifact(context, StageId::Voice, ArtifactKind::VoiceTiming)?;
  let mut input_artifact_ids = source_video.iter().chain(visual_assets.iter()).map(|artifact| artifact.artifact_id.clone()).collect::<Vec<_>>();
  input_artifact_ids.extend([script.artifact_id.clone(), voice_audio.artifact_id.clone(), voice_timing.artifact_id.clone()]);
  Ok(MediaTimelineInput { source_video, visual_assets, scene_plan, scenes, script: read_json(script)?, voice_audio: artifact_input(voice_audio), voice_timing: read_json(voice_timing)?, output_dir: work_dir.join("media_timeline").to_string_lossy().to_string(), input_artifact_ids })
}

pub async fn run_openmontage(input: &MediaTimelineInput, cancel_flag: Arc<AtomicBool>) -> Result<MediaTimelineOutput, MediaTimelineError> {
  validate_input(input)?;
  let base_url = env::var("FLOWORD_BACKEND_URL").unwrap_or_else(|_| DEFAULT_BACKEND_URL.to_string());
  let url = format!("{}/api/montage/timeline", base_url.trim_end_matches('/'));
  let client = Client::builder().timeout(Duration::from_secs(60)).build().map_err(|error| MediaTimelineError::new("OPENMONTAGE_UNAVAILABLE", error.to_string(), true))?;
  let response = tokio::select! {
    result = client.post(url).json(input).send() => result,
    _ = wait_for_cancel(&cancel_flag) => return Err(MediaTimelineError::cancelled()),
  }
  .map_err(|error| MediaTimelineError::new(if error.is_timeout() { "OPENMONTAGE_TIMEOUT" } else { "OPENMONTAGE_UNAVAILABLE" }, error.to_string(), true))?;
  let status = response.status();
  let body = response.text().await.unwrap_or_default();
  if !status.is_success() {
    return Err(classify_http_failure(status, &body));
  }
  if cancel_flag.load(Ordering::SeqCst) {
    return Err(MediaTimelineError::cancelled());
  }
  let output = serde_json::from_str::<MediaTimelineOutput>(&body).map_err(|error| MediaTimelineError::new("OPENMONTAGE_INVALID_RESPONSE", error.to_string(), false))?;
  validate_output(&output, input)?;
  Ok(output)
}

pub fn validate_input(input: &MediaTimelineInput) -> Result<(), MediaTimelineError> {
  if input.source_video.is_none() && input.visual_assets.is_empty() {
    return Err(MediaTimelineError::new("MEDIA_TIMELINE_VISUAL_MISSING", "source video or generated visual assets are required", false));
  }
  let mut files = input.source_video.iter().map(|asset| ("source_video", asset)).chain(input.visual_assets.iter().map(|asset| ("visual_asset", asset))).collect::<Vec<_>>();
  files.push(("voice_audio", &input.voice_audio));
  for (label, artifact) in files {
    let path = Path::new(&artifact.path);
    let valid = std::fs::metadata(path).map(|metadata| metadata.is_file() && metadata.len() > 0).unwrap_or(false);
    if !valid {
      return Err(MediaTimelineError::new("OPENMONTAGE_ARTIFACT_MISSING", format!("{label} artifact {} is missing or empty", artifact.artifact_id), false));
    }
  }
  for (label, document) in [("scenes", &input.scenes), ("script", &input.script), ("voice_timing", &input.voice_timing)] {
    if !document.is_object() {
      return Err(MediaTimelineError::new("OPENMONTAGE_INPUT_INVALID", format!("{label} artifact must contain a JSON object"), false));
    }
  }
  Ok(())
}

fn classify_http_failure(status: reqwest::StatusCode, body: &str) -> MediaTimelineError {
  let detail = serde_json::from_str::<Value>(body).ok().and_then(|value| value.get("detail").cloned());
  let structured_code = detail.as_ref().and_then(|value| value.get("code")).and_then(Value::as_str);
  let server_message = detail.as_ref().and_then(|value| value.get("message").and_then(Value::as_str).or_else(|| value.as_str())).unwrap_or(body).trim();
  let code = if let Some(code) = structured_code {
    code
  } else if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY || status == reqwest::StatusCode::BAD_REQUEST {
    if server_message.contains("non-empty file") {
      "OPENMONTAGE_ARTIFACT_MISSING"
    } else {
      "OPENMONTAGE_INPUT_INVALID"
    }
  } else if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
    "OPENMONTAGE_UNAVAILABLE"
  } else if status.is_server_error() {
    "OPENMONTAGE_INTERNAL_ERROR"
  } else {
    "OPENMONTAGE_INPUT_INVALID"
  };
  let retryable = matches!(code, "OPENMONTAGE_UNAVAILABLE" | "OPENMONTAGE_TIMEOUT") || status.as_u16() == 429;
  MediaTimelineError::new(code, format!("OpenMontage HTTP {}: {server_message}", status.as_u16()), retryable)
}

pub fn validate_output(output: &MediaTimelineOutput, input: &MediaTimelineInput) -> Result<(), MediaTimelineError> {
  let duration = output.timeline.get("durationSeconds").and_then(Value::as_f64).filter(|value| value.is_finite() && *value > 0.0).ok_or_else(|| MediaTimelineError::new("OPENMONTAGE_INVALID_TIMELINE", "timeline duration is invalid", false))?;
  let tracks = output.timeline.get("tracks").and_then(Value::as_array).ok_or_else(|| MediaTimelineError::new("OPENMONTAGE_INVALID_TIMELINE", "timeline tracks are missing", false))?;
  for kind in ["video", "voice", "caption"] {
    if !tracks.iter().any(|track| track.get("kind").and_then(Value::as_str) == Some(kind)) {
      return Err(MediaTimelineError::new("OPENMONTAGE_INVALID_TIMELINE", format!("{kind} track is missing"), false));
    }
  }
  for track in tracks {
    if let Some(segments) = track.get("segments").and_then(Value::as_array) {
      for segment in segments {
        let start = segment.get("startSeconds").or_else(|| segment.get("start")).and_then(Value::as_f64).ok_or_else(|| MediaTimelineError::new("OPENMONTAGE_INVALID_TIMELINE", "track segment start timing is missing", false))?;
        let end = segment.get("endSeconds").or_else(|| segment.get("end")).and_then(Value::as_f64).ok_or_else(|| MediaTimelineError::new("OPENMONTAGE_INVALID_TIMELINE", "track segment end timing is missing", false))?;
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end < start || end > duration + 0.25 {
          return Err(MediaTimelineError::new("OPENMONTAGE_INVALID_TIMELINE", "track segment timing is outside timeline duration", false));
        }
      }
    }
  }
  let referenced = output.timeline.get("referencedArtifacts").and_then(Value::as_array).ok_or_else(|| MediaTimelineError::new("OPENMONTAGE_INVALID_TIMELINE", "referenced artifacts are missing", false))?;
  let expected = input.source_video.iter().chain(input.visual_assets.iter()).map(|artifact| artifact.artifact_id.as_str()).chain(std::iter::once(input.voice_audio.artifact_id.as_str()));
  for id in expected {
    if !referenced.iter().any(|value| value.as_str() == Some(id)) {
      return Err(MediaTimelineError::new("OPENMONTAGE_INVALID_TIMELINE", format!("artifact {id} is not referenced"), false));
    }
  }
  if !output.timeline_path.is_file() || !output.captions_path.is_file() || serde_json::from_slice::<Value>(&std::fs::read(&output.timeline_path).map_err(|error| MediaTimelineError::new("OPENMONTAGE_ARTIFACT_INVALID", error.to_string(), false))?).is_err() || serde_json::from_slice::<Value>(&std::fs::read(&output.captions_path).map_err(|error| MediaTimelineError::new("OPENMONTAGE_ARTIFACT_INVALID", error.to_string(), false))?).is_err() {
    return Err(MediaTimelineError::new("OPENMONTAGE_ARTIFACT_INVALID", "timeline or captions file is missing/invalid JSON", false));
  }
  Ok(())
}

pub fn should_retry(error: &MediaTimelineError, attempt: u32) -> bool {
  error.retryable && !error.cancelled && attempt < MEDIA_TIMELINE_MAX_ATTEMPTS
}

fn stage_artifact<'a>(context: &'a PipelineContext, producer: StageId, kind: ArtifactKind) -> Result<&'a ArtifactRef, PipelineContractError> {
  let ids = context.stage_states.iter().find(|state| state.stage_id == producer).map(|state| state.output_artifact_ids.as_slice()).unwrap_or_default();
  let id = ids.iter().find(|id| context.artifact_refs.iter().any(|artifact| artifact.artifact_id == id.as_str() && artifact.kind == kind)).ok_or(PipelineContractError::MissingArtifact { stage_id: StageId::MediaTimeline, kind })?;
  context.require_artifact_id(StageId::MediaTimeline, id)
}

fn artifact_input(artifact: &ArtifactRef) -> ArtifactInput {
  ArtifactInput { artifact_id: artifact.artifact_id.clone(), path: artifact.location.clone(), duration_seconds: artifact.metadata.get("duration_seconds").and_then(Value::as_f64), scene_id: artifact.metadata.get("scene_id").and_then(Value::as_str).map(str::to_string) }
}

fn read_json(artifact: &ArtifactRef) -> Result<Value, PipelineContractError> {
  let bytes = std::fs::read(&artifact.location).map_err(|error| PipelineContractError::InvalidArtifact { artifact_id: artifact.artifact_id.clone(), message: error.to_string() })?;
  serde_json::from_slice(&bytes).map_err(|error| PipelineContractError::InvalidArtifact { artifact_id: artifact.artifact_id.clone(), message: error.to_string() })
}

async fn wait_for_cancel(flag: &Arc<AtomicBool>) {
  while !flag.load(Ordering::SeqCst) {
    tokio::time::sleep(Duration::from_millis(100)).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::artifact_store::ArtifactStore;
  use serde_json::json;

  #[test]
  fn rejects_negative_or_overlong_track_timing() {
    let temp = tempfile::tempdir().unwrap();
    let timeline_path = temp.path().join("timeline.json");
    let captions_path = temp.path().join("captions.json");
    std::fs::write(&timeline_path, b"{}").unwrap();
    std::fs::write(&captions_path, b"{}").unwrap();
    let input = MediaTimelineInput { source_video: Some(ArtifactInput { artifact_id: "source".into(), path: "source.mp4".into(), duration_seconds: None, scene_id: None }), visual_assets: vec![], scene_plan: None, scenes: json!({}), script: json!({}), voice_audio: ArtifactInput { artifact_id: "voice".into(), path: "voice.mp3".into(), duration_seconds: None, scene_id: None }, voice_timing: json!({}), output_dir: temp.path().to_string_lossy().to_string(), input_artifact_ids: vec![] };
    let output = MediaTimelineOutput { timeline: json!({ "durationSeconds": 5.0, "referencedArtifacts": ["source", "voice"], "tracks": [{"kind":"video","segments":[{"startSeconds":0,"endSeconds":6}]},{"kind":"voice","segments":[]},{"kind":"caption","segments":[]}] }), captions: json!({}), timeline_path, captions_path };
    assert_eq!(validate_output(&output, &input).unwrap_err().code, "OPENMONTAGE_INVALID_TIMELINE");
  }

  #[test]
  fn retry_is_limited_to_transient_failures() {
    assert!(should_retry(&MediaTimelineError::new("OPENMONTAGE_UNAVAILABLE", "offline", true), 1));
    assert!(!should_retry(&MediaTimelineError::new("OPENMONTAGE_UNAVAILABLE", "offline", true), 2));
    assert!(!should_retry(&MediaTimelineError::new("OPENMONTAGE_INVALID_INPUT", "bad", false), 1));
  }

  #[test]
  fn missing_input_file_is_rejected_before_http() {
    let temp = tempfile::tempdir().unwrap();
    let voice = temp.path().join("voice.mp3");
    std::fs::write(&voice, b"voice").unwrap();
    let input = MediaTimelineInput { source_video: Some(ArtifactInput { artifact_id: "source".into(), path: temp.path().join("missing.mp4").to_string_lossy().to_string(), duration_seconds: None, scene_id: None }), visual_assets: vec![], scene_plan: None, scenes: json!({"scenes": []}), script: json!({"scenes": []}), voice_audio: ArtifactInput { artifact_id: "voice".into(), path: voice.to_string_lossy().to_string(), duration_seconds: None, scene_id: None }, voice_timing: json!({"segments": []}), output_dir: temp.path().join("out").to_string_lossy().to_string(), input_artifact_ids: vec![] };
    assert_eq!(validate_input(&input).unwrap_err().code, "OPENMONTAGE_ARTIFACT_MISSING");
  }

  #[test]
  fn server_failures_are_classified_from_status_and_detail() {
    let error = classify_http_failure(reqwest::StatusCode::INTERNAL_SERVER_ERROR, r#"{"detail":{"code":"OPENMONTAGE_COMPOSE_FAILED","message":"compose failed"}}"#);
    assert_eq!(error.code, "OPENMONTAGE_COMPOSE_FAILED");
    assert!(!error.retryable);
    let generic = classify_http_failure(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    assert_eq!(generic.code, "OPENMONTAGE_INTERNAL_ERROR");
  }

  #[tokio::test]
  #[ignore = "requires the local unified backend and real Phase 4 artifacts"]
  async fn runtime_real_artifacts_produce_registered_timeline_and_captions() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().to_path_buf();
    let workflow_root = root.join("artifacts/phase5-runtime-rust");
    let timing: Value = serde_json::from_slice(&std::fs::read(root.join("artifacts/phase4-runtime-stage/voice/voice_timing.json")).unwrap()).unwrap();
    let segments = timing["segments"].as_array().unwrap();
    let input = MediaTimelineInput { source_video: Some(ArtifactInput { artifact_id: "runtime-source-video-artifact".into(), path: root.join("test_data/video/mp4/golden_sun_garoh.mp4").to_string_lossy().to_string(), duration_seconds: None, scene_id: None }), visual_assets: vec![], scene_plan: None, scenes: json!({ "scenes": [{"start_seconds": 0.0, "end_seconds": 7.584}, {"start_seconds": 7.584, "end_seconds": 15.168}] }), script: json!({ "scenes": [{"id": "scene-1", "narration": segments[0]["text"]}, {"id": "scene-2", "narration": segments[1]["text"]}] }), voice_audio: ArtifactInput { artifact_id: "runtime-voice-audio-artifact".into(), path: root.join("artifacts/phase4-runtime-stage/voice/voice.mp3").to_string_lossy().to_string(), duration_seconds: None, scene_id: None }, voice_timing: timing, output_dir: workflow_root.join("media_timeline").to_string_lossy().to_string(), input_artifact_ids: vec!["runtime-source-video-artifact".into(), "runtime-scenes-artifact".into(), "runtime-script-artifact".into(), "runtime-voice-audio-artifact".into(), "runtime-voice-timing-artifact".into()] };
    let output = run_openmontage(&input, Arc::new(AtomicBool::new(false))).await.unwrap();
    let timeline = ArtifactStore::register_typed_artifact(&workflow_root, "phase5-runtime", StageId::MediaTimeline, "openmontage", ArtifactKind::Timeline, &output.timeline_path, json!({"input_artifact_ids": input.input_artifact_ids})).unwrap();
    let captions = ArtifactStore::register_typed_artifact(&workflow_root, "phase5-runtime", StageId::MediaTimeline, "openmontage", ArtifactKind::Captions, &output.captions_path, json!({"duration_seconds": output.timeline["durationSeconds"]})).unwrap();
    assert_eq!(timeline.to_artifact_ref(StageId::MediaTimeline).unwrap().kind, ArtifactKind::Timeline);
    assert_eq!(captions.to_artifact_ref(StageId::MediaTimeline).unwrap().kind, ArtifactKind::Captions);
  }
}
