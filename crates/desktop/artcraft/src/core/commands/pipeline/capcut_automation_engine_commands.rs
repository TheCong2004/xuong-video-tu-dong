use crate::services::pipeline::capcut_automation_engine_manager::{CapcutAutomationEngineManager, CapcutEngineHealth, CapcutEngineKind, CapcutModelRecord};
use crate::services::pipeline::capcut_automation_transcription::{transcribe_local, LocalTranscriptionRequest, LocalTranscriptionResponse};
use crate::services::pipeline::capcut_automation_translation::{translate_local, LocalTranslationRequest, LocalTranslationResponse};
use crate::services::pipeline::capcut_automation_ocr::{recognize_local, translate_regions_local, LocalOcrRequest, LocalOcrResponse, LocalOcrTranslationRequest, LocalOcrTranslationResponse};
use crate::services::pipeline::capcut_automation_speaker::{detect_speakers_local, LocalSpeakerRequest, LocalSpeakerResponse};
use tauri::State;

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
