use crate::services::pipeline::capcut_automation_engine_manager::{CapcutAutomationEngineManager, CapcutEngineHealth, CapcutEngineKind, CapcutModelRecord};
use crate::services::pipeline::capcut_automation_transcription::{transcribe_local, LocalTranscriptionRequest, LocalTranscriptionResponse};
use crate::services::pipeline::capcut_automation_translation::{translate_local, LocalTranslationRequest, LocalTranslationResponse};
use crate::services::pipeline::capcut_automation_ocr::{recognize_local, translate_regions_local, LocalOcrRequest, LocalOcrResponse, LocalOcrTranslationRequest, LocalOcrTranslationResponse};
use crate::services::pipeline::capcut_automation_speaker::{detect_speakers_local, LocalSpeakerRequest, LocalSpeakerResponse};
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use voicestudio_client::VoiceStudioClient;

const ARTCRAFT_SPEECH_BASE_URL: &str = "http://127.0.0.1:3900";
const ARTCRAFT_SPEECH_SERVICE_NAME: &str = "artcraft-speech";

#[derive(Debug, Default, serde::Deserialize)]
pub struct EnsureArtcraftSpeechRuntimeRequest {
  #[serde(default)]
  pub accept_model_terms: bool,
}

fn push_unique_path(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
  if !candidates.iter().any(|path| path == &candidate) {
    candidates.push(candidate);
  }
}

fn resolve_artcraft_speech_entrypoint(app: &AppHandle) -> Option<PathBuf> {
  let mut candidates = Vec::new();
  push_unique_path(&mut candidates, PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tools/speech-runtime/src/artcraft_speech_runtime.py"));

  if let Ok(resource_dir) = app.path().resource_dir() {
    push_unique_path(&mut candidates, resource_dir.join("speech-runtime/src/artcraft_speech_runtime.py"));
    push_unique_path(&mut candidates, resource_dir.join("resources/speech-runtime/src/artcraft_speech_runtime.py"));
  }

  if let Ok(executable) = std::env::current_exe() {
    if let Some(directory) = executable.parent() {
      push_unique_path(&mut candidates, directory.join("resources/speech-runtime/src/artcraft_speech_runtime.py"));
    }
  }

  candidates.into_iter().find(|candidate| candidate.is_file())
}

async fn artcraft_speech_health() -> Result<Value, String> {
  let client = artcraft_speech_client()?;
  let health = client.health().await.map_err(|error| error.to_string())?;
  if health.get("service").and_then(Value::as_str) != Some(ARTCRAFT_SPEECH_SERVICE_NAME) {
    return Err("ARTCRAFT_SPEECH_PORT_IN_USE_BY_OTHER_SERVICE".to_string());
  }
  Ok(health)
}

fn artcraft_speech_client() -> Result<VoiceStudioClient, String> {
  VoiceStudioClient::new(ARTCRAFT_SPEECH_BASE_URL, None).map_err(|error| error.to_string())
}

fn require_python_311() -> Result<(), String> {
  let status = crate::core::lifecycle::startup::tasks::background_command::background_command(Command::new("py")).args(["-3.11", "--version"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|_| "ARTCRAFT_SPEECH_PYTHON_311_REQUIRED".to_string())?;
  if status.success() {
    Ok(())
  } else {
    Err("ARTCRAFT_SPEECH_PYTHON_311_REQUIRED".to_string())
  }
}

/// Starts ArtCraft's embedded speech service. It never installs Python,
/// dependencies, or model weights automatically because those are large
/// downloads subject to model terms the user must accept.
#[tauri::command]
pub async fn ensure_artcraft_speech_runtime(app: AppHandle, request: Option<EnsureArtcraftSpeechRuntimeRequest>) -> Result<Value, String> {
  let accept_model_terms = request.unwrap_or_default().accept_model_terms;
  match artcraft_speech_health().await {
    Ok(health) => {
      if accept_model_terms {
        artcraft_speech_client()?.accept_model_terms().await.map_err(|error| error.to_string())?;
      }
      return Ok(health);
    },
    Err(error) if error == "ARTCRAFT_SPEECH_PORT_IN_USE_BY_OTHER_SERVICE" => return Err(error),
    Err(_) => {},
  }

  require_python_311()?;
  let entrypoint = resolve_artcraft_speech_entrypoint(&app).ok_or_else(|| "ARTCRAFT_SPEECH_RUNTIME_NOT_PACKAGED".to_string())?;
  let data_dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("ArtCraft")).join("speech");
  std::fs::create_dir_all(&data_dir).map_err(|_| "ARTCRAFT_SPEECH_DATA_DIRECTORY_UNAVAILABLE".to_string())?;

  crate::core::lifecycle::startup::tasks::background_command::background_command(Command::new("py")).args(["-3.11"]).arg(&entrypoint).env("ARTCRAFT_SPEECH_DATA_DIR", &data_dir).env("ARTCRAFT_SPEECH_PORT", "3900").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_| "ARTCRAFT_SPEECH_RUNTIME_START_FAILED".to_string())?;

  for _ in 0..20 {
    tokio::time::sleep(Duration::from_millis(250)).await;
    if let Ok(health) = artcraft_speech_health().await {
      if accept_model_terms {
        artcraft_speech_client()?.accept_model_terms().await.map_err(|error| error.to_string())?;
      }
      return Ok(health);
    }
  }

  Err("ARTCRAFT_SPEECH_RUNTIME_START_TIMEOUT".to_string())
}

#[tauri::command]
pub fn list_capcut_automation_engines(manager: State<'_, CapcutAutomationEngineManager>) -> Vec<CapcutEngineHealth> {
  manager.health()
}

#[derive(serde::Deserialize)]
pub struct CapcutEngineKindRequest {
  pub kind: CapcutEngineKind,
}

#[tauri::command]
pub fn check_capcut_automation_engine(manager: State<'_, CapcutAutomationEngineManager>, request: CapcutEngineKindRequest) -> Vec<CapcutEngineHealth> {
  manager.check(request.kind)
}

#[tauri::command]
pub fn transcribe_capcut_automation_local(manager: State<'_, CapcutAutomationEngineManager>, request: LocalTranscriptionRequest) -> Result<LocalTranscriptionResponse, String> {
  transcribe_local(&manager, request)
}

#[tauri::command]
pub fn translate_capcut_automation_local(manager: State<'_, CapcutAutomationEngineManager>, request: LocalTranslationRequest) -> Result<LocalTranslationResponse, String> {
  translate_local(&manager, request)
}

#[tauri::command]
pub fn ocr_capcut_automation_local(manager: State<'_, CapcutAutomationEngineManager>, request: LocalOcrRequest) -> Result<LocalOcrResponse, String> {
  recognize_local(&manager, request)
}

#[tauri::command]
pub fn translate_ocr_capcut_automation_local(manager: State<'_, CapcutAutomationEngineManager>, request: LocalOcrTranslationRequest) -> Result<LocalOcrTranslationResponse, String> {
  translate_regions_local(&manager, request)
}

#[tauri::command]
pub fn detect_speakers_capcut_automation_local(manager: State<'_, CapcutAutomationEngineManager>, request: LocalSpeakerRequest) -> Result<LocalSpeakerResponse, String> {
  detect_speakers_local(&manager, request)
}

#[tauri::command]
pub fn list_capcut_ai_models(manager: State<'_, CapcutAutomationEngineManager>) -> Vec<CapcutModelRecord> {
  manager.list_models()
}

#[tauri::command]
pub fn get_capcut_ai_model_status(manager: State<'_, CapcutAutomationEngineManager>, id: String) -> Result<CapcutModelRecord, String> {
  manager.model(&id)
}

#[tauri::command]
pub async fn download_capcut_ai_model(manager: State<'_, CapcutAutomationEngineManager>, id: String) -> Result<CapcutModelRecord, String> {
  manager.download_model(&id).await
}

#[tauri::command]
pub fn cancel_capcut_ai_model_download(manager: State<'_, CapcutAutomationEngineManager>, id: String) -> Result<CapcutModelRecord, String> {
  // Cancelling is represented by removing the .partial file; the next call
  // starts from a clean artifact instead of resuming an unverified stream.
  let record = manager.model(&id)?;
  let partial = manager.install_root().join(format!("{}.partial", record.relative_path));
  if partial.exists() {
    std::fs::remove_file(partial).map_err(|_| "CAPCUT_MODEL_CANCEL_FAILED".to_string())?;
  }
  manager.model(&id)
}

#[tauri::command]
pub fn verify_capcut_ai_model(manager: State<'_, CapcutAutomationEngineManager>, id: String) -> Result<CapcutModelRecord, String> {
  manager.verify_model(&id)
}

#[tauri::command]
pub fn remove_capcut_ai_model(manager: State<'_, CapcutAutomationEngineManager>, id: String) -> Result<(), String> {
  manager.remove_model(&id)
}

/// Return the capabilities advertised by the configured VoiceStudio instance.
/// This keeps the frontend on the same HTTP contract as the pipeline workers.
#[tauri::command]
pub async fn get_voice_studio_capabilities() -> Result<Value, String> {
  let client = artcraft_speech_client()?;
  client.capabilities().await.map_err(|error| error.to_string())
}

/// List persisted voice profiles usable by cloned-voice synthesis.
#[tauri::command]
pub async fn list_voice_studio_profiles() -> Result<Value, String> {
  let client = artcraft_speech_client()?;
  client.voice_profiles().await.map_err(|error| error.to_string())
}

#[derive(Debug, serde::Deserialize)]
pub struct VoiceStudioUploadClipRequest {
  pub path: PathBuf,
  pub name: Option<String>,
  pub reference_text: Option<String>,
}

/// Store a consented reference clip as a real local voice profile.
#[tauri::command]
pub async fn upload_voice_studio_clip(request: VoiceStudioUploadClipRequest) -> Result<Value, String> {
  if !request.path.is_file() {
    return Err("VOICE_STUDIO_CLIP_NOT_FOUND".to_string());
  }

  let client = artcraft_speech_client()?;
  let fallback_name = request.path.file_stem().and_then(|value| value.to_str()).filter(|value| !value.trim().is_empty()).unwrap_or("ArtCraft voice clip");
  client.upload_voice_clip(&request.path, request.name.as_deref().filter(|value| !value.trim().is_empty()).unwrap_or(fallback_name), request.reference_text.as_deref()).await.map_err(|error| error.to_string())
}

#[derive(Debug, serde::Deserialize)]
pub struct SaveVoiceStudioProfileRequest {
  pub voice_id: String,
  pub profile_name: String,
}

/// Persist a gallery voice as a VoiceStudio profile usable by TTS/dubbing.
#[tauri::command]
pub async fn save_voice_studio_profile(request: SaveVoiceStudioProfileRequest) -> Result<Value, String> {
  let voice_id = request.voice_id.trim();
  let profile_name = request.profile_name.trim();
  if voice_id.is_empty() || profile_name.is_empty() {
    return Err("VOICE_STUDIO_PROFILE_INPUT_INVALID".to_string());
  }

  let client = artcraft_speech_client()?;
  client.save_voice_as_profile(voice_id, profile_name).await.map_err(|error| error.to_string())
}
