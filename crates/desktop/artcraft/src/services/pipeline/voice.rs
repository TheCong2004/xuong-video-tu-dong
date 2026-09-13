use crate::services::pipeline::clients::omniroute_client::StructuredScript;
use crate::services::pipeline::contracts::{ArtifactKind, PipelineContext, PipelineContractError, StageId};
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use voicestudio_client::{SpeechRequest as VoiceStudioSpeechRequest, VoiceStudioClient};

const ARTCRAFT_SPEECH_BASE_URL: &str = "http://127.0.0.1:3900";
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;
use std::process::Stdio;
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
  pub piper_executable: Option<PathBuf>,
  pub piper_model: Option<PathBuf>,
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
  // A VoiceStudio profile is selected explicitly through the environment so
  // native CapCut Automation jobs can use a cloned voice without changing the
  // persisted pipeline contract.  The context voice remains the fallback for
  // the existing local/OmniRoute providers.
  let fallback_voice = context.voice_id.as_deref().filter(|value| !value.trim().is_empty()).map(str::to_string).unwrap_or_else(|| default_voice(&context.language).to_string());
  let named_voice = configured_voice_id(&fallback_voice);
  let model = configured_tts_model();
  let voice = if model.starts_with("gtts/") { context.language.clone() } else { named_voice };
  Ok(VoiceInput { script_artifact_id: script_id.clone(), script, voice, language: context.language.clone(), model, piper_executable: None, piper_model: None })
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
  let mut local_input = input.clone();
  if input.model == "piper" {
    let manager = app.state::<crate::services::pipeline::capcut_automation_engine_manager::CapcutAutomationEngineManager>();
    let engine = manager.inner().resolve_first(crate::services::pipeline::capcut_automation_engine_manager::CapcutEngineKind::TextToSpeech).map_err(|error| VoiceError::new("VOICE_LOCAL_ENGINE_NOT_READY", error, false))?;
    local_input.piper_executable = engine.executable_path;
    local_input.piper_model = Some(engine.resource_path);
  }
  synthesize_voice_with_ffmpeg(&local_input, work_dir, cancel_flag, ffmpeg).await
}

async fn synthesize_voice_with_ffmpeg(input: &VoiceInput, work_dir: &Path, cancel_flag: Arc<AtomicBool>, ffmpeg: Ffmpeg) -> Result<VoiceRuntimeOutput, VoiceError> {
  if input.script.scenes.is_empty() {
    return Err(VoiceError::new("VOICE_INPUT_INVALID", "Script contains no scenes", false));
  }
  preflight_voicestudio(input, &cancel_flag).await?;
  let voice_dir = work_dir.join("voice");
  let clips_dir = voice_dir.join("clips");
  std::fs::create_dir_all(&clips_dir).map_err(|error| VoiceError::new("VOICE_ARTIFACT_FAILED", error.to_string(), false))?;
  let client = Client::builder().timeout(Duration::from_secs(60)).build().map_err(|error| VoiceError::new("VOICE_TTS_UNAVAILABLE", error.to_string(), true))?;
  let narrations = input
    .script
    .scenes
    .iter()
    .enumerate()
    .map(|(scene_position, scene)| {
      let narration = normalize_narration_for_tts(scene.narration.trim(), scene_position + 1 == input.script.scenes.len());
      if narration.is_empty() {
        Err(VoiceError::new("VOICE_INPUT_INVALID", format!("Scene {} narration is empty", scene.id), false))
      } else {
        Ok(narration)
      }
    })
    .collect::<Result<Vec<_>, _>>()?;
  if cancel_flag.load(Ordering::SeqCst) {
    return Err(VoiceError::cancelled());
  }
  let audio_results = if use_voicestudio_provider(&input.model) {
    request_voicestudio_batch(input, &narrations, &cancel_flag).await?
  } else {
    let mut outputs = Vec::with_capacity(narrations.len());
    for narration in &narrations {
      outputs.push(request_speech(&client, input, narration, &cancel_flag).await?);
    }
    outputs
  };
  let mut clip_paths = Vec::with_capacity(input.script.scenes.len());
  let mut measured = Vec::with_capacity(input.script.scenes.len());

  for (scene, (narration, audio)) in input.script.scenes.iter().zip(narrations.into_iter().zip(audio_results.into_iter())) {
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
    measured.push((scene.id.clone(), scene.index, narration, probe.duration_seconds));
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

async fn preflight_voicestudio(input: &VoiceInput, cancel_flag: &Arc<AtomicBool>) -> Result<(), VoiceError> {
  if !use_voicestudio_provider(&input.model) {
    return Ok(());
  }
  if cancel_flag.load(Ordering::SeqCst) {
    return Err(VoiceError::cancelled());
  }
  let client = speech_provider_client(&input.model)?;
  tokio::time::timeout(Duration::from_secs(5), client.health()).await.map_err(|_| VoiceError::new("VOICE_STUDIO_HEALTH_TIMEOUT", "VoiceStudio health check timed out", true))?.map_err(|error| VoiceError::new("VOICE_STUDIO_UNAVAILABLE", error.to_string(), true))?;
  Ok(())
}

struct SpeechAudio {
  bytes: Vec<u8>,
  extension: &'static str,
}

async fn request_speech(client: &Client, input: &VoiceInput, text: &str, cancel_flag: &Arc<AtomicBool>) -> Result<SpeechAudio, VoiceError> {
  if input.model == "piper" {
    return request_piper_speech(input, text, cancel_flag).await;
  }
  if use_voicestudio_provider(&input.model) {
    return request_voicestudio_speech(input, text, cancel_flag).await;
  }
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

fn use_voicestudio_provider(model: &str) -> bool {
  let provider = env::var("FLOWORD_VOICE_PROVIDER").unwrap_or_default();
  provider.eq_ignore_ascii_case("voicestudio") || model.eq_ignore_ascii_case("voicestudio") || model.to_ascii_lowercase().starts_with("voicestudio/") || use_artcraft_speech_provider(model)
}

fn use_artcraft_speech_provider(model: &str) -> bool {
  model.eq_ignore_ascii_case("artcraft-speech") || model.to_ascii_lowercase().starts_with("artcraft-speech/")
}

fn speech_provider_client(model: &str) -> Result<VoiceStudioClient, VoiceError> {
  if use_artcraft_speech_provider(model) {
    VoiceStudioClient::new(ARTCRAFT_SPEECH_BASE_URL, None).map_err(|error| VoiceError::new("ARTCRAFT_SPEECH_UNAVAILABLE", error.to_string(), true))
  } else {
    VoiceStudioClient::from_env().map_err(|error| VoiceError::new("VOICE_STUDIO_UNAVAILABLE", error.to_string(), true))
  }
}

pub(crate) fn configured_tts_model() -> String {
  let provider = env::var("FLOWORD_VOICE_PROVIDER").unwrap_or_default();
  let configured = env::var("FLOWORD_TTS_MODEL").ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
  if provider.eq_ignore_ascii_case("voicestudio") && configured.as_deref().map(|value| value.eq_ignore_ascii_case("piper")).unwrap_or(true) {
    "voicestudio".to_string()
  } else {
    configured.unwrap_or_else(|| "piper".to_string())
  }
}

pub(crate) fn configured_voice_id(fallback: &str) -> String {
  env::var("VOICESTUDIO_VOICE_ID").ok().or_else(|| env::var("FLOWORD_VOICE_ID").ok()).filter(|value| !value.trim().is_empty()).unwrap_or_else(|| fallback.to_string())
}

async fn request_voicestudio_speech(input: &VoiceInput, text: &str, cancel_flag: &Arc<AtomicBool>) -> Result<SpeechAudio, VoiceError> {
  let client = speech_provider_client(&input.model)?;
  let request = voicestudio_request(input, text, None);
  let response = tokio::select! {
    result = client.synthesize(&request) => result,
    _ = wait_for_cancel(cancel_flag) => return Err(VoiceError::cancelled()),
  }
  .map_err(|error| VoiceError::new("VOICE_STUDIO_TTS_FAILED", error.to_string(), true))?;
  speech_audio_from_voicestudio(response)
}

async fn request_voicestudio_batch(input: &VoiceInput, narrations: &[String], cancel_flag: &Arc<AtomicBool>) -> Result<Vec<SpeechAudio>, VoiceError> {
  let client = speech_provider_client(&input.model)?;
  let requests = narrations
    .iter()
    .enumerate()
    .map(|(index, text)| {
      let duration = input.script.scenes.get(index).map(|scene| scene.duration_ms as f32 / 1_000.0).filter(|duration| duration.is_finite() && *duration > 0.0);
      voicestudio_request(input, text, duration)
    })
    .collect::<Vec<_>>();
  let concurrency = env::var("FLOWORD_VOICE_BATCH_CONCURRENCY").ok().and_then(|value| value.trim().parse::<usize>().ok()).map(|value| value.clamp(1, 10)).unwrap_or(3);
  let responses = tokio::select! {
    result = client.synthesize_batch(requests, concurrency) => result,
    _ = wait_for_cancel(cancel_flag) => return Err(VoiceError::cancelled()),
  }
  .map_err(|error| VoiceError::new("VOICE_STUDIO_TTS_FAILED", error.to_string(), true))?;
  responses.into_iter().map(speech_audio_from_voicestudio).collect()
}

fn voicestudio_request(input: &VoiceInput, text: &str, duration: Option<f32>) -> VoiceStudioSpeechRequest {
  let model = input.model.strip_prefix("artcraft-speech/").map(str::to_owned).or_else(|| env::var("VOICESTUDIO_TTS_MODEL").ok().filter(|value| !value.trim().is_empty())).unwrap_or_else(|| "omnivoice".to_string());
  VoiceStudioSpeechRequest { model, input: text.to_string(), voice: input.voice.clone(), response_format: "mp3".to_string(), speed: 1.0, language: Some(input.language.clone()), instruct: None, duration }
}

fn speech_audio_from_voicestudio(response: voicestudio_client::SpeechResponse) -> Result<SpeechAudio, VoiceError> {
  let content_type = response.content_type.to_ascii_lowercase();
  let extension = if content_type.contains("wav") {
    "wav"
  } else if content_type.contains("mpeg") || content_type.contains("mp3") {
    "mp3"
  } else {
    return Err(VoiceError::new("VOICE_AUDIO_INVALID", format!("VoiceStudio returned unsupported content type: {}", response.content_type), false));
  };
  Ok(SpeechAudio { bytes: response.bytes, extension })
}

async fn request_piper_speech(input: &VoiceInput, text: &str, cancel_flag: &Arc<AtomicBool>) -> Result<SpeechAudio, VoiceError> {
  let executable = input.piper_executable.as_ref().ok_or_else(|| VoiceError::new("VOICE_LOCAL_ENGINE_NOT_READY", "Piper executable is not resolved", false))?;
  let model = input.piper_model.as_ref().ok_or_else(|| VoiceError::new("VOICE_LOCAL_ENGINE_NOT_READY", "Piper voice model is not resolved", false))?;
  let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|value| value.as_nanos()).unwrap_or_default();
  let output_path = env::temp_dir().join(format!("artcraft-piper-{}-{suffix}.wav", std::process::id()));
  let mut child = TokioCommand::new(executable).current_dir(executable.parent().unwrap_or_else(|| Path::new("."))).arg("--model").arg(model).arg("--output_file").arg(&output_path).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|error| VoiceError::new("VOICE_PIPER_START_FAILED", error.to_string(), false))?;
  if let Some(mut stdin) = child.stdin.take() {
    stdin.write_all(text.as_bytes()).await.map_err(|error| VoiceError::new("VOICE_PIPER_WRITE_FAILED", error.to_string(), false))?;
    drop(stdin);
  }
  let status = tokio::select! {
    result = child.wait() => result.map_err(|error| VoiceError::new("VOICE_PIPER_WAIT_FAILED", error.to_string(), false))?,
    _ = wait_for_cancel(cancel_flag) => {
      let _ = child.kill().await;
      let _ = tokio::fs::remove_file(&output_path).await;
      return Err(VoiceError::cancelled());
    }
  };
  if !status.success() {
    let _ = tokio::fs::remove_file(&output_path).await;
    return Err(VoiceError::new("VOICE_PIPER_FAILED", format!("Piper exited with status {status}"), false));
  }
  let bytes = tokio::fs::read(&output_path).await.map_err(|error| VoiceError::new("VOICE_PIPER_OUTPUT_MISSING", error.to_string(), false))?;
  let _ = tokio::fs::remove_file(&output_path).await;
  if bytes.is_empty() {
    return Err(VoiceError::new("VOICE_AUDIO_INVALID", "Piper returned empty audio", false));
  }
  Ok(SpeechAudio { bytes, extension: "wav" })
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

fn normalize_narration_for_tts(text: &str, is_last: bool) -> String {
  let mut normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
  if normalized.is_empty() {
    return normalized;
  }
  let has_terminal_punctuation = normalized.chars().last().is_some_and(|character| ".!?;:,\u{3002}\u{3001}\u{FF01}\u{FF0C}\u{FF1A}\u{FF1B}\u{FF1F}\u{2026}".contains(character));
  if !has_terminal_punctuation {
    normalized.push(if is_last { '.' } else { ',' });
  }
  normalized
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
  fn narration_normalization_adds_boundary_prosody_without_overwriting_punctuation() {
    assert_eq!(normalize_narration_for_tts("Xin chào", false), "Xin chào,");
    assert_eq!(normalize_narration_for_tts("Đã xong!", true), "Đã xong!");
    assert_eq!(normalize_narration_for_tts("你好", true), "你好.");
    assert_eq!(normalize_narration_for_tts("   ", false), "");
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
    let mut input = VoiceInput { script_artifact_id: "script".into(), script, voice: "vi-VN-HoaiMyNeural".into(), language: "vi".into(), model: "gtts/default".into(), piper_executable: None, piper_model: None };
    assert_eq!(provider_voice(&input), "vi");
    input.model = "edgetts/vi-VN-HoaiMyNeural".into();
    assert_eq!(provider_voice(&input), "vi-VN-HoaiMyNeural");
  }

  #[test]
  fn voicestudio_model_selects_voice_studio_provider() {
    assert!(use_voicestudio_provider("voicestudio"));
    assert!(use_voicestudio_provider("voicestudio/omnivoice"));
    assert!(!use_voicestudio_provider("piper"));
  }

  #[tokio::test]
  #[ignore = "requires live OmniRoute TTS and ffmpeg/ffprobe"]
  async fn runtime_real_tts_produces_registered_artifacts() {
    let root = PathBuf::from(env::var("FLOWORD_PHASE4_RUNTIME_ROOT").expect("FLOWORD_PHASE4_RUNTIME_ROOT is required"));
    std::fs::create_dir_all(&root).unwrap();
    let script = StructuredScript { title: "Floword Phase 4 runtime".into(), hook: "Runtime TTS".into(), cta: "Verified".into(), language: "vi".into(), target_duration_seconds: 10, scenes: vec![crate::services::pipeline::clients::omniroute_client::ScriptScene { id: "scene-1".into(), index: 0, narration: "Xin chào. Đây là kiểm tra giọng nói thật.".into(), caption: "Xin chào".into(), visual_instruction: String::new(), search_keywords: vec![], emotion: String::new(), duration_ms: 0 }, crate::services::pipeline::clients::omniroute_client::ScriptScene { id: "scene-2".into(), index: 1, narration: "Thời lượng được đo trực tiếp bằng ffprobe.".into(), caption: "Đo bằng ffprobe".into(), visual_instruction: String::new(), search_keywords: vec![], emotion: String::new(), duration_ms: 0 }] };
    let input = VoiceInput { script_artifact_id: "runtime-script-artifact".into(), script, voice: "vi".into(), language: "vi".into(), model: "gtts/default".into(), piper_executable: None, piper_model: None };
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
