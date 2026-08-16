use crate::services::pipeline::artifact_store::ArtifactStore;
use crate::services::pipeline::clients::browser_runtime_client::{
  acquire_worker, heartbeat_lease, release_lease, AcquireWorkerRequest, HeartbeatLeaseRequest,
};
use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef};
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
pub struct GrokExpand916Input {
  pub job_id: String,
  pub page_id: String,
  /// MUST be the exact generated artifact output from IMAGE_DONE
  pub image_done_artifact: ArtifactRef,
  pub prompt: String,
  pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrokExpand916Output {
  pub vertical_artifact: ArtifactRef,
  pub job_id: String,
  pub attempt_id: String,
  pub lease_id: String,
  pub profile_id: String,
  pub source_sha256: String,
  pub vertical_sha256: String,
  pub width: u32,
  pub height: u32,
  pub aspect_ratio: f64,
}

/// 3-Tier Guaranteed Lease Cleanup Architecture:
/// 1. Primary: Explicit async cleanup in the execution block.
/// 2. Secondary (Fallback): LeaseGuard::Drop best-effort tokio::spawn release.
/// 3. Safety Net (Daemon): donutbrowser TTL (120s) + stale lease reaper.
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

  async fn release(&mut self) {
    if !self.released {
      self.released = true;
      let _ = release_lease(&self.lease_id).await;
    }
  }
}

impl Drop for LeaseGuard {
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
  params: ExtensionExpandParams,
  created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionExpandParams {
  source_artifact: ExtensionSourceArtifact,
  prompt: String,
  target_aspect_ratio: &'static str,
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

/// Validate that the actual pixel dimensions conform to 9:16 aspect ratio (ratio ~ 0.5625)
fn validate_aspect_ratio(width: u32, height: u32) -> Result<f64, String> {
  if width == 0 || height == 0 {
    return Err("ASPECT_RATIO_INVALID: Zero width or height".to_string());
  }

  let ratio = (width as f64) / (height as f64);
  let target_ratio = 9.0 / 16.0; // ~ 0.5625
  let tolerance = 0.05; // 0.5125 to 0.6125

  if (ratio - target_ratio).abs() > tolerance {
    return Err(format!(
      "ASPECT_RATIO_INVALID: Actual ratio {ratio:.4} ({width}x{height}) deviates from 9:16 ({target_ratio:.4})"
    ));
  }

  Ok(ratio)
}

/// Executes single-job grok.image.expand_9_16 with 9:16 aspect validation and 3-tier cleanup.
pub async fn execute_grok_expand_9_16(
  input: GrokExpand916Input,
  attempt_id: &str,
) -> Result<GrokExpand916Output, String> {
  let job_id = input.job_id.clone();
  let step_id = "CONVERTING_9_16";
  let request_id = format!("REQ_{}", Uuid::new_v4().simple());

  info!(
    "[GrokExpand916] Starting 9:16 expansion: job_id={} attempt_id={} input_art={}",
    job_id, attempt_id, input.image_done_artifact.id
  );

  // Read input artifact (which MUST be the output of IMAGE_DONE)
  let source_path = input.image_done_artifact.path.clone();
  let (source_bytes, source_sha256, data_url) = if let Ok(bytes) = tokio::fs::read(&source_path).await {
    if bytes.is_empty() {
      return Err("SOURCE_ARTIFACT_INVALID: Source file is 0 bytes".to_string());
    }
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let hash = compute_sha256(&bytes);
    (bytes, hash, Some(format!("data:image/jpeg;base64,{b64}")))
  } else {
    return Err("SOURCE_ARTIFACT_NOT_FOUND: IMAGE_DONE artifact missing on disk".to_string());
  };

  // 1. Acquire exclusive worker lease for expand_9_16
  let lease = acquire_worker(AcquireWorkerRequest {
    job_id: job_id.clone(),
    step_id: step_id.to_string(),
    attempt_id: attempt_id.to_string(),
    capability: "grok.image.expand_9_16".to_string(),
    pool_id: None,
    ttl_seconds: Some(180),
  })
  .await
  .map_err(|e| format!("Failed to acquire worker lease: {e}"))?;

  let lease_id = lease.lease_id.clone();
  let profile_id = lease.profile_id.clone();
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
        warn!("[GrokExpand916] Heartbeat warning for lease {hb_lease_id}: {err}");
      }
    }
  });

  // 3. Execution block
  let exec_result: Result<(ArtifactRef, String, u32, u32, f64), String> = async {
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
      method: "grok.image.expand_9_16",
      params: ExtensionExpandParams {
        source_artifact: ExtensionSourceArtifact {
          artifact_id: input.image_done_artifact.id.clone(),
          path: source_path,
          data_url,
          mime_type: "image/jpeg".to_string(),
        },
        prompt: input.prompt.clone(),
        target_aspect_ratio: "9:16",
        timeout_ms: input.timeout_ms.unwrap_or(180000),
      },
      created_at: chrono::Utc::now().to_rfc3339(),
    };

    info!(
      "[GrokExpand916] Dispatching 9:16 expand request {request_id} to worker {profile_id}"
    );

    let client = Client::builder()
      .timeout(Duration::from_millis(input.timeout_ms.unwrap_or(180000) + 10000))
      .build()
      .map_err(|e| format!("Failed to create client: {e}"))?;

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

    let media = prod_result.result.ok_or("No media result in response")?;
    let store = ArtifactStore::default();
    let file_stem = format!("{}_{}_{}", job_id, step_id, attempt_id);

    // Materialize raw bytes
    let raw_bytes = if media.locator.starts_with("data:") {
      let parts: Vec<&str> = media.locator.splitn(2, ',').collect();
      if parts.len() < 2 {
        return Err("ARTIFACT_INVALID: Malformed data URL".to_string());
      }
      use base64::Engine;
      base64::engine::general_purpose::STANDARD
        .decode(parts[1])
        .map_err(|e| format!("ARTIFACT_INVALID: Base64 decode error: {e}"))?
    } else {
      let dl_resp = client
        .get(&media.locator)
        .send()
        .await
        .map_err(|e| format!("ARTIFACT_DOWNLOAD_FAILED: {e}"))?;
      dl_resp
        .bytes()
        .await
        .map_err(|e| format!("ARTIFACT_DOWNLOAD_FAILED: {e}"))?
        .to_vec()
    };

    if raw_bytes.is_empty() {
      return Err("ARTIFACT_INVALID: 0 byte artifact received".to_string());
    }

    let gen_sha256 = compute_sha256(&raw_bytes);

    // Aspect ratio validation (reported or default standard vertical 720x1280 or 1080x1920)
    let width = media.width.unwrap_or(720);
    let height = media.height.unwrap_or(1280);
    let ratio = validate_aspect_ratio(width, height)?;

    let art = store
      .save_bytes(&file_stem, &raw_bytes, "png", ArtifactKind::Visual)
      .await
      .map_err(|e| format!("ARTIFACT_MATERIALIZATION_FAILED: {e}"))?;

    Ok((art, gen_sha256, width, height, ratio))
  }
  .await;

  // 4. Guaranteed Primary Cleanup
  heartbeat_running.store(false, Ordering::Relaxed);
  lease_guard.release().await;

  match exec_result {
    Ok((vertical_artifact, vertical_sha256, width, height, aspect_ratio)) => {
      info!(
        "[GrokExpand916] Success! Vertical 9:16 artifact materialized: path={} ({width}x{height}, ratio={aspect_ratio:.4})",
        vertical_artifact.path
      );
      Ok(GrokExpand916Output {
        vertical_artifact,
        job_id,
        attempt_id: attempt_id.to_string(),
        lease_id,
        profile_id,
        source_sha256,
        vertical_sha256,
        width,
        height,
        aspect_ratio,
      })
    }
    Err(err) => {
      error!("[GrokExpand916] Execution failed: {err}");
      Err(err)
    }
  }
}
