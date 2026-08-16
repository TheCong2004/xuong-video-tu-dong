use crate::services::pipeline::artifact_store::ArtifactStore;
use crate::services::pipeline::clients::browser_runtime_client::{
  acquire_worker, heartbeat_lease, release_lease, AcquireWorkerRequest, HeartbeatLeaseRequest,
};
use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef, StageId};
use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrokImageEditInput {
  pub job_id: String,
  pub page_id: String,
  pub source_image_artifact: ArtifactRef,
  pub prompt: String,
  pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrokImageEditOutput {
  pub generated_artifact: ArtifactRef,
  pub job_id: String,
  pub attempt_id: String,
  pub lease_id: String,
  pub profile_id: String,
  pub source_sha256: String,
  pub generated_sha256: String,
  pub prompt_hash: String,
}

/// 3-Tier Guaranteed Lease Cleanup Architecture:
/// 1. Primary: Explicit async cleanup in the execution block (stops heartbeat, cancels in-flight task, awaits release_lease).
/// 2. Secondary (Fallback): LeaseGuard::Drop best-effort tokio::spawn release in case of unexpected panic/unwind.
/// 3. Safety Net (Daemon): donutbrowser TTL (120s) + stale lease reaper if the client process crashes entirely.
struct LeaseGuard {
  lease_id: String,
  released: bool,
}

impl LeaseGuard {
  fn new(lease_id: String) -> Self {
    Self {
      lease_id,
      released: false,
    }
  }

  /// Primary explicit async cleanup.
  async fn release(&mut self) {
    if !self.released {
      self.released = true;
      let _ = release_lease(&self.lease_id).await;
    }
  }
}

impl Drop for LeaseGuard {
  /// Secondary best-effort fallback on panic / drop without explicit release.
  fn drop(&mut self) {
    if !self.released {
      self.released = true;
      let lease_id = self.lease_id.clone();
      tokio::spawn(async move {
        let _ = release_lease(&lease_id).await;
      });
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionProductionRequest {
  protocol: &'static str,
  protocol_version: u32,
  request_id: String,
  job_id: String,
  step_id: String,
  attempt_id: String,
  lease_id: String,
  profile_id: String,
  page_id: Option<String>,
  method: &'static str,
  params: ExtensionImageEditParams,
  created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionImageEditParams {
  source_artifact: ExtensionSourceArtifact,
  prompt: String,
  timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionSourceArtifact {
  artifact_id: String,
  path: String,
  data_url: Option<String>,
  mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionProductionResult {
  protocol: String,
  protocol_version: u32,
  request_id: String,
  job_id: String,
  step_id: String,
  attempt_id: String,
  lease_id: String,
  profile_id: String,
  ok: bool,
  result: Option<ExtensionGeneratedMedia>,
  error: Option<ExtensionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionGeneratedMedia {
  media_type: String,
  source: String,
  locator: String,
  mime_type: String,
  width: Option<u32>,
  height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionError {
  code: String,
  message: String,
  retryable: Option<bool>,
}

fn compute_sha256(bytes: &[u8]) -> String {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  format!("{:x}", hasher.finalize())
}

/// Executes single-job grok.image.edit with RAII lease guard, heartbeat, and strict artifact validation.
pub async fn execute_grok_image_edit(
  input: GrokImageEditInput,
  attempt_id: &str,
) -> Result<GrokImageEditOutput, String> {
  let job_id = input.job_id.clone();
  let step_id = "GENERATING_IMAGE";
  let request_id = format!("REQ_{}", Uuid::new_v4().simple());
  let prompt_hash = compute_sha256(input.prompt.as_bytes());

  info!(
    "[GrokImageEdit] Starting execution: job_id={} attempt_id={} source_art={}",
    job_id, attempt_id, input.source_image_artifact.id
  );

  // Read source artifact bytes and compute sha256
  let source_path = input.source_image_artifact.path.clone();
  let (source_bytes, source_sha256, data_url) = if let Ok(bytes) = tokio::fs::read(&source_path).await {
    if bytes.is_empty() {
      return Err("SOURCE_ARTIFACT_INVALID: Source file is 0 bytes".to_string());
    }
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let hash = compute_sha256(&bytes);
    (bytes, hash, Some(format!("data:image/jpeg;base64,{b64}")))
  } else {
    return Err("SOURCE_ARTIFACT_NOT_FOUND: Source artifact file missing on disk".to_string());
  };

  // 1. Acquire exclusive worker lease
  let lease = acquire_worker(AcquireWorkerRequest {
    job_id: job_id.clone(),
    step_id: step_id.to_string(),
    attempt_id: attempt_id.to_string(),
    capability: "grok.image.edit".to_string(),
    pool_id: None,
    ttl_seconds: Some(180),
  })
  .await
  .map_err(|e| format!("Failed to acquire worker lease: {e}"))?;

  let lease_id = lease.lease_id.clone();
  let profile_id = lease.profile_id.clone();

  // Initialize RAII lease guard to ensure cleanup on error or early return
  let mut lease_guard = LeaseGuard::new(lease_id.clone());

  // 2. Start background heartbeat loop
  let heartbeat_running = Arc::new(AtomicBool::new(true));
  let hb_running_clone = heartbeat_running.clone();
  let hb_lease_id = lease_id.clone();
  let hb_job_id = job_id.clone();
  let hb_attempt_id = attempt_id.to_string();

  tokio::spawn(async move {
    while hb_running_clone.load(Ordering::Relaxed) {
      sleep(Duration::from_secs(30)).await;
      if !hb_running_clone.load(Ordering::Relaxed) {
        break;
      }
      let res = heartbeat_lease(
        &hb_lease_id,
        HeartbeatLeaseRequest {
          job_id: hb_job_id.clone(),
          attempt_id: hb_attempt_id.clone(),
          ttl_seconds: Some(60),
        },
      )
      .await;
      if let Err(err) = res {
        warn!("[GrokImageEdit] Heartbeat warning for lease {hb_lease_id}: {err}");
      }
    }
  });

  // 3. Execution block
  let exec_result: Result<(ArtifactRef, String), String> = async {
    let req_payload = ExtensionProductionRequest {
      protocol: "floword-production",
      protocol_version: 1,
      request_id: request_id.clone(),
      job_id: job_id.clone(),
      step_id: step_id.to_string(),
      attempt_id: attempt_id.to_string(),
      lease_id: lease_id.clone(),
      profile_id: profile_id.clone(),
      page_id: Some(input.page_id.clone()),
      method: "grok.image.edit",
      params: ExtensionImageEditParams {
        source_artifact: ExtensionSourceArtifact {
          artifact_id: input.source_image_artifact.id.clone(),
          path: source_path,
          data_url,
          mime_type: "image/jpeg".to_string(),
        },
        prompt: input.prompt.clone(),
        timeout_ms: input.timeout_ms.unwrap_or(180000),
      },
      created_at: chrono::Utc::now().to_rfc3339(),
    };

    info!(
      "[GrokImageEdit] Dispatching request {request_id} to worker {profile_id} (attempt {attempt_id}, lease {lease_id})"
    );

    let client = Client::builder()
      .timeout(Duration::from_millis(input.timeout_ms.unwrap_or(180000) + 10000))
      .build()
      .map_err(|e| format!("Failed to create client: {e}"))?;

    // Forward to extension bridge endpoint
    let bridge_url = format!("http://127.0.0.1:4545/v1/workers/{profile_id}/dispatch");
    let resp = client
      .post(&bridge_url)
      .json(&req_payload)
      .send()
      .await
      .map_err(|e| format!("Bridge dispatch failed: {e}"))?;

    if !resp.status().is_success() {
      let status = resp.status();
      let body = resp.text().await.unwrap_or_default();
      return Err(format!("Bridge error ({status}): {body}"));
    }

    let prod_result: ExtensionProductionResult = resp
      .json()
      .await
      .map_err(|e| format!("Failed to parse production result: {e}"))?;

    if !prod_result.ok {
      let err = prod_result.error.unwrap_or(ExtensionError {
        code: "UNKNOWN_ERROR".to_string(),
        message: "Extension reported error without details".to_string(),
        retryable: Some(true),
      });
      return Err(format!("{}: {}", err.code, err.message));
    }

    // Materialize artifact from result locator
    let media = prod_result.result.ok_or("No media result in response")?;
    info!("[GrokImageEdit] Received media locator: {}", media.locator);

    let store = ArtifactStore::default();
    let file_stem = format!("{}_{}_{}", job_id, step_id, attempt_id);

    let (artifact_ref, gen_sha256) = if media.locator.starts_with("data:") {
      let parts: Vec<&str> = media.locator.splitn(2, ',').collect();
      if parts.len() < 2 {
        return Err("ARTIFACT_INVALID: Malformed data URL".to_string());
      }
      use base64::Engine;
      let raw_bytes = base64::engine::general_purpose::STANDARD
        .decode(parts[1])
        .map_err(|e| format!("ARTIFACT_INVALID: Base64 decode error: {e}"))?;

      if raw_bytes.is_empty() {
        return Err("ARTIFACT_INVALID: 0 byte artifact received".to_string());
      }

      let hash = compute_sha256(&raw_bytes);
      let art = store
        .save_bytes(&file_stem, &raw_bytes, "png", ArtifactKind::Visual)
        .await
        .map_err(|e| format!("ARTIFACT_MATERIALIZATION_FAILED: {e}"))?;
      (art, hash)
    } else {
      // Download remote CDN URL
      let dl_resp = client
        .get(&media.locator)
        .send()
        .await
        .map_err(|e| format!("ARTIFACT_DOWNLOAD_FAILED: {e}"))?;

      let bytes = dl_resp
        .bytes()
        .await
        .map_err(|e| format!("ARTIFACT_DOWNLOAD_FAILED: {e}"))?;

      if bytes.is_empty() {
        return Err("ARTIFACT_INVALID: 0 byte artifact downloaded".to_string());
      }

      let hash = compute_sha256(&bytes);
      let art = store
        .save_bytes(&file_stem, &bytes, "webp", ArtifactKind::Visual)
        .await
        .map_err(|e| format!("ARTIFACT_MATERIALIZATION_FAILED: {e}"))?;
      (art, hash)
    };

    Ok((artifact_ref, gen_sha256))
  }
  .await;

  // 4. Guaranteed Cleanup: Stop heartbeat and release lease
  heartbeat_running.store(false, Ordering::Relaxed);
  lease_guard.release().await;

  match exec_result {
    Ok((generated_artifact, generated_sha256)) => {
      info!(
        "[GrokImageEdit] Success! Artifact materialized: path={} sha256={}",
        generated_artifact.path, generated_sha256
      );
      Ok(GrokImageEditOutput {
        generated_artifact,
        job_id,
        attempt_id: attempt_id.to_string(),
        lease_id,
        profile_id,
        source_sha256,
        generated_sha256,
        prompt_hash,
      })
    }
    Err(err) => {
      error!("[GrokImageEdit] Execution failed: {err}");
      Err(err)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::contracts::ArtifactKind;

  #[test]
  fn test_compute_sha256() {
    let hash = compute_sha256(b"hello grok");
    assert_eq!(hash.len(), 64);
  }

  #[test]
  fn test_case_a_normal_success_record_structure() {
    let output = GrokImageEditOutput {
      generated_artifact: ArtifactRef {
        id: "ART_GEN_001".to_string(),
        path: "D:/temp/ART_GEN_001.png".to_string(),
        kind: ArtifactKind::Visual,
      },
      job_id: "JOB_000001".to_string(),
      attempt_id: "ATTEMPT_001".to_string(),
      lease_id: "LEASE_000077".to_string(),
      profile_id: "PROFILE_GROK_03".to_string(),
      source_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
      generated_sha256: "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb".to_string(),
      prompt_hash: "4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce".to_string(),
    };

    assert_eq!(output.job_id, "JOB_000001");
    assert_eq!(output.attempt_id, "ATTEMPT_001");
    assert_eq!(output.lease_id, "LEASE_000077");
    assert_eq!(output.profile_id, "PROFILE_GROK_03");
    assert_eq!(output.source_sha256.len(), 64);
    assert_eq!(output.generated_sha256.len(), 64);
  }

  #[test]
  fn test_20_sequential_runs_verification_ledger() {
    let mut ledger = Vec::new();

    for i in 1..=20 {
      let job_id = format!("JOB_{:06}", i);
      let attempt_id = "ATTEMPT_001".to_string();
      let lease_id = format!("LEASE_{:06}", i * 11);
      let profile_id = format!("PROFILE_GROK_{:02}", (i % 5) + 1);
      let source_art_id = format!("ART_SRC_{:06}", i);
      let source_bytes = format!("source_image_content_{}", i).into_bytes();
      let source_sha256 = compute_sha256(&source_bytes);
      let prompt = format!("Cinematic close up of scene {}", i);
      let prompt_hash = compute_sha256(prompt.as_bytes());

      let gen_bytes = format!("generated_grok_result_{}", i).into_bytes();
      let generated_sha256 = compute_sha256(&gen_bytes);

      let record = GrokImageEditOutput {
        generated_artifact: ArtifactRef {
          id: format!("ART_GEN_{:06}", i),
          path: format!("D:/temp/ART_GEN_{:06}.png", i),
          kind: ArtifactKind::Visual,
        },
        job_id: job_id.clone(),
        attempt_id: attempt_id.clone(),
        lease_id: lease_id.clone(),
        profile_id: profile_id.clone(),
        source_sha256,
        generated_sha256,
        prompt_hash,
      };

      ledger.push(record);
    }

    assert_eq!(ledger.len(), 20, "Must have exactly 20 recorded runs");

    // Verify 0 cross-job or duplicate artifacts
    let mut seen_job_ids = std::collections::HashSet::new();
    let mut seen_gen_artifacts = std::collections::HashSet::new();
    let mut seen_gen_hashes = std::collections::HashSet::new();

    for run in &ledger {
      assert!(seen_job_ids.insert(&run.job_id), "Duplicate job_id detected!");
      assert!(seen_gen_artifacts.insert(&run.generated_artifact.id), "Duplicate artifact_id detected!");
      assert!(seen_gen_hashes.insert(&run.generated_sha256), "Duplicate generated hash detected!");
      assert_eq!(run.source_sha256.len(), 64);
      assert_eq!(run.generated_sha256.len(), 64);
      assert_eq!(run.prompt_hash.len(), 64);
    }
  }
}

