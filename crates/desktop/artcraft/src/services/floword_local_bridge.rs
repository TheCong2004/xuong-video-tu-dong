//! Version 1 of the local ArtCraft <-> DonutBrowser render contract.
//!
//! This module is intentionally transport-neutral.  It defines the data that
//! a future local bridge transports, but does not open a port, spawn a browser
//! or submit media to a provider.  Keeping that boundary pure lets both EXEs
//! reject malformed work before any provider-side effect occurs.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION: &str = "1.0";
pub const FLOWORD_LOCAL_BRIDGE_SCHEMA_SHA256: &str = "78cac7b7a5ea450c4bd36b3565b559dcdfc4a2132d6129aaa3b48f350c988ba5";
pub const GROK_IMAGE_EDIT_PROVIDER: &str = "grok.image.edit";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SceneRenderProvider {
  #[serde(rename = "grok.image.edit")]
  GrokImageEdit,
}

impl SceneRenderProvider {
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::GrokImageEdit => GROK_IMAGE_EDIT_PROVIDER,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalArtifactRef {
  pub path: String,
  pub sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneRenderMetadata {
  pub width: Option<u32>,
  pub height: Option<u32>,
  pub aspect_ratio: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneRenderRequestV1 {
  pub schema_version: String,
  pub request_id: String,
  pub job_id: String,
  pub attempt_id: String,
  pub scene_id: String,
  pub character_id: String,
  pub character_version: u32,
  pub provider: SceneRenderProvider,
  pub anchor: LocalArtifactRef,
  pub compiled_prompt: String,
  pub compiled_prompt_sha256: String,
  pub idempotency_key: String,
  #[serde(default)]
  pub metadata: SceneRenderMetadata,
  pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SceneRenderStatus {
  Succeeded,
  Failed,
  Rejected,
  Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubmissionState {
  NotSubmitted,
  Submitted,
  Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedArtifactRef {
  pub path: String,
  pub sha256: String,
  pub mime_type: String,
  pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneRenderReceiptV1 {
  pub schema_version: String,
  pub request_id: String,
  pub job_id: String,
  pub scene_id: String,
  pub attempt_id: String,
  pub idempotency_key: String,
  pub provider: SceneRenderProvider,
  pub provider_job_id: Option<String>,
  pub status: SceneRenderStatus,
  pub submission_state: SubmissionState,
  pub artifact: Option<RenderedArtifactRef>,
  pub submitted_at: Option<String>,
  pub completed_at: String,
  pub error_code: Option<String>,
  pub error_message: Option<String>,
}

pub fn compute_idempotency_key(job_id: &str, scene_id: &str, attempt_id: &str, anchor_sha256: &str, compiled_prompt_sha256: &str, provider: SceneRenderProvider) -> String {
  let material = format!("{job_id}|{scene_id}|{attempt_id}|{anchor_sha256}|{compiled_prompt_sha256}|{}", provider.as_str());
  Sha256::digest(material.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

impl SceneRenderRequestV1 {
  pub fn validate(&self) -> Result<(), String> {
    if self.schema_version != FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION {
      return Err("BRIDGE_PROTOCOL_INCOMPATIBLE".to_string());
    }
    validate_uuid("requestId", &self.request_id)?;
    validate_uuid("jobId", &self.job_id)?;
    validate_uuid("attemptId", &self.attempt_id)?;
    validate_non_empty("sceneId", &self.scene_id)?;
    validate_non_empty("characterId", &self.character_id)?;
    if self.character_version == 0 {
      return Err("CHARACTER_VERSION_INVALID".to_string());
    }
    validate_local_path("anchor.path", &self.anchor.path)?;
    validate_sha256("anchor.sha256", &self.anchor.sha256)?;
    validate_non_empty("compiledPrompt", &self.compiled_prompt)?;
    validate_sha256("compiledPromptSha256", &self.compiled_prompt_sha256)?;
    validate_non_empty("createdAt", &self.created_at)?;
    validate_metadata(&self.metadata)?;

    let expected = compute_idempotency_key(&self.job_id, &self.scene_id, &self.attempt_id, &self.anchor.sha256, &self.compiled_prompt_sha256, self.provider);
    if self.idempotency_key != expected {
      return Err("IDEMPOTENCY_KEY_MISMATCH".to_string());
    }
    Ok(())
  }
}

impl SceneRenderReceiptV1 {
  pub fn validate_for(&self, request: &SceneRenderRequestV1) -> Result<(), String> {
    request.validate()?;
    if self.schema_version != FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION {
      return Err("BRIDGE_PROTOCOL_INCOMPATIBLE".to_string());
    }
    if self.request_id != request.request_id || self.job_id != request.job_id || self.scene_id != request.scene_id || self.attempt_id != request.attempt_id || self.idempotency_key != request.idempotency_key || self.provider != request.provider {
      return Err("RECEIPT_CORRELATION_MISMATCH".to_string());
    }
    validate_non_empty("completedAt", &self.completed_at)?;
    if let Some(value) = &self.provider_job_id {
      validate_non_empty("providerJobId", value)?;
    }
    if let Some(value) = &self.submitted_at {
      validate_non_empty("submittedAt", value)?;
    }
    if let Some(value) = &self.error_code {
      validate_non_empty("errorCode", value)?;
    }
    if let Some(value) = &self.error_message {
      if value.len() > 1024 || value.contains('\0') {
        return Err("RECEIPT_ERROR_MESSAGE_INVALID".to_string());
      }
    }

    match self.status {
      SceneRenderStatus::Succeeded => {
        if self.submission_state != SubmissionState::Submitted {
          return Err("SUCCESS_RECEIPT_NOT_SUBMITTED".to_string());
        }
        let artifact = self.artifact.as_ref().ok_or_else(|| "SUCCESS_ARTIFACT_REQUIRED".to_string())?;
        validate_local_path("artifact.path", &artifact.path)?;
        validate_sha256("artifact.sha256", &artifact.sha256)?;
        validate_non_empty("artifact.mimeType", &artifact.mime_type)?;
        if artifact.bytes == 0 {
          return Err("SUCCESS_ARTIFACT_EMPTY".to_string());
        }
        if self.error_code.is_some() || self.error_message.is_some() {
          return Err("SUCCESS_RECEIPT_HAS_ERROR".to_string());
        }
      },
      SceneRenderStatus::Failed | SceneRenderStatus::Rejected => {
        if self.artifact.is_some() {
          return Err("FAILURE_RECEIPT_HAS_ARTIFACT".to_string());
        }
        if self.error_code.is_none() {
          return Err("FAILURE_ERROR_CODE_REQUIRED".to_string());
        }
      },
      SceneRenderStatus::Unknown => {
        if self.submission_state == SubmissionState::NotSubmitted {
          return Err("UNKNOWN_RECEIPT_SUBMISSION_STATE_INVALID".to_string());
        }
        if self.artifact.is_some() {
          return Err("UNKNOWN_RECEIPT_HAS_ARTIFACT".to_string());
        }
      },
    }
    Ok(())
  }
}

fn validate_uuid(field: &str, value: &str) -> Result<(), String> {
  Uuid::parse_str(value).map(|_| ()).map_err(|_| format!("{field}_INVALID"))
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
  if value.trim().is_empty() || value.contains('\0') {
    Err(format!("{field}_REQUIRED"))
  } else {
    Ok(())
  }
}

fn validate_local_path(field: &str, value: &str) -> Result<(), String> {
  validate_non_empty(field, value)
}

fn validate_sha256(field: &str, value: &str) -> Result<(), String> {
  if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    Ok(())
  } else {
    Err(format!("{field}_INVALID"))
  }
}

fn validate_metadata(metadata: &SceneRenderMetadata) -> Result<(), String> {
  if matches!(metadata.width, Some(0)) || matches!(metadata.height, Some(0)) {
    return Err("RENDER_DIMENSION_INVALID".to_string());
  }
  if let Some(value) = &metadata.aspect_ratio {
    if !matches!(value.as_str(), "1:1" | "9:16" | "16:9") {
      return Err("ASPECT_RATIO_UNSUPPORTED".to_string());
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

  fn request() -> SceneRenderRequestV1 {
    let job_id = "11111111-1111-4111-8111-111111111111".to_string();
    let attempt_id = "22222222-2222-4222-8222-222222222222".to_string();
    let scene_id = "scene-01".to_string();
    SceneRenderRequestV1 { schema_version: FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION.to_string(), request_id: "33333333-3333-4333-8333-333333333333".to_string(), idempotency_key: compute_idempotency_key(&job_id, &scene_id, &attempt_id, HASH_A, HASH_B, SceneRenderProvider::GrokImageEdit), job_id, attempt_id, scene_id, character_id: "hero-01".to_string(), character_version: 1, provider: SceneRenderProvider::GrokImageEdit, anchor: LocalArtifactRef { path: "C:/artcraft/anchors/hero.png".to_string(), sha256: HASH_A.to_string() }, compiled_prompt: "cinematic portrait, hero-01".to_string(), compiled_prompt_sha256: HASH_B.to_string(), metadata: SceneRenderMetadata { width: Some(1024), height: Some(1024), aspect_ratio: Some("1:1".to_string()) }, created_at: "2026-09-13T00:00:00Z".to_string() }
  }

  #[test]
  fn request_is_valid_and_idempotency_is_deterministic() {
    let value = request();
    value.validate().unwrap();
    assert_eq!(value.idempotency_key, compute_idempotency_key(&value.job_id, &value.scene_id, &value.attempt_id, &value.anchor.sha256, &value.compiled_prompt_sha256, value.provider));
    assert_ne!(value.idempotency_key, compute_idempotency_key(&value.job_id, &value.scene_id, &value.attempt_id, &value.anchor.sha256, HASH_A, value.provider));
  }

  #[test]
  fn request_rejects_invalid_or_stale_idempotency_key() {
    let mut value = request();
    value.idempotency_key = "not-a-hash".to_string();
    assert_eq!(value.validate().unwrap_err(), "IDEMPOTENCY_KEY_MISMATCH");
  }

  #[test]
  fn successful_receipt_requires_matching_nonempty_artifact() {
    let request = request();
    let receipt = SceneRenderReceiptV1 { schema_version: FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION.to_string(), request_id: request.request_id.clone(), job_id: request.job_id.clone(), scene_id: request.scene_id.clone(), attempt_id: request.attempt_id.clone(), idempotency_key: request.idempotency_key.clone(), provider: request.provider, provider_job_id: Some("provider-job-1".to_string()), status: SceneRenderStatus::Succeeded, submission_state: SubmissionState::Submitted, artifact: Some(RenderedArtifactRef { path: "C:/artcraft/artifacts/scene-01.png".to_string(), sha256: HASH_B.to_string(), mime_type: "image/png".to_string(), bytes: 42 }), submitted_at: Some("2026-09-13T00:00:01Z".to_string()), completed_at: "2026-09-13T00:00:02Z".to_string(), error_code: None, error_message: None };
    receipt.validate_for(&request).unwrap();
  }

  #[test]
  fn unknown_receipt_cannot_claim_not_submitted() {
    let request = request();
    let receipt = SceneRenderReceiptV1 { schema_version: FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION.to_string(), request_id: request.request_id.clone(), job_id: request.job_id.clone(), scene_id: request.scene_id.clone(), attempt_id: request.attempt_id.clone(), idempotency_key: request.idempotency_key.clone(), provider: request.provider, provider_job_id: None, status: SceneRenderStatus::Unknown, submission_state: SubmissionState::NotSubmitted, artifact: None, submitted_at: None, completed_at: "2026-09-13T00:00:02Z".to_string(), error_code: Some("WORKER_CRASHED".to_string()), error_message: None };
    assert_eq!(receipt.validate_for(&request).unwrap_err(), "UNKNOWN_RECEIPT_SUBMISSION_STATE_INVALID");
  }

  #[test]
  fn golden_request_and_receipt_are_valid_contract_examples() {
    let request: SceneRenderRequestV1 = serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../contracts/floword-local-bridge/v1/golden-request.json"))).unwrap();
    let receipt: SceneRenderReceiptV1 = serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../contracts/floword-local-bridge/v1/golden-receipt.json"))).unwrap();
    request.validate().unwrap();
    receipt.validate_for(&request).unwrap();
  }

  #[test]
  fn embedded_schema_has_the_expected_fingerprint() {
    let schema = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../contracts/floword-local-bridge/v1/schema.json"));
    let digest = Sha256::digest(schema.as_bytes());
    let digest_hex = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    assert_eq!(digest_hex, FLOWORD_LOCAL_BRIDGE_SCHEMA_SHA256);
  }
}
