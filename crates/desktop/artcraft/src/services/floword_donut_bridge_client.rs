//! ArtCraft client for the DonutBrowser local bridge helper.
//!
//! The two desktop applications remain independently installable.  This client
//! discovers DonutBrowser's per-user manifest and invokes its bundled helper
//! with request/receipt files; it deliberately never calls a Donut HTTP port.

use super::floword_local_bridge::{SceneRenderReceiptV1, SceneRenderRequestV1, FLOWORD_LOCAL_BRIDGE_SCHEMA_SHA256, FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::{
  fs,
  path::{Path, PathBuf},
  time::Duration,
};
use tokio::{process::Command, time::timeout};
use uuid::Uuid;

const MANIFEST_FILE: &str = "donut-browser-install-v1.json";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DonutBridgeMode {
  DryRun,
  Submit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DonutBridgeEnvelopeV1 {
  pub protocol_version: String,
  pub contract_schema_sha256: String,
  pub browser_profile_id: String,
  pub mode: DonutBridgeMode,
  pub request: SceneRenderRequestV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DonutBridgeInstallManifestV1 {
  pub protocol_version: String,
  pub contract_schema_sha256: String,
  pub bridge_executable: String,
  pub registered_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DonutBridgeHealthV1 {
  pub protocol_version: String,
  pub contract_schema_sha256: String,
  pub worker_version: String,
  pub state_root: String,
  pub provider_submission: bool,
}

#[derive(Debug, Clone)]
pub struct DonutBridgeClient {
  executable: PathBuf,
  state_root: PathBuf,
  timeout: Duration,
}

impl DonutBridgeClient {
  pub fn discover() -> Result<Self, String> {
    let root = default_bridge_root()?;
    let manifest_path = root.join(MANIFEST_FILE);
    let manifest: DonutBridgeInstallManifestV1 = read_json(&manifest_path)?;
    validate_manifest(&manifest)?;
    let executable = PathBuf::from(&manifest.bridge_executable);
    if !executable.is_file() {
      return Err("DONUT_BRIDGE_EXECUTABLE_MISSING".to_string());
    }
    Ok(Self { executable, state_root: root, timeout: DEFAULT_TIMEOUT })
  }

  pub fn for_test(executable: PathBuf, state_root: PathBuf) -> Self {
    Self { executable, state_root, timeout: DEFAULT_TIMEOUT }
  }

  pub async fn health(&self) -> Result<DonutBridgeHealthV1, String> {
    let output = self.temp_file("health")?;
    let result = self.run(&["health", "--output", path_arg(&output)]).await;
    let health = result.and_then(|_| read_json(&output));
    let _ = fs::remove_file(output);
    let health: DonutBridgeHealthV1 = health?;
    if health.protocol_version != FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION || health.contract_schema_sha256 != FLOWORD_LOCAL_BRIDGE_SCHEMA_SHA256 {
      return Err("DONUT_BRIDGE_PROTOCOL_INCOMPATIBLE".to_string());
    }
    Ok(health)
  }

  pub async fn execute(&self, envelope: &DonutBridgeEnvelopeV1) -> Result<SceneRenderReceiptV1, String> {
    validate_envelope(envelope)?;
    let input = self.temp_file("request")?;
    let output = self.temp_file("receipt")?;
    write_json(&input, envelope)?;
    let result = self.run(&["execute", "--input", path_arg(&input), "--output", path_arg(&output)]).await;
    let receipt = result.and_then(|_| read_json(&output));
    let _ = fs::remove_file(input);
    let _ = fs::remove_file(output);
    let receipt: SceneRenderReceiptV1 = receipt?;
    receipt.validate_for(&envelope.request)?;
    Ok(receipt)
  }

  pub async fn get_receipt(&self, idempotency_key: &str) -> Result<SceneRenderReceiptV1, String> {
    if idempotency_key.len() != 64 || !idempotency_key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
      return Err("idempotencyKey_INVALID".to_string());
    }
    let output = self.temp_file("receipt")?;
    let result = self.run(&["get-receipt", "--idempotency-key", idempotency_key.to_string(), "--output", path_arg(&output)]).await;
    let receipt = result.and_then(|_| read_json(&output));
    let _ = fs::remove_file(output);
    receipt
  }

  async fn run(&self, args: &[String]) -> Result<(), String> {
    let mut command = Command::new(&self.executable);
    command.arg("--state-root").arg(&self.state_root);
    command.args(args);
    let completed = timeout(self.timeout, command.output()).await.map_err(|_| "DONUT_BRIDGE_TIMEOUT".to_string())?.map_err(|_| "DONUT_BRIDGE_START_FAILED".to_string())?;
    if completed.status.success() {
      return Ok(());
    }
    let message = String::from_utf8_lossy(&completed.stderr);
    Err(format!("DONUT_BRIDGE_FAILED:{}", sanitize_message(&message)))
  }

  fn temp_file(&self, kind: &str) -> Result<PathBuf, String> {
    let directory = self.state_root.join("artcraft-client");
    fs::create_dir_all(&directory).map_err(|_| "DONUT_BRIDGE_CLIENT_STORAGE_FAILED".to_string())?;
    Ok(directory.join(format!("{kind}-{}.json", Uuid::new_v4())))
  }
}

fn default_bridge_root() -> Result<PathBuf, String> {
  directories::BaseDirs::new().map(|value| value.data_local_dir().join("Floword").join("DonutBridge")).ok_or_else(|| "DONUT_BRIDGE_LOCALAPPDATA_UNAVAILABLE".to_string())
}

fn validate_manifest(value: &DonutBridgeInstallManifestV1) -> Result<(), String> {
  if value.protocol_version != FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION || value.contract_schema_sha256 != FLOWORD_LOCAL_BRIDGE_SCHEMA_SHA256 {
    return Err("DONUT_BRIDGE_PROTOCOL_INCOMPATIBLE".to_string());
  }
  if value.bridge_executable.trim().is_empty() || value.bridge_executable.contains('\0') || value.registered_at.trim().is_empty() {
    return Err("DONUT_BRIDGE_MANIFEST_INVALID".to_string());
  }
  Ok(())
}

fn validate_envelope(value: &DonutBridgeEnvelopeV1) -> Result<(), String> {
  if value.protocol_version != FLOWORD_LOCAL_BRIDGE_SCHEMA_VERSION || value.contract_schema_sha256 != FLOWORD_LOCAL_BRIDGE_SCHEMA_SHA256 || value.browser_profile_id.trim().is_empty() || value.browser_profile_id.contains('\0') {
    return Err("DONUT_BRIDGE_PROTOCOL_INCOMPATIBLE".to_string());
  }
  value.request.validate()
}

fn path_arg(path: &Path) -> String {
  path.to_string_lossy().into_owned()
}
fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T, String> {
  let body = fs::read_to_string(path).map_err(|_| "DONUT_BRIDGE_RECEIPT_MISSING".to_string())?;
  serde_json::from_str(&body).map_err(|_| "DONUT_BRIDGE_JSON_INVALID".to_string())
}
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
  let serialized = serde_json::to_vec(value).map_err(|_| "DONUT_BRIDGE_JSON_SERIALIZE_FAILED".to_string())?;
  fs::write(path, serialized).map_err(|_| "DONUT_BRIDGE_CLIENT_STORAGE_FAILED".to_string())
}
fn sanitize_message(value: &str) -> String {
  value.chars().filter(|character| !character.is_control() || *character == ' ').take(240).collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::floword_local_bridge::{compute_idempotency_key, LocalArtifactRef, SceneRenderMetadata, SceneRenderProvider};

  const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

  fn envelope() -> DonutBridgeEnvelopeV1 {
    let job_id = "11111111-1111-4111-8111-111111111111".to_string();
    let attempt_id = "22222222-2222-4222-8222-222222222222".to_string();
    let scene_id = "scene-01".to_string();
    DonutBridgeEnvelopeV1 { protocol_version: "1.0".to_string(), contract_schema_sha256: FLOWORD_LOCAL_BRIDGE_SCHEMA_SHA256.to_string(), browser_profile_id: "profile-01".to_string(), mode: DonutBridgeMode::DryRun, request: SceneRenderRequestV1 { schema_version: "1.0".to_string(), request_id: "33333333-3333-4333-8333-333333333333".to_string(), job_id: job_id.clone(), attempt_id: attempt_id.clone(), scene_id: scene_id.clone(), character_id: "hero-01".to_string(), character_version: 1, provider: SceneRenderProvider::GrokImageEdit, anchor: LocalArtifactRef { path: "C:/anchor.png".to_string(), sha256: HASH_A.to_string() }, compiled_prompt: "portrait".to_string(), compiled_prompt_sha256: HASH_B.to_string(), idempotency_key: compute_idempotency_key(&job_id, &scene_id, &attempt_id, HASH_A, HASH_B, SceneRenderProvider::GrokImageEdit), metadata: SceneRenderMetadata::default(), created_at: "2026-09-13T00:00:00Z".to_string() } }
  }
  #[test]
  fn envelope_requires_matching_schema_and_valid_request() {
    validate_envelope(&envelope()).unwrap();
    let mut invalid = envelope();
    invalid.browser_profile_id.clear();
    assert_eq!(validate_envelope(&invalid).unwrap_err(), "DONUT_BRIDGE_PROTOCOL_INCOMPATIBLE");
  }
}
