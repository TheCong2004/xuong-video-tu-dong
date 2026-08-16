use crate::services::pipeline::clients::omniroute_client::StructuredScript;
use crate::services::pipeline::contracts::{ArtifactKind, PipelineContext, PipelineContractError, StageId};
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use vynaro_detect::Ffmpeg;

const DEFAULT_OMNIROUTE_URL: &str = "http://127.0.0.1:20128";
pub const VOICE_MAX_ATTEMPTS: u32 = 2;

#[derive(Clone, Debug)]
pub struct VoiceInput {
  pub script_artifact_id: String,
  pub script: StructuredScript,
  pub voice: String,
  pub language: String,
  pub model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSegmentTiming {
  pub scene_id: String,
  pub scene_index: u32,
  pub text: String,
  pub start_seconds: f64,
  pub end_seconds: f64,
  pub duration_seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceTiming {
  pub source: String,
  pub script_artifact_id: String,
  pub model: String,
  pub voice: String,
  pub language: String,
  pub duration_seconds: f64,
  pub segments: Vec<VoiceSegmentTiming>,
}

#[derive(Clone, Debug)]
pub struct VoiceRuntimeOutput {
  pub audio_path: PathBuf,
  pub timing_path: PathBuf,
  pub timing: VoiceTiming,
  pub audio_codec: Option<String>,
  pub size_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceError {
  pub code: String,
  pub message: String,
  pub retryable: bool,
  pub cancelled: bool,
}

impl VoiceError {
  pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
    Self { code: code.into(), message: message.into(), retryable, cancelled: false }
  }

  fn cancelled() -> Self {
    Self { code: "VOICE_CANCELLED".to_string(), message: "Voice stage cancelled".to_string(), retryable: false, cancelled: true }
  }
}

pub fn prepare_voice(context: &PipelineContext) -> Result<VoiceInput, PipelineContractError> {
  let story_state = context.stage_states.iter().find(|state| state.stage_id == StageId::StoryScript);
  let script_id = story_state.and_then(|state| state.output_artifact_ids.iter().find(|id| context.artifact_refs.iter().any(|artifact| artifact.artifact_id == id.as_str() && artifact.kind == ArtifactKind::Script))).ok_or(PipelineContractError::MissingArtifact { stage_id: StageId::Voice, kind: ArtifactKind::Script })?;
  let artifact = context.require_artifact_id(StageId::Voice, script_id)?;
  if artifact.kind != ArtifactKind::Script {
    return Err(PipelineContractError::MissingArtifact { stage_id: StageId::Voice, kind: ArtifactKind::Script });
  }
  let bytes = std::fs::read(&artifact.location).map_err(|error| PipelineContractError::InvalidArtifact { artifact_id: artifact.artifact_id.clone(), message: error.to_string() })?;
  let script = serde_json::from_slice::<StructuredScript>(&bytes).map_err(|error| PipelineContractError::InvalidArtifact { artifact_id: artifact.artifact_id.clone(), message: error.to_string() })?;
  let requested_voice = context.voice_id.as_deref().map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
  let named_voice = requested_voice.clone().unwrap_or_else(|| default_voice(&context.language).to_string());
  let model = env::var("FLOWORD_TTS_MODEL").ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty()).unwrap_or_else(|| requested_voice.as_ref().map(|voice| format!("edgetts/{voice}")).unwrap_or_else(|| "gtts/default".to_string()));
  let voice = if model.starts_with("gtts/") { context.language.clone() } else { named_voice };
  Ok(VoiceInput { script_artifact_id: script_id.clone(), script, voice, language: context.language.clone(), model })
}

pub async fn synthesize_voice(input: &VoiceInput, work_dir: &Path, cancel_flag: Arc<AtomicBool>) -> Result<VoiceRuntimeOutput, VoiceError> {
  let ffmpeg = Ffmpeg::discover().map_err(|error| VoiceError::new("FFMPEG_NOT_FOUND", error.to_string(), false))?;
  synthesize_voice_with_ffmpeg(input, work_dir, cancel_flag, ffmpeg).await
}

/// Production entry point: resolve the ArtCraft-managed FFmpeg pair instead of
/// relying on the worker process PATH.
pub async fn synthesize_voice_with_runtime(app: &AppHandle, input: &VoiceInput, work_dir: &Path, cancel_flag: Arc<AtomicBool>) -> Result<VoiceRuntimeOutput, VoiceError> {
  let runtime = app_lib::services::resolve_ffmpeg_runtime(app).await.map_err(|error| VoiceError::new(error.code(), error.to_string(), false))?;
  let ffmpeg = Ffmpeg::with_bins(runtime.ffmpeg_path, runtime.ffprobe_path);
  synthesize_voice_with_ffmpeg(input, work_dir, cancel_flag, ffmpeg).await
}

async fn synthesize_voice_with_ffmpeg(input: &VoiceInput, work_dir: &Path, cancel_flag: Arc<AtomicBool>, ffmpeg: Ffmpeg) -> Result<VoiceRuntimeOutput, VoiceError> {
  if input.script.scenes.is_empty() {
    return Err(VoiceError::new("VOICE_INPUT_INVALID", "Script contains no scenes", false));
  }
  let voice_dir = work_dir.join("voice");
  let clips_dir = voice_dir.join("clips");
  std::fs::create_dir_all(&clips_dir).map_err(|error| VoiceError::new("VOICE_ARTIFACT_FAILED", error.to_string(), false))?;
  let client = Client::builder().timeout(Duration::from_secs(60)).build().map_err(|error| VoiceError::new("VOICE_TTS_UNAVAILABLE", error.to_string(), true))?;
  let mut clip_paths = Vec::with_capacity(input.script.scenes.len());
  let mut measured = Vec::with_capacity(input.script.scenes.len());

  for scene in &input.script.scenes {
    if cancel_flag.load(Ordering::SeqCst) {
      return Err(VoiceError::cancelled());
    }
    let narration = scene.narration.trim();
    if narration.is_empty() {
      return Err(VoiceError::new("VOICE_INPUT_INVALID", format!("Scene {} narration is empty", scene.id), false));
    }
    let audio = request_speech(&client, input, narration, &cancel_flag).await?;
    let path = clips_dir.join(format!("scene_{:04}.{}", scene.index, audio.extension));
    std::fs::write(&path, &audio.bytes).map_err(|error| VoiceError::new("VOICE_ARTIFACT_FAILED", error.to_string(), false))?;
    let metadata = std::fs::metadata(&path).map_err(|error| VoiceError::new("VOICE_ARTIFACT_FAILED", error.to_string(), false))?;
    if metadata.len() == 0 {
      return Err(VoiceError::new("VOICE_AUDIO_INVALID", format!("TTS returned empty audio for scene {}", scene.id), false));
    }
    let probe = tokio::time::timeout(Duration::from_secs(120), ffmpeg.probe(&path)).await.map_err(|_| VoiceError::new("FFMPEG_TIMEOUT", format!("ffprobe timed out for scene {}", scene.id), true))?.map_err(|error| VoiceError::new("FFMPEG_PROCESS_FAILED", error.to_string(), false))?;
    if !probe.duration_seconds.is_finite() || probe.duration_seconds <= 0.0 {
      return Err(VoiceError::new("VOICE_AUDIO_INVALID", format!("Audio duration is invalid for scene {}", scene.id), false));
    }
    clip_paths.push(path);
    measured.push((scene.id.clone(), scene.index, narration.to_string(), probe.duration_seconds));
  }

  if cancel_flag.load(Ordering::SeqCst) {
    return Err(VoiceError::cancelled());
  }
  let extension = clip_paths.first().and_then(|path| path.extension()).and_then(|value| value.to_str()).unwrap_or("mp3");
  let audio_path = voice_dir.join(format!("voice.{extension}"));
  if clip_paths.len() == 1 {
    std::fs::copy(&clip_paths[0], &audio_path).map_err(|error| VoiceError::new("VOICE_ARTIFACT_FAILED", error.to_string(), false))?;
  } else {
    tokio::time::timeout(Duration::from_secs(120), ffmpeg.concat(&clip_paths, &audio_path)).await.map_err(|_| VoiceError::new("FFMPEG_TIMEOUT", "ffmpeg audio concatenation timed out", true))?.map_err(|error| VoiceError::new("FFMPEG_PROCESS_FAILED", error.to_string(), false))?;
  }
  if cancel_flag.load(Ordering::SeqCst) {
    return Err(VoiceError::cancelled());
  }
  let final_probe = tokio::time::timeout(Duration::from_secs(120), ffmpeg.probe(&audio_path)).await.map_err(|_| VoiceError::new("FFMPEG_TIMEOUT", "Final ffprobe validation timed out", true))?.map_err(|error| VoiceError::new("FFMPEG_PROCESS_FAILED", error.to_string(), false))?;
  let file_metadata = std::fs::metadata(&audio_path).map_err(|error| VoiceError::new("VOICE_ARTIFACT_FAILED", error.to_string(), false))?;
  if file_metadata.len() == 0 || !final_probe.duration_seconds.is_finite() || final_probe.duration_seconds <= 0.0 {
    return Err(VoiceError::new("VOICE_AUDIO_INVALID", "Final synthesized audio failed size/duration validation", false));
  }
  let segments = measured_timings(&measured)?;
  let timing = VoiceTiming { source: "ffprobe".to_string(), script_artifact_id: input.script_artifact_id.clone(), model: input.model.clone(), voice: input.voice.clone(), language: input.language.clone(), duration_seconds: final_probe.duration_seconds, segments };
  let timing_path = voice_dir.join("voice_timing.json");
  let timing_bytes = serde_json::to_vec_pretty(&timing).map_err(|error| VoiceError::new("VOICE_TIMING_INVALID", error.to_string(), false))?;
  std::fs::write(&timing_path, timing_bytes).map_err(|error| VoiceError::new("VOICE_ARTIFACT_FAILED", error.to_string(), false))?;
  serde_json::from_slice::<VoiceTiming>(&std::fs::read(&timing_path).map_err(|error| VoiceError::new("VOICE_ARTIFACT_FAILED", error.to_string(), false))?).map_err(|error| VoiceError::new("VOICE_TIMING_INVALID", error.to_string(), false))?;
  Ok(VoiceRuntimeOutput { audio_path, timing_path, timing, audio_codec: final_probe.audio_codec, size_bytes: file_metadata.len() })
}

struct SpeechAudio {
  bytes: Vec<u8>,
  extension: &'static str,
}

async fn request_speech(client: &Client, input: &VoiceInput, text: &str, cancel_flag: &Arc<AtomicBool>) -> Result<SpeechAudio, VoiceError> {
  let base_url = env::var("LLM_BASE_URL").unwrap_or_else(|_| DEFAULT_OMNIROUTE_URL.to_string());
  let url = format!("{}/v1/audio/speech", base_url.trim_end_matches('/'));
  let provider_voice = provider_voice(input);
  let mut request = client.post(url).json(&json!({ "model": input.model, "input": text, "voice": provider_voice, "language": input.language, "response_format": "mp3" }));
  if let Some(key) = env::var("LLM_API_KEY").ok().filter(|value| !value.trim().is_empty()) {
    request = request.bearer_auth(key);
  }
  let response = tokio::select! {
    result = request.send() => result,
    _ = wait_for_cancel(cancel_flag) => return Err(VoiceError::cancelled()),
  }
  .map_err(|error| VoiceError::new(if error.is_timeout() { "VOICE_TTS_TIMEOUT" } else { "VOICE_TTS_UNAVAILABLE" }, error.to_string(), true))?;
  let status = response.status();
  let content_type = response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).unwrap_or("").to_ascii_lowercase();
  if !status.is_success() {
    let body = response.text().await.unwrap_or_default();
    let (code, retryable) = match status.as_u16() {
      401 | 403 => ("VOICE_AUTH_REQUIRED", false),
      429 => ("VOICE_RATE_LIMITED", true),
      500..=599 => ("VOICE_TTS_UNAVAILABLE", true),
      _ => ("VOICE_TTS_REJECTED", false),
    };
    return Err(VoiceError::new(code, format!("OmniRoute TTS HTTP {}: {body}", status.as_u16()), retryable));
  }
  let extension = if content_type.contains("wav") {
    "wav"
  } else if content_type.contains("mpeg") || content_type.contains("mp3") {
    "mp3"
  } else {
    return Err(VoiceError::new("VOICE_AUDIO_INVALID", format!("Unsupported TTS content type: {content_type}"), false));
  };
  let bytes = tokio::select! {
    result = response.bytes() => result,
    _ = wait_for_cancel(cancel_flag) => return Err(VoiceError::cancelled()),
  }
  .map_err(|error| VoiceError::new("VOICE_TTS_UNAVAILABLE", error.to_string(), true))?;
  if bytes.is_empty() {
    return Err(VoiceError::new("VOICE_AUDIO_INVALID", "OmniRoute returned an empty audio body", false));
  }
  Ok(SpeechAudio { bytes: bytes.to_vec(), extension })
}

fn provider_voice(input: &VoiceInput) -> &str {
  // OmniRoute's gTTS adapter follows its canonical contract where `voice`
  // carries the language code; named-voice engines receive the user voice.
  if input.model.starts_with("gtts/") {
    input.language.as_str()
  } else {
    input.voice.as_str()
  }
}

fn measured_timings(measured: &[(String, u32, String, f64)]) -> Result<Vec<VoiceSegmentTiming>, VoiceError> {
  let mut cursor = 0.0;
  let mut result = Vec::with_capacity(measured.len());
  for (scene_id, scene_index, text, duration) in measured {
    if !duration.is_finite() || *duration <= 0.0 {
      return Err(VoiceError::new("VOICE_TIMING_INVALID", format!("Invalid measured duration for scene {scene_id}"), false));
    }
    let end = cursor + duration;
    result.push(VoiceSegmentTiming { scene_id: scene_id.clone(), scene_index: *scene_index, text: text.clone(), start_seconds: cursor, end_seconds: end, duration_seconds: *duration });
    cursor = end;
  }
  Ok(result)
}

pub fn should_retry(error: &VoiceError, attempt: u32) -> bool {
  error.retryable && !error.cancelled && attempt < VOICE_MAX_ATTEMPTS
}

fn default_voice(language: &str) -> &'static str {
  match language.trim().to_ascii_lowercase().split(['-', '_']).next().unwrap_or("en") {
    "vi" => "vi-VN-HoaiMyNeural",
    "zh" => "zh-CN-XiaoxiaoNeural",
    "ja" => "ja-JP-NanamiNeural",
    "es" => "es-ES-ElviraNeural",
    "fr" => "fr-FR-DeniseNeural",
    "de" => "de-DE-KatjaNeural",
    _ => "en-US-AriaNeural",
  }
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
  use crate::services::pipeline::contracts::{ArtifactRef, ContentSource, StageStatus};
  use serde_json::Value;

  #[test]
  fn prepares_voice_from_story_output_artifact_id() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("script.json");
    std::fs::write(&path, br#"{"title":"t","hook":"h","cta":"c","language":"vi","target_duration_seconds":5,"scenes":[{"id":"s1","index":0,"narration":"Xin chao","caption":"Xin chao","duration_ms":5000}]}"#).unwrap();
    let artifact = ArtifactRef { artifact_id: "script-real".to_string(), kind: ArtifactKind::Script, produced_by_stage: StageId::StoryScript, location: path.to_string_lossy().to_string(), mime_type: Some("application/json".to_string()), metadata: Value::Null };
    let mut context = PipelineContext { job_id: "job-voice".to_string(), project_id: None, workflow_mode: "source_based".to_string(), content_source: Some(ContentSource::PromptOnly.as_str().to_string()), prompt: "p".to_string(), model_id: None, voice_id: Some("vi-VN-NamMinhNeural".to_string()), language: "vi".to_string(), target_duration_seconds: 5, output_mode: "draft_only".to_string(), source_url: None, local_file: None, story_url: None, research_enabled: false, research_platform: None, research_query: None, research_mode: None, xhs_variant: None, artifact_refs: vec![artifact], stage_states: PipelineContext::initial_stage_states() };
    let state = context.stage_states.iter_mut().find(|state| state.stage_id == StageId::StoryScript).unwrap();
    state.status = StageStatus::Completed;
    state.output_artifact_ids = vec!["script-real".to_string()];
    let input = prepare_voice(&context).unwrap();
    assert_eq!(input.script_artifact_id, "script-real");
    assert_eq!(input.voice, "vi-VN-NamMinhNeural");
    assert_eq!(input.language, "vi");
  }

  #[test]
  fn timing_uses_measured_audio_durations() {
    let timing = measured_timings(&[("a".into(), 0, "one".into(), 1.25), ("b".into(), 1, "two".into(), 2.0)]).unwrap();
    assert_eq!(timing[0].start_seconds, 0.0);
    assert_eq!(timing[0].end_seconds, 1.25);
    assert_eq!(timing[1].start_seconds, 1.25);
    assert_eq!(timing[1].end_seconds, 3.25);
  }

  #[test]
  fn invalid_measured_duration_is_rejected() {
    let error = measured_timings(&[("a".into(), 0, "one".into(), 0.0)]).unwrap_err();
    assert_eq!(error.code, "VOICE_TIMING_INVALID");
  }

  #[test]
  fn retry_only_allows_transient_failures_before_limit() {
    let transient = VoiceError::new("VOICE_TTS_UNAVAILABLE", "offline", true);
    assert!(should_retry(&transient, 1));
    assert!(!should_retry(&transient, 2));
    assert!(!should_retry(&VoiceError::new("VOICE_INPUT_INVALID", "bad", false), 1));
  }

  #[test]
  fn gtts_uses_language_while_named_voice_engines_use_voice() {
    let script = StructuredScript { title: "t".into(), hook: "h".into(), cta: "c".into(), language: "vi".into(), target_duration_seconds: 1, scenes: vec![] };
    let mut input = VoiceInput { script_artifact_id: "script".into(), script, voice: "vi-VN-HoaiMyNeural".into(), language: "vi".into(), model: "gtts/default".into() };
    assert_eq!(provider_voice(&input), "vi");
    input.model = "edgetts/vi-VN-HoaiMyNeural".into();
    assert_eq!(provider_voice(&input), "vi-VN-HoaiMyNeural");
  }

  #[tokio::test]
  #[ignore = "requires live OmniRoute TTS and ffmpeg/ffprobe"]
  async fn runtime_real_tts_produces_registered_artifacts() {
    let root = PathBuf::from(env::var("FLOWORD_PHASE4_RUNTIME_ROOT").expect("FLOWORD_PHASE4_RUNTIME_ROOT is required"));
    std::fs::create_dir_all(&root).unwrap();
    let script = StructuredScript { title: "Floword Phase 4 runtime".into(), hook: "Runtime TTS".into(), cta: "Verified".into(), language: "vi".into(), target_duration_seconds: 10, scenes: vec![crate::services::pipeline::clients::omniroute_client::ScriptScene { id: "scene-1".into(), index: 0, narration: "Xin chào. Đây là kiểm tra giọng nói thật.".into(), caption: "Xin chào".into(), visual_instruction: String::new(), search_keywords: vec![], emotion: String::new(), duration_ms: 0 }, crate::services::pipeline::clients::omniroute_client::ScriptScene { id: "scene-2".into(), index: 1, narration: "Thời lượng được đo trực tiếp bằng ffprobe.".into(), caption: "Đo bằng ffprobe".into(), visual_instruction: String::new(), search_keywords: vec![], emotion: String::new(), duration_ms: 0 }] };
    let input = VoiceInput { script_artifact_id: "runtime-script-artifact".into(), script, voice: "vi".into(), language: "vi".into(), model: "gtts/default".into() };
    let output = synthesize_voice(&input, &root, Arc::new(AtomicBool::new(false))).await.unwrap();
    let audio = ArtifactStore::register_typed_artifact(&root, "phase4-runtime", StageId::Voice, "omniroute_tts", ArtifactKind::VoiceAudio, &output.audio_path, json!({ "duration_seconds": output.timing.duration_seconds })).unwrap();
    let timing = ArtifactStore::register_typed_artifact(&root, "phase4-runtime", StageId::Voice, "vynaro_ffprobe", ArtifactKind::VoiceTiming, &output.timing_path, json!({ "source": "ffprobe" })).unwrap();
    audio.to_artifact_ref(StageId::Voice).unwrap().validate().unwrap();
    timing.to_artifact_ref(StageId::Voice).unwrap().validate().unwrap();
    assert_eq!(output.timing.segments.len(), 2);
    assert!(output.timing.duration_seconds > 0.0);
    assert!(output.size_bytes > 0);
    println!("{}", json!({ "voice_audio_artifact_id": audio.id, "voice_timing_artifact_id": timing.id, "audio_path": audio.path, "timing_path": timing.path, "size_bytes": output.size_bytes, "duration_seconds": output.timing.duration_seconds, "segments": output.timing.segments.len() }));
  }
}
