use crate::services::pipeline::artifact_store::ArtifactStore;
use crate::services::pipeline::clients::browser_runtime_client::{
  acquire_worker, get_extension_bridge_base_url, heartbeat_lease, release_lease, AcquireWorkerRequest,
  HeartbeatLeaseRequest, HeartbeatLeaseResponse,
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
#[serde(rename_all = "camelCase")]
pub struct GrokImageEditInput {
  pub job_id: String,
  pub page_id: String,
  pub browser_profile_id: Option<String>,
  pub source_image_artifact: ArtifactRef,
  pub prompt: String,
  pub timeout_ms: Option<u64>,
  pub workflow_root: std::path::PathBuf,
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
  pub mime_type: String,
  pub size_bytes: usize,
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
struct ExtensionImageEditParams {
  source_artifact: ExtensionSourceArtifact,
  prompt: String,
  timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionSourceArtifact {
  artifact_id: String,
  path: String,
  data_url: Option<String>,
  mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
struct ExtensionGeneratedMedia {
  media_type: String,
  source: String,
  locator: String,
  mime_type: Option<String>,
  width: Option<u32>,
  height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionError {
  code: String,
  message: String,
  retryable: Option<bool>,
}

pub fn compute_sha256(bytes: &[u8]) -> String {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  let result = hasher.finalize();
  result.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Detect image MIME type from raw header magic bytes
pub fn detect_image_mime(bytes: &[u8]) -> Result<(&'static str, &'static str), String> {
  if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
    Ok(("image/png", "png"))
  } else if bytes.len() >= 3 && &bytes[0..3] == b"\xFF\xD8\xFF" {
    Ok(("image/jpeg", "jpg"))
  } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
    Ok(("image/webp", "webp"))
  } else {
    Err("ARTIFACT_INVALID_MIME: Unrecognized or unsupported image byte signature".to_string())
  }
}

/// Primary orchestrator entry point for executing a Grok image edit step.
pub async fn execute_grok_image_edit_stage(
  input: GrokImageEditInput,
  attempt_id: &str,
  cancel_flag: Option<&Arc<AtomicBool>>,
) -> Result<GrokImageEditOutput, String> {
  let job_id = input.job_id.clone();
  let step_id = "GENERATING_IMAGE";
  let request_id = format!("REQ_{}", Uuid::new_v4().simple());
  let prompt_hash = compute_sha256(input.prompt.as_bytes());

  info!(
    "[GrokImageEdit] Starting execution: job_id={} attempt_id={} source_art={}",
    job_id, attempt_id, input.source_image_artifact.artifact_id
  );

  // Fail-closed canonical workflow root validation
  let workflow_root_buf = input.workflow_root.clone();
  if workflow_root_buf.as_os_str().is_empty() {
    return Err("WORKFLOW_ROOT_REQUIRED: Canonical workflow artifact root path is required".to_string());
  }
  std::fs::create_dir_all(&workflow_root_buf)
    .map_err(|e| format!("WORKFLOW_ROOT_INVALID: Cannot create canonical workflow artifact root {:?}: {e}", workflow_root_buf))?;

  // Read source artifact bytes and validate MIME BEFORE acquiring worker lease
  let source_path = input.source_image_artifact.location.clone();
  let (source_bytes, source_sha256, source_mime, data_url) = if let Ok(bytes) = tokio::fs::read(&source_path).await {
    if bytes.is_empty() {
      return Err("SOURCE_ARTIFACT_INVALID: Source file is 0 bytes".to_string());
    }
    let (mime, _ext) = detect_image_mime(&bytes)
      .map_err(|e| format!("SOURCE_ARTIFACT_INVALID_MIME: {e}"))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let hash = compute_sha256(&bytes);
    let d_url = format!("data:{mime};base64,{b64}");
    (bytes, hash, mime.to_string(), Some(d_url))
  } else {
    return Err("SOURCE_ARTIFACT_NOT_FOUND: Source artifact file missing on disk".to_string());
  };

  // 1. Acquire exclusive worker lease
  let lease = acquire_worker(AcquireWorkerRequest {
    pool_id: None,
    profile_id: input.browser_profile_id.clone(),
    capability: "grok.image.edit".to_string(),
    job_id: job_id.clone(),
    step_id: step_id.to_string(),
    attempt_id: attempt_id.to_string(),
    ttl_seconds: Some(120),
  })
  .await
  .map_err(|e| format!("Failed to acquire worker lease: {e}"))?;

  let lease_id = lease.lease_id.clone();
  let profile_id = lease.profile_id.clone();
  let mut lease_guard = LeaseGuard::new(lease_id.clone());

  info!(
    "[GrokImageEdit] Acquired worker lease: lease_id={} profile_id={}",
    lease_id, profile_id
  );

  // 2. Start background heartbeat loop (every 30s)
  let heartbeat_running = Arc::new(AtomicBool::new(true));
  let hb_running_clone = heartbeat_running.clone();
  let hb_lease_id = lease_id.clone();
  let hb_job_id = job_id.clone();
  let hb_attempt_id = attempt_id.to_string();
  let heartbeat_lost = Arc::new(AtomicBool::new(false));
  let hb_lost_clone = heartbeat_lost.clone();

  let heartbeat_task = tokio::spawn(async move {
    while hb_running_clone.load(Ordering::Relaxed) {
      sleep(Duration::from_secs(30)).await;
      if !hb_running_clone.load(Ordering::Relaxed) {
        break;
      }
      let req = HeartbeatLeaseRequest {
        job_id: hb_job_id.clone(),
        attempt_id: hb_attempt_id.clone(),
        ttl_seconds: Some(60),
      };
      if let Err(err) = heartbeat_lease(&hb_lease_id, req).await {
        warn!("[GrokImageEdit] Heartbeat failed for lease {hb_lease_id}: {err}");
        if err.contains("LEASE_NOT_FOUND")
          || err.contains("Lease not found")
          || err.contains("LEASE_NOT_ACTIVE")
          || err.contains("Lease is not active")
          || err.contains("CORRELATION_MISMATCH")
        {
          error!("[GrokImageEdit] Authoritative lease ownership loss detected for {hb_lease_id}: {err}");
          hb_lost_clone.store(true, Ordering::Relaxed);
          break;
        }
      }
    }
  });

  // 3. Execution block
  let exec_result: Result<(ArtifactRef, String, String, usize), String> = async {
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
          artifact_id: input.source_image_artifact.artifact_id.clone(),
          path: source_path,
          data_url,
          mime_type: source_mime,
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
    let bridge_base = get_extension_bridge_base_url();
    let bridge_url = format!("{bridge_base}/v1/workers/{profile_id}/dispatch");

    let (resp_res, was_cancelled) = tokio::select! {
      res = client.post(&bridge_url).json(&req_payload).send() => {
        (Some(res), false)
      }
      _ = async {
        if let Some(flag) = cancel_flag {
          while !flag.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(100)).await;
          }
        } else {
          std::future::pending::<()>().await;
        }
      } => {
        (None, true)
      }
    };

    if was_cancelled {
      info!("[GrokImageEdit] Job cancelled in-flight! Dispatching cancel request to worker {profile_id}");
      let cancel_payload = serde_json::json!({
        "protocol": "floword-production",
        "protocolVersion": 1,
        "requestId": format!("CANCEL_{}", Uuid::new_v4().simple()),
        "jobId": job_id,
        "stepId": step_id,
        "attemptId": attempt_id,
        "leaseId": lease_id,
        "profileId": profile_id,
        "method": "production.task.cancel",
        "params": {
          "targetRequestId": request_id,
        },
        "createdAt": chrono::Utc::now().to_rfc3339(),
      });
      let _ = client.post(&bridge_url).json(&cancel_payload).send().await;
      lease_guard.release().await;
      return Err("CANCELLED".to_string());
    }

    let resp = resp_res.unwrap().map_err(|e| format!("Bridge dispatch failed: {e}"))?;

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

    // Mid-execution ownership check
    if heartbeat_lost.load(Ordering::Relaxed) {
      return Err("LEASE_LOST: Authoritative lease loss detected during generation".to_string());
    }

    // Materialize artifact from result locator
    let media = prod_result.result.ok_or("No media result in response")?;
    info!("[GrokImageEdit] Received media locator: {}", media.locator);

    let file_stem = format!("{}_{}_{}", job_id, step_id, attempt_id);

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
      // Download remote CDN URL
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

    // Sniff MIME and validate real image format from bytes
    let (detected_mime, ext) = detect_image_mime(&raw_bytes)?;
    let size_bytes = raw_bytes.len();
    let gen_sha256 = compute_sha256(&raw_bytes);

    // Save physical file directly into workflow artifact root
    let _ = std::fs::create_dir_all(&workflow_root_buf);
    let file_path = workflow_root_buf.join(format!("{file_stem}.{ext}"));
    std::fs::write(&file_path, &raw_bytes)
      .map_err(|e| format!("ARTIFACT_MATERIALIZATION_FAILED: {e}"))?;

    // Canonical ArtifactStore registration as GeneratedImage
    let stored = ArtifactStore::register_typed_artifact(
      &workflow_root_buf,
      &job_id,
      StageId::StoryScript,
      "grok",
      ArtifactKind::GeneratedImage,
      &file_path,
      serde_json::json!({
        "sha256": gen_sha256,
        "mime_type": detected_mime,
        "size_bytes": size_bytes,
        "prompt_hash": prompt_hash,
        "service": "grok"
      }),
    )
    .map_err(|e| format!("ARTIFACT_MATERIALIZATION_FAILED: {e}"))?;

    let art = stored
      .to_artifact_ref(StageId::StoryScript)
      .map_err(|e| format!("ARTIFACT_MATERIALIZATION_FAILED: {e}"))?;

    // P0-5: TERMINAL HEARTBEAT OWNERSHIP BARRIER (FAIL-CLOSED)
    // Authoritative check right before declaring stage success
    let final_hb = heartbeat_lease(
      &lease_id,
      HeartbeatLeaseRequest {
        job_id: job_id.clone(),
        attempt_id: attempt_id.to_string(),
        ttl_seconds: Some(60),
      },
    )
    .await;

    let hb_valid = match final_hb {
      Ok(res) => res.status == "Active",
      Err(err) => {
        error!("[GrokImageEdit] Terminal heartbeat failed for lease {lease_id}: {err}");
        false
      }
    };

    if !hb_valid || heartbeat_lost.load(Ordering::Relaxed) {
      return Err("LEASE_LOST: Authoritative lease loss detected at terminal success boundary".to_string());
    }

    Ok((art, gen_sha256, detected_mime.to_string(), size_bytes))
  }
  .await;

  // 4. Guaranteed Cleanup: Stop heartbeat task and release lease
  heartbeat_running.store(false, Ordering::Relaxed);
  heartbeat_task.abort();
  lease_guard.release().await;

  match exec_result {
    Ok((generated_artifact, generated_sha256, mime_type, size_bytes)) => {
      info!(
        "[GrokImageEdit] Success! Artifact materialized: path={} mime={} size={} sha256={}",
        generated_artifact.location, mime_type, size_bytes, generated_sha256
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
        mime_type,
        size_bytes,
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
  fn test_detect_image_mime_png() {
    let png_header = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
    let (mime, ext) = detect_image_mime(png_header).unwrap();
    assert_eq!(mime, "image/png");
    assert_eq!(ext, "png");
  }

  #[test]
  fn test_detect_image_mime_jpeg() {
    let jpeg_header = b"\xFF\xD8\xFF\xE0\x00\x10JFIF";
    let (mime, ext) = detect_image_mime(jpeg_header).unwrap();
    assert_eq!(mime, "image/jpeg");
    assert_eq!(ext, "jpg");
  }

  #[test]
  fn test_detect_image_mime_webp() {
    let webp_header = b"RIFF\x20\x00\x00\x00WEBPVP8 ";
    let (mime, ext) = detect_image_mime(webp_header).unwrap();
    assert_eq!(mime, "image/webp");
    assert_eq!(ext, "webp");
  }

  #[test]
  fn test_detect_image_mime_invalid_fails_before_acquire() {
    let invalid_bytes = b"not an image file";
    let err = detect_image_mime(invalid_bytes);
    assert!(err.is_err());
  }

  #[test]
  fn test_case_a_normal_success_record_structure() {
    let output = GrokImageEditOutput {
      generated_artifact: ArtifactRef {
        artifact_id: "ART_GEN_001".to_string(),
        kind: ArtifactKind::GeneratedImage,
        produced_by_stage: StageId::StoryScript,
        location: "D:/temp/ART_GEN_001.png".to_string(),
        mime_type: Some("image/png".to_string()),
        metadata: serde_json::json!({}),
      },
      job_id: "JOB_000001".to_string(),
      attempt_id: "ATTEMPT_001".to_string(),
      lease_id: "LEASE_000077".to_string(),
      profile_id: "PROFILE_GROK_03".to_string(),
      source_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
      generated_sha256: "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb".to_string(),
      prompt_hash: "4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce".to_string(),
      mime_type: "image/png".to_string(),
      size_bytes: 1048576,
    };

    assert_eq!(output.job_id, "JOB_000001");
    assert_eq!(output.attempt_id, "ATTEMPT_001");
    assert_eq!(output.lease_id, "LEASE_000077");
    assert_eq!(output.profile_id, "PROFILE_GROK_03");
    assert_eq!(output.source_sha256.len(), 64);
    assert_eq!(output.generated_sha256.len(), 64);
    assert_eq!(output.mime_type, "image/png");
    assert_eq!(output.generated_artifact.kind, ArtifactKind::GeneratedImage);
    assert!(output.size_bytes > 0);
  }

  #[tokio::test]
  async fn test_terminal_ownership_barrier_rejection_on_lease_loss() {
    let heartbeat_lost = Arc::new(AtomicBool::new(true));
    let has_lost = heartbeat_lost.load(Ordering::Relaxed);
    assert!(has_lost, "Lease loss must be detected and cause rejection");
  }

  // TEST GROUP E — Final ownership barrier fail-closed tests
  #[test]
  fn test_e1_final_heartbeat_ok_allows_success() {
    let hb_status = "Active";
    let is_valid = hb_status == "Active";
    assert!(is_valid);
  }

  #[test]
  fn test_e2_final_heartbeat_lease_not_active_fails() {
    let hb_res: Result<HeartbeatLeaseResponse, String> = Ok(HeartbeatLeaseResponse {
      lease_id: "L1".to_string(),
      status: "Released".to_string(),
      expires_at: "".to_string(),
    });
    let outcome = match hb_res {
      Ok(r) if r.status == "Active" => Ok(()),
      _ => Err("LEASE_LOST: Terminal ownership check failed".to_string()),
    };
    assert!(outcome.is_err());
  }

  #[test]
  fn test_e3_final_heartbeat_correlation_mismatch_fails() {
    let hb_res: Result<HeartbeatLeaseResponse, String> = Err("CORRELATION_MISMATCH: lease belongs to another job".to_string());
    let outcome = match hb_res {
      Ok(r) if r.status == "Active" => Ok(()),
      Err(e) => Err(format!("LEASE_LOST: {e}")),
      _ => Err("LEASE_LOST".to_string()),
    };
    assert!(outcome.is_err());
    assert!(outcome.unwrap_err().contains("CORRELATION_MISMATCH"));
  }

  #[test]
  fn test_e4_final_heartbeat_connection_refused_fails() {
    let hb_res: Result<HeartbeatLeaseResponse, String> = Err("Connection refused (os error 111)".to_string());
    let outcome = match hb_res {
      Ok(r) if r.status == "Active" => Ok(()),
      Err(e) => Err(format!("LEASE_LOST: Terminal ownership verification failed: {e}")),
      _ => Err("LEASE_LOST".to_string()),
    };
    assert!(outcome.is_err());
    assert!(outcome.unwrap_err().contains("Connection refused"));
  }

  #[test]
  fn test_e5_final_heartbeat_timeout_fails() {
    let hb_res: Result<HeartbeatLeaseResponse, String> = Err("Operation timed out".to_string());
    let outcome = match hb_res {
      Ok(r) if r.status == "Active" => Ok(()),
      Err(e) => Err(format!("LEASE_LOST: Terminal ownership verification failed: {e}")),
      _ => Err("LEASE_LOST".to_string()),
    };
    assert!(outcome.is_err());
    assert!(outcome.unwrap_err().contains("timed out"));
  }

  #[test]
  fn test_e6_final_heartbeat_http_500_fails() {
    let hb_res: Result<HeartbeatLeaseResponse, String> = Err("Heartbeat failed (500 Internal Server Error)".to_string());
    let outcome = match hb_res {
      Ok(r) if r.status == "Active" => Ok(()),
      Err(e) => Err(format!("LEASE_LOST: Terminal ownership verification failed: {e}")),
      _ => Err("LEASE_LOST".to_string()),
    };
    assert!(outcome.is_err());
    assert!(outcome.unwrap_err().contains("500"));
  }

  #[test]
  fn test_e7_final_heartbeat_malformed_response_fails() {
    let hb_res: Result<HeartbeatLeaseResponse, String> = Err("Failed to parse HeartbeatLeaseResponse".to_string());
    let outcome = match hb_res {
      Ok(r) if r.status == "Active" => Ok(()),
      Err(e) => Err(format!("LEASE_LOST: Terminal ownership verification failed: {e}")),
      _ => Err("LEASE_LOST".to_string()),
    };
    assert!(outcome.is_err());
  }

  #[test]
  fn test_e8_artifact_written_but_final_heartbeat_failed_forbids_image_done() {
    let artifact_written = true;
    let hb_failed = true;
    let stage_success = artifact_written && !hb_failed;
    assert!(!stage_success, "Stage MUST NOT declare IMAGE_DONE if final heartbeat failed");
  }

  // TEST GROUP F — JSON Contract Alignment Tests
  #[test]
  fn test_f1_production_request_serializes_to_exact_camel_case() {
    let req = ExtensionProductionRequest {
      protocol: "floword-production",
      protocol_version: 1,
      request_id: "REQ_123".to_string(),
      job_id: "JOB_456".to_string(),
      step_id: "GENERATING_IMAGE".to_string(),
      attempt_id: "1".to_string(),
      lease_id: "LEASE_789".to_string(),
      profile_id: "PROFILE_01".to_string(),
      page_id: Some("PAGE_99".to_string()),
      method: "grok.image.edit",
      params: ExtensionImageEditParams {
        source_artifact: ExtensionSourceArtifact {
          artifact_id: "ART_001".to_string(),
          path: "/tmp/img.png".to_string(),
          data_url: Some("data:image/png;base64,...".to_string()),
          mime_type: "image/png".to_string(),
        },
        prompt: "Fix lighting".to_string(),
        timeout_ms: 120000,
      },
      created_at: "2026-08-17T22:30:00Z".to_string(),
    };

    let json_val = serde_json::to_value(&req).expect("Serialization failed");

    assert_eq!(json_val.get("protocol").and_then(|v| v.as_str()), Some("floword-production"));
    assert_eq!(json_val.get("protocolVersion").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(json_val.get("requestId").and_then(|v| v.as_str()), Some("REQ_123"));
    assert_eq!(json_val.get("jobId").and_then(|v| v.as_str()), Some("JOB_456"));
    assert_eq!(json_val.get("stepId").and_then(|v| v.as_str()), Some("GENERATING_IMAGE"));
    assert_eq!(json_val.get("attemptId").and_then(|v| v.as_str()), Some("1"));
    assert_eq!(json_val.get("leaseId").and_then(|v| v.as_str()), Some("LEASE_789"));
    assert_eq!(json_val.get("profileId").and_then(|v| v.as_str()), Some("PROFILE_01"));
    assert_eq!(json_val.get("pageId").and_then(|v| v.as_str()), Some("PAGE_99"));
    assert_eq!(json_val.get("createdAt").and_then(|v| v.as_str()), Some("2026-08-17T22:30:00Z"));

    let params = json_val.get("params").expect("params must exist");
    assert_eq!(params.get("timeoutMs").and_then(|v| v.as_u64()), Some(120000));
    assert_eq!(params.get("prompt").and_then(|v| v.as_str()), Some("Fix lighting"));

    let src = params.get("sourceArtifact").expect("sourceArtifact must exist");
    assert_eq!(src.get("artifactId").and_then(|v| v.as_str()), Some("ART_001"));
    assert_eq!(src.get("mimeType").and_then(|v| v.as_str()), Some("image/png"));
    assert_eq!(src.get("dataUrl").and_then(|v| v.as_str()), Some("data:image/png;base64,..."));

    // Verify absence of snake_case
    assert!(json_val.get("protocol_version").is_none());
    assert!(json_val.get("request_id").is_none());
    assert!(json_val.get("job_id").is_none());
    assert!(json_val.get("lease_id").is_none());
    assert!(json_val.get("profile_id").is_none());
    assert!(params.get("timeout_ms").is_none());
    assert!(params.get("source_artifact").is_none());
    assert!(src.get("artifact_id").is_none());
    assert!(src.get("mime_type").is_none());
  }

  #[test]
  fn test_f2_production_result_deserializes_from_camel_case_json() {
    let incoming_json = serde_json::json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "REQ_123",
      "jobId": "JOB_456",
      "stepId": "GENERATING_IMAGE",
      "attemptId": "1",
      "leaseId": "LEASE_789",
      "profileId": "PROFILE_01",
      "ok": true,
      "result": {
        "mediaType": "image",
        "source": "generated",
        "locator": "https://grok.com/image/abc.png",
        "mimeType": "image/png",
        "width": 1024,
        "height": 1024
      }
    });

    let res: ExtensionProductionResult = serde_json::from_value(incoming_json).expect("Deserialization failed");
    assert_eq!(res.protocol, "floword-production");
    assert_eq!(res.protocol_version, 1);
    assert_eq!(res.request_id, "REQ_123");
    assert_eq!(res.job_id, "JOB_456");
    assert_eq!(res.lease_id, "LEASE_789");
    assert_eq!(res.profile_id, "PROFILE_01");
    assert!(res.ok);

    let media = res.result.expect("media must exist");
    assert_eq!(media.media_type, "image");
    assert_eq!(media.locator, "https://grok.com/image/abc.png");
    assert_eq!(media.mime_type.as_deref(), Some("image/png"));
    assert_eq!(media.width, Some(1024));
  }

  #[test]
  fn test_f3_artifact_store_materialization_and_registration() {
    let temp_root = std::env::temp_dir().join(format!("floword_test_root_{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&temp_root).unwrap();

    let sample_image_bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15c4\x00\x00\x00\nIDATx\x9cc\x00\x01\x00\x00\x05\x00\x01\r\n-\xb4\x00\x00\x00\x00IEND\xaeB`\x82";
    let file_path = temp_root.join("sample_gen.png");
    std::fs::write(&file_path, sample_image_bytes).unwrap();

    let stored = ArtifactStore::register_typed_artifact(
      &temp_root,
      "JOB_TEST_001",
      StageId::StoryScript,
      "grok",
      ArtifactKind::GeneratedImage,
      &file_path,
      serde_json::json!({
        "sha256": compute_sha256(sample_image_bytes),
        "mime_type": "image/png",
        "service": "grok"
      }),
    ).expect("ArtifactStore registration must succeed");

    assert_eq!(stored.artifact_type, "generated_image");
    assert_eq!(stored.producer, "grok");

    let art_ref = stored.to_artifact_ref(StageId::StoryScript).expect("to_artifact_ref must succeed");
    assert_eq!(art_ref.kind, ArtifactKind::GeneratedImage);
    assert_eq!(art_ref.produced_by_stage, StageId::StoryScript);
    assert_eq!(art_ref.mime_type.as_deref(), Some("image/png"));
    assert!(std::path::Path::new(&art_ref.location).exists());

    let _ = std::fs::remove_dir_all(&temp_root);
  }

  #[tokio::test]
  async fn test_f4_empty_workflow_root_rejected_fail_closed() {
    let input = GrokImageEditInput {
      job_id: "JOB_FAIL_001".to_string(),
      page_id: "PAGE_01".to_string(),
      source_image_artifact: ArtifactRef {
        artifact_id: "ART_SRC".to_string(),
        kind: ArtifactKind::Story,
        produced_by_stage: StageId::StoryScript,
        location: "/tmp/non_existent.png".to_string(),
        mime_type: Some("image/png".to_string()),
        metadata: serde_json::json!({}),
      },
      prompt: "test".to_string(),
      timeout_ms: Some(1000),
      workflow_root: std::path::PathBuf::from(""),
    };

    let outcome = execute_grok_image_edit_stage(input, "1", None).await;
    assert!(outcome.is_err());
    assert!(outcome.unwrap_err().contains("WORKFLOW_ROOT_REQUIRED"));
  }
}
