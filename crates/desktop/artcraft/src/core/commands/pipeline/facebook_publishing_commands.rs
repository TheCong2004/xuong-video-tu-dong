use crate::core::commands::response::failure_response_wrapper::{CommandErrorResponseWrapper, CommandErrorStatus};
use crate::core::commands::response::shorthand::ResponseOrError;
use crate::core::commands::response::success_response_wrapper::SerializeMarker;
use crate::core::commands::pipeline::floword_commands::FlowordErrorDetails;
use crate::core::state::task_database::TaskDatabase;
use crate::services::publishing::facebook_binding::{FacebookPageSnapshot, FacebookRuntimeBinding, LivePublishConfirmation};
use serde::{Deserialize, Serialize};
use sqlite_tasks::queries::floword_settings::get_floword_setting::get_floword_setting;
use sqlite_tasks::queries::floword_settings::upsert_floword_setting::{upsert_floword_setting, UpsertFlowordSettingArgs};
use tauri::State;

const SETTINGS_PREFIX: &str = "facebook_publishing";

fn setting_key(kind: &str, id: &str) -> String {
  format!("{SETTINGS_PREFIX}.{kind}.{}", id.trim())
}

fn validate_id(id: &str) -> Result<(), String> {
  if id.trim().is_empty() {
    Err("Facebook persistence id is required".to_string())
  } else {
    Ok(())
  }
}

fn contains_secret_marker(value: &str) -> bool {
  let lower = value.to_ascii_lowercase();
  ["bearer ", "authorization:", "cookie:", "token=", "ws://", "wss://"].iter().any(|marker| lower.contains(marker))
}

fn validate_runtime_binding(binding: &FacebookRuntimeBinding) -> Result<(), String> {
  let Some(runtime) = binding.runtime.as_ref() else {
    return Ok(());
  };
  let cdp = url::Url::parse(&runtime.cdp_endpoint).map_err(|_| "Invalid CDP endpoint".to_string())?;
  let host = cdp.host_str().unwrap_or_default();
  if cdp.scheme() != "http" || !matches!(host, "127.0.0.1" | "localhost" | "::1") || cdp.username() != "" || cdp.password().is_some() || cdp.query().is_some() || cdp.fragment().is_some() || cdp.port().is_none() || cdp.port().unwrap_or_default() == 0 || cdp.port().unwrap_or_default() != runtime.cdp_endpoint.rsplit(':').next().and_then(|value| value.parse::<u16>().ok()).unwrap_or_default() {
    return Err("Runtime identity must use loopback HTTP CDP without credentials or query data".to_string());
  }
  let page = url::Url::parse(&runtime.managed_page_url).map_err(|_| "Invalid Facebook managed page URL".to_string())?;
  let page_host = page.host_str().unwrap_or_default().to_ascii_lowercase();
  if page.scheme() != "https" || !(page_host == "facebook.com" || page_host.ends_with(".facebook.com")) || page.query().is_some() || page.fragment().is_some() || contains_secret_marker(&runtime.managed_page_url) {
    return Err("Runtime identity must use a sanitized Facebook page URL".to_string());
  }
  Ok(())
}

fn command_error(code: &str, message: impl Into<String>) -> CommandErrorResponseWrapper<(), FlowordErrorDetails> {
  CommandErrorResponseWrapper { status: CommandErrorStatus::BadRequest, error_message: Some(message.into()), error_type: Some(()), error_details: Some(FlowordErrorDetails { error_code: code.to_string(), job_id: None }) }
}

fn internal_error(message: impl Into<String>) -> CommandErrorResponseWrapper<(), FlowordErrorDetails> {
  CommandErrorResponseWrapper { status: CommandErrorStatus::ServerError, error_message: Some(message.into()), error_type: Some(()), error_details: Some(FlowordErrorDetails { error_code: "INTERNAL_ERROR".to_string(), job_id: None }) }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacebookRuntimeBindingRequest {
  pub binding: FacebookRuntimeBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacebookRuntimeBindingResponse {
  pub binding: FacebookRuntimeBinding,
}
impl SerializeMarker for FacebookRuntimeBindingResponse {}

#[tauri::command]
pub async fn upsert_facebook_runtime_binding_command(task_database: State<'_, TaskDatabase>, request: FacebookRuntimeBindingRequest) -> ResponseOrError<FacebookRuntimeBindingResponse, FlowordErrorDetails> {
  let binding = request.binding;
  if let Err(message) = validate_id(&binding.binding_id) {
    return Err(command_error("FACEBOOK_BINDING_INVALID", message));
  }
  if binding.donut_profile_id.trim().is_empty() || binding.target_kind != "FACEBOOK" {
    return Err(command_error("FACEBOOK_BINDING_INVALID", "Facebook binding requires a profile and targetKind=FACEBOOK"));
  }
  if let Err(message) = validate_runtime_binding(&binding) {
    return Err(command_error("FACEBOOK_BINDING_INVALID", message));
  }
  let value_json = serde_json::to_string(&binding).map_err(|e| internal_error(format!("Failed to encode Facebook runtime binding: {e}")))?;
  let setting = upsert_floword_setting(task_database.get_connection(), UpsertFlowordSettingArgs { key: setting_key("runtime_binding", &binding.binding_id), value_json }).await.map_err(|e| internal_error(format!("Failed to persist Facebook runtime binding: {e}")))?;
  let stored = serde_json::from_str(&setting.value_json).map_err(|e| internal_error(format!("Persisted Facebook runtime binding is invalid: {e}")))?;
  Ok(FacebookRuntimeBindingResponse { binding: stored }.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFacebookBindingRequest {
  pub binding_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFacebookBindingResponse {
  pub binding: Option<FacebookRuntimeBinding>,
}
impl SerializeMarker for GetFacebookBindingResponse {}

#[tauri::command]
pub async fn get_facebook_runtime_binding_command(task_database: State<'_, TaskDatabase>, request: GetFacebookBindingRequest) -> ResponseOrError<GetFacebookBindingResponse, FlowordErrorDetails> {
  if let Err(message) = validate_id(&request.binding_id) {
    return Err(command_error("FACEBOOK_BINDING_INVALID", message));
  }
  let setting = get_floword_setting(task_database.get_connection(), &setting_key("runtime_binding", &request.binding_id)).await.map_err(|e| internal_error(format!("Failed to read Facebook runtime binding: {e}")))?;
  let binding = setting.map(|row| serde_json::from_str(&row.value_json)).transpose().map_err(|e| internal_error(format!("Persisted Facebook runtime binding is invalid: {e}")))?;
  Ok(GetFacebookBindingResponse { binding }.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacebookPageSnapshotRequest {
  pub snapshot: FacebookPageSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacebookPageSnapshotResponse {
  pub snapshot: FacebookPageSnapshot,
}
impl SerializeMarker for FacebookPageSnapshotResponse {}

#[tauri::command]
pub async fn upsert_facebook_page_snapshot_command(task_database: State<'_, TaskDatabase>, request: FacebookPageSnapshotRequest) -> ResponseOrError<FacebookPageSnapshotResponse, FlowordErrorDetails> {
  let snapshot = request.snapshot;
  if let Err(message) = validate_id(&snapshot.facebook_page_binding_id) {
    return Err(command_error("FACEBOOK_PAGE_IDENTITY_UNKNOWN", message));
  }
  if snapshot.donut_profile_id.trim().is_empty() || snapshot.facebook_page_display_name.trim().is_empty() || snapshot.identity_evidence.trim().is_empty() || contains_secret_marker(&snapshot.identity_evidence) || snapshot.facebook_page_id.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_none() && snapshot.facebook_page_canonical_url.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_none() {
    return Err(command_error("FACEBOOK_PAGE_IDENTITY_UNKNOWN", "Facebook Page snapshot requires an id or canonical URL, display name, and DOM evidence"));
  }
  let value_json = serde_json::to_string(&snapshot).map_err(|e| internal_error(format!("Failed to encode Facebook Page snapshot: {e}")))?;
  let setting = upsert_floword_setting(task_database.get_connection(), UpsertFlowordSettingArgs { key: setting_key("page_snapshot", &snapshot.facebook_page_binding_id), value_json }).await.map_err(|e| internal_error(format!("Failed to persist Facebook Page snapshot: {e}")))?;
  let stored = serde_json::from_str(&setting.value_json).map_err(|e| internal_error(format!("Persisted Facebook Page snapshot is invalid: {e}")))?;
  Ok(FacebookPageSnapshotResponse { snapshot: stored }.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFacebookPageSnapshotRequest {
  pub binding_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFacebookPageSnapshotResponse {
  pub snapshot: Option<FacebookPageSnapshot>,
}
impl SerializeMarker for GetFacebookPageSnapshotResponse {}

#[tauri::command]
pub async fn get_facebook_page_snapshot_command(task_database: State<'_, TaskDatabase>, request: GetFacebookPageSnapshotRequest) -> ResponseOrError<GetFacebookPageSnapshotResponse, FlowordErrorDetails> {
  if let Err(message) = validate_id(&request.binding_id) {
    return Err(command_error("FACEBOOK_PAGE_IDENTITY_UNKNOWN", message));
  }
  let setting = get_floword_setting(task_database.get_connection(), &setting_key("page_snapshot", &request.binding_id)).await.map_err(|e| internal_error(format!("Failed to read Facebook Page snapshot: {e}")))?;
  let snapshot = setting.map(|row| serde_json::from_str(&row.value_json)).transpose().map_err(|e| internal_error(format!("Persisted Facebook Page snapshot is invalid: {e}")))?;
  Ok(GetFacebookPageSnapshotResponse { snapshot }.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivePublishConfirmationRequest {
  pub confirmation: LivePublishConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivePublishConfirmationResponse {
  pub confirmation: LivePublishConfirmation,
}
impl SerializeMarker for LivePublishConfirmationResponse {}

#[tauri::command]
pub async fn upsert_live_publish_confirmation_command(task_database: State<'_, TaskDatabase>, request: LivePublishConfirmationRequest) -> ResponseOrError<LivePublishConfirmationResponse, FlowordErrorDetails> {
  let confirmation = request.confirmation;
  if let Err(message) = validate_id(&confirmation.publication_id) {
    return Err(command_error("LIVE_PUBLISH_CONFIRMATION_INVALIDATED", message));
  }
  if confirmation.scheduled_occurrence_id.trim().is_empty() || confirmation.donut_profile_id.trim().is_empty() || confirmation.video_sha256.trim().is_empty() || confirmation.caption_sha256.trim().is_empty() || !confirmation.confirmed_by_user || confirmation.confirmation_expires_at_utc <= confirmation.confirmation_created_at_utc {
    return Err(command_error("LIVE_PUBLISH_CONFIRMATION_INVALIDATED", "Confirmation must be explicit, exact, and time-bounded"));
  }
  let value_json = serde_json::to_string(&confirmation).map_err(|e| internal_error(format!("Failed to encode live publish confirmation: {e}")))?;
  let setting = upsert_floword_setting(task_database.get_connection(), UpsertFlowordSettingArgs { key: setting_key("confirmation", &confirmation.publication_id), value_json }).await.map_err(|e| internal_error(format!("Failed to persist live publish confirmation: {e}")))?;
  let stored = serde_json::from_str(&setting.value_json).map_err(|e| internal_error(format!("Persisted live publish confirmation is invalid: {e}")))?;
  Ok(LivePublishConfirmationResponse { confirmation: stored }.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLivePublishConfirmationRequest {
  pub publication_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLivePublishConfirmationResponse {
  pub confirmation: Option<LivePublishConfirmation>,
}
impl SerializeMarker for GetLivePublishConfirmationResponse {}

#[tauri::command]
pub async fn get_live_publish_confirmation_command(task_database: State<'_, TaskDatabase>, request: GetLivePublishConfirmationRequest) -> ResponseOrError<GetLivePublishConfirmationResponse, FlowordErrorDetails> {
  if let Err(message) = validate_id(&request.publication_id) {
    return Err(command_error("LIVE_PUBLISH_CONFIRMATION_INVALIDATED", message));
  }
  let setting = get_floword_setting(task_database.get_connection(), &setting_key("confirmation", &request.publication_id)).await.map_err(|e| internal_error(format!("Failed to read live publish confirmation: {e}")))?;
  let confirmation = setting.map(|row| serde_json::from_str(&row.value_json)).transpose().map_err(|e| internal_error(format!("Persisted live publish confirmation is invalid: {e}")))?;
  Ok(GetLivePublishConfirmationResponse { confirmation }.into())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::publishing::facebook_binding::FacebookRuntimeIdentity;

  #[test]
  fn persistence_keys_are_scoped_and_trimmed() {
    assert_eq!(setting_key("runtime_binding", " binding-1 "), "facebook_publishing.runtime_binding.binding-1");
  }

  #[test]
  fn secret_markers_are_rejected_from_evidence() {
    assert!(contains_secret_marker("Authorization: Bearer abc"));
    assert!(contains_secret_marker("https://example.test/?token=abc"));
    assert!(!contains_secret_marker("publishing_surface:page_header"));
  }

  #[test]
  fn runtime_validation_requires_sanitized_loopback_facebook_identity() {
    let binding = FacebookRuntimeBinding { binding_id: "binding-1".into(), donut_profile_id: "profile-1".into(), target_kind: "FACEBOOK".into(), enabled: true, auto_start_managed_donut_profile: false, created_at_utc: 1, updated_at_utc: 2, runtime: Some(FacebookRuntimeIdentity { browser_pid: 1, launch_generation: 1, cdp_endpoint: "http://127.0.0.1:9222".into(), managed_target_id: "target-1".into(), managed_page_url: "https://www.facebook.com/page".into(), claimed_at_utc: 1, last_validated_at_utc: 1 }) };
    assert!(validate_runtime_binding(&binding).is_ok());
    let mut invalid = binding;
    invalid.runtime.as_mut().unwrap().cdp_endpoint = "http://127.0.0.1:9222?token=secret".into();
    assert!(validate_runtime_binding(&invalid).is_err());
  }
}
