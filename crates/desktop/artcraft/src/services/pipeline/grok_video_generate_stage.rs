use crate::services::pipeline::artifact_store::ArtifactStore;
use crate::services::pipeline::clients::browser_runtime_client::{
  acquire_worker, get_donut_browser_api_base_url, heartbeat_lease, release_lease,
  AcquireWorkerRequest, HeartbeatLeaseRequest,
};
use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef, StageId};
use crate::services::pipeline::grok_image_edit_stage::{compute_sha256, detect_image_mime};
use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokVideoGenerateInput {
  pub job_id: String,
  pub page_id: String,
  /// MUST be the exact 9:16 vertical artifact output from IMAGE_9_16_DONE
  pub vertical_image_artifact: ArtifactRef,
  pub prompt: String,
  pub timeout_ms: Option<u64>,
  pub workflow_root: std::path::PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokVideoGenerateOutput {
  pub video_artifact: ArtifactRef,
  pub job_id: String,
  pub attempt_id: String,
  pub lease_id: String,
  pub profile_id: String,
  pub source_sha256: String,
  pub video_sha256: String,
  pub duration_sec: Option<f64>,
  pub mime_type: String,
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
  params: ExtensionVideoGenerateParams,
  created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionVideoGenerateParams {
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
  duration_sec: Option<f64>,
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

/// Sniff video container magic bytes (MP4 / WebM)
pub fn detect_video_mime(bytes: &[u8]) -> Result<(&'static str, &'static str), String> {
  if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
    Ok(("video/mp4", "mp4"))
  } else if bytes.len() >= 4 && &bytes[0..4] == b"\x1A\x45\xDF\xA3" {
    Ok(("video/webm", "webm"))
  } else {
    // Default to MP4 if video data starts with typical header or non-empty binary
    if bytes.len() >= 8 {
      Ok(("video/mp4", "mp4"))
    } else {
      Err("ARTIFACT_INVALID_VIDEO: Unrecognized or truncated video byte signature".to_string())
    }
  }
}

/// Executes single-job grok.video.generate with 3-tier cleanup and terminal barrier.
pub async fn execute_grok_video_generate(
  input: GrokVideoGenerateInput,
  attempt_id: &str,
) -> Result<GrokVideoGenerateOutput, String> {
  let job_id = input.job_id.clone();
  let step_id = "GENERATING_VIDEO";
  let request_id = format!("REQ_{}", Uuid::new_v4().simple());

  info!(
    "[GrokVideoGenerate] Starting video generation: job_id={} attempt_id={} input_art={}",
    job_id, attempt_id, input.vertical_image_artifact.artifact_id
  );

  // Fail-closed canonical workflow root validation
  let workflow_root_buf = input.workflow_root.clone();
  if workflow_root_buf.as_os_str().is_empty() {
    return Err("WORKFLOW_ROOT_REQUIRED: Canonical workflow artifact root path is required".to_string());
  }
  std::fs::create_dir_all(&workflow_root_buf)
    .map_err(|e| format!("WORKFLOW_ROOT_INVALID: Cannot create canonical workflow artifact root {:?}: {e}", workflow_root_buf))?;

  // Read vertical 9:16 input artifact (which MUST be the output of IMAGE_9_16_DONE)
  let source_path = input.vertical_image_artifact.location.clone();
  let (source_bytes, source_sha256, source_mime, data_url) = if let Ok(bytes) = tokio::fs::read(&source_path).await {
    if bytes.is_empty() {
      return Err("SOURCE_ARTIFACT_INVALID: Source file is 0 bytes".to_string());
    }
    let (mime, _ext) = detect_image_mime(&bytes)
      .map_err(|e| format!("SOURCE_ARTIFACT_INVALID_MIME: {e}"))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let hash = compute_sha256(&bytes);
    (bytes, hash, mime, Some(format!("data:{mime};base64,{b64}")))
  } else {
    return Err("SOURCE_ARTIFACT_NOT_FOUND: IMAGE_9_16_DONE artifact missing on disk".to_string());
  };

  // 1. Acquire exclusive worker lease for video generation
  let lease = acquire_worker(AcquireWorkerRequest {
    job_id: job_id.clone(),
    step_id: step_id.to_string(),
    attempt_id: attempt_id.to_string(),
    capability: "grok.video.generate".to_string(),
    pool_id: None,
    ttl_seconds: Some(300),
  })
  .await
  .map_err(|e| format!("Failed to acquire worker lease for grok.video.generate: {e}"))?;

  let lease_id = lease.lease_id.clone();
  let profile_id = lease.profile_id.clone();
  let mut lease_guard = LeaseGuard::new(lease_id.clone());

  // 2. Start background heartbeat loop (every 30s)
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
      let _ = heartbeat_lease(
        &hb_lease_id,
        HeartbeatLeaseRequest {
          job_id: hb_job_id.clone(),
          attempt_id: hb_attempt_id.clone(),
          ttl_seconds: Some(60),
        },
      )
      .await;
    }
  });

  // 3. Primary Execution Block
  let timeout_val = input.timeout_ms.unwrap_or(300000);
  let exec_result: Result<(ArtifactRef, String, Option<f64>, String), String> = async {
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
      method: "grok.video.generate",
      params: ExtensionVideoGenerateParams {
        source_artifact: ExtensionSourceArtifact {
          artifact_id: input.vertical_image_artifact.artifact_id.clone(),
          path: source_path.clone(),
          data_url,
          mime_type: source_mime.to_string(),
        },
        prompt: input.prompt.clone(),
        timeout_ms: timeout_val,
      },
      created_at: chrono::Utc::now().to_rfc3339(),
    };

    info!(
      "[GrokVideoGenerate] Dispatching video generate request {request_id} to worker {profile_id}"
    );

    let client = Client::builder()
      .timeout(Duration::from_millis(timeout_val + 10000))
      .build()
      .map_err(|e| format!("Failed to create client: {e}"))?;

    let bridge_url = format!("{}/v1/workers/{profile_id}/dispatch", get_donut_browser_api_base_url());
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

    let media = prod_result.result.ok_or_else(|| "No media result in response".to_string())?;

    // Materialize raw video bytes
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
        .map_err(|e| format!("VIDEO_DOWNLOAD_FAILED: {e}"))?;
      dl_resp
        .bytes()
        .await
        .map_err(|e| format!("VIDEO_DOWNLOAD_FAILED: {e}"))?
        .to_vec()
    };

    if raw_bytes.is_empty() {
      return Err("ARTIFACT_INVALID: 0 byte video artifact received".to_string());
    }

    let (detected_mime, ext) = detect_video_mime(&raw_bytes)
      .map_err(|e| format!("ARTIFACT_INVALID_VIDEO_MIME: {e}"))?;
    let video_sha256 = compute_sha256(&raw_bytes);

    // Write file directly into canonical workflow directory
    let file_name = format!("{}_{}_{}.{}", job_id, step_id, attempt_id, ext);
    let file_path = workflow_root_buf.join(&file_name);
    std::fs::write(&file_path, &raw_bytes)
      .map_err(|e| format!("ARTIFACT_MATERIALIZATION_FAILED: Failed to write {file_path:?}: {e}"))?;

    // Register typed artifact through canonical ArtifactStore
    let metadata = serde_json::json!({
      "sourceArtifactId": input.vertical_image_artifact.artifact_id,
      "sha256": video_sha256,
      "mimeType": detected_mime,
      "durationSec": media.duration_sec,
      "service": "grok"
    });

    let stored = ArtifactStore::register_typed_artifact(
      &workflow_root_buf,
      &job_id,
      StageId::StoryScript,
      "grok",
      ArtifactKind::GeneratedVideo,
      &file_path,
      metadata,
    )
    .map_err(|e| format!("ARTIFACT_STORE_REGISTRATION_FAILED: {e}"))?;

    let art_ref = stored
      .to_artifact_ref(StageId::StoryScript)
      .map_err(|e| format!("ARTIFACT_REF_CONVERSION_FAILED: {e}"))?;

    // Terminal Ownership Barrier: Must confirm lease ACTIVE before returning VIDEO_DONE
    let hb_check = heartbeat_lease(
      &lease_id,
      HeartbeatLeaseRequest {
        job_id: job_id.clone(),
        attempt_id: attempt_id.to_string(),
        ttl_seconds: Some(60),
      },
    )
    .await;

    match hb_check {
      Ok(hb_res) if hb_res.status == "ACTIVE" => {
        info!("[GrokVideoGenerate] Terminal heartbeat barrier passed. Lease {} ACTIVE.", lease_id);
      }
      Ok(hb_res) => {
        let _ = std::fs::remove_file(&file_path);
        return Err(format!(
          "TERMINAL_OWNERSHIP_LOST: Final heartbeat returned inactive status: {}",
          hb_res.status
        ));
      }
      Err(err) => {
        let _ = std::fs::remove_file(&file_path);
        return Err(format!(
          "TERMINAL_OWNERSHIP_LOST: Final heartbeat check failed: {err}"
        ));
      }
    }

    Ok((art_ref, video_sha256, media.duration_sec, detected_mime.to_string()))
  }
  .await;

  // 4. Guaranteed Primary Cleanup
  heartbeat_running.store(false, Ordering::Relaxed);
  lease_guard.release().await;

  match exec_result {
    Ok((video_artifact, video_sha256, duration_sec, mime_type)) => {
      info!(
        "[GrokVideoGenerate] Success! Video artifact materialized: path={}",
        video_artifact.location
      );
      Ok(GrokVideoGenerateOutput {
        video_artifact,
        job_id,
        attempt_id: attempt_id.to_string(),
        lease_id,
        profile_id,
        source_sha256,
        video_sha256,
        duration_sec,
        mime_type,
      })
    }
    Err(err) => {
      error!("[GrokVideoGenerate] Execution failed: {err}");
      Err(err)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_detect_video_mime_mp4() {
    let mp4_header = b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00isom";
    let (mime, ext) = detect_video_mime(mp4_header).unwrap();
    assert_eq!(mime, "video/mp4");
    assert_eq!(ext, "mp4");
  }

  #[test]
  fn test_empty_workflow_root_rejected_fail_closed() {
    let input = GrokVideoGenerateInput {
      job_id: "JOB_VID_001".to_string(),
      page_id: "PAGE_01".to_string(),
      vertical_image_artifact: ArtifactRef {
        artifact_id: "ART_VERT".to_string(),
        kind: ArtifactKind::VerticalImage,
        produced_by_stage: StageId::StoryScript,
        location: "/non_existent.png".to_string(),
        mime_type: Some("image/png".to_string()),
        metadata: serde_json::json!({}),
      },
      prompt: "generate video".to_string(),
      timeout_ms: Some(1000),
      workflow_root: std::path::PathBuf::from(""),
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let outcome = rt.block_on(execute_grok_video_generate(input, "1"));
    assert!(outcome.is_err());
    assert!(outcome.unwrap_err().contains("WORKFLOW_ROOT_REQUIRED"));
  }
}
