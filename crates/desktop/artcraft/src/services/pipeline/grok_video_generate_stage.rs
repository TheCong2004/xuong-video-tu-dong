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

/// Sniff video container magic bytes (MP4 / WebM) strictly fail-closed
pub fn detect_video_mime(bytes: &[u8]) -> Result<(&'static str, &'static str), String> {
  if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
    Ok(("video/mp4", "mp4"))
  } else if bytes.len() >= 4 && &bytes[0..4] == b"\x1A\x45\xDF\xA3" {
    Ok(("video/webm", "webm"))
  } else {
    Err("ARTIFACT_INVALID_VIDEO_MIME: Unrecognized or invalid video container signature".to_string())
  }
}

/// Executes single-job grok.video.generate with 3-tier cleanup and terminal barrier.
pub async fn execute_grok_video_generate(
  input: GrokVideoGenerateInput,
  attempt_id: &str,
  cancel_flag: Option<&Arc<AtomicBool>>,
) -> Result<GrokVideoGenerateOutput, String> {
  let job_id = input.job_id.clone();
  let step_id = "GENERATING_VIDEO";
  let request_id = format!("REQ_{}", Uuid::new_v4().simple());

  info!(
    "[GrokVideoGenerate] Starting video generation: job_id={} attempt_id={} input_art={}",
    job_id, attempt_id, input.vertical_image_artifact.artifact_id
  );

  // Fail-closed canonical workflow root validation
  if input.workflow_root.as_os_str().is_empty() {
    return Err("WORKFLOW_ROOT_REQUIRED: GrokVideoGenerateInput workflow_root must be a non-empty canonical PathBuf".to_string());
  }
  let workflow_root_buf = input.workflow_root.clone();

  // Tier 1: Acquire exclusive lease
  let acq_req = AcquireWorkerRequest {
    job_id: job_id.clone(),
    step_id: step_id.to_string(),
    attempt_id: attempt_id.to_string(),
    capability: "grok.video.generate".to_string(),
    pool_id: None,
    ttl_seconds: Some(300),
  };

  let acq_res = acquire_worker(acq_req)
    .await
    .map_err(|e| format!("Failed to acquire worker lease: {e}"))?;

  let lease_id = acq_res.lease_id.clone();
  let profile_id = acq_res.profile_id.clone();
  info!("[GrokVideoGenerate] Lease acquired: {lease_id} for profile {profile_id}");

  let mut guard = LeaseGuard::new(lease_id.clone());

  // Heartbeat loop
  let hb_cancel = Arc::new(AtomicBool::new(false));
  let hb_cancel_clone = hb_cancel.clone();
  let hb_lease_id = lease_id.clone();
  let hb_job_id = job_id.clone();
  let hb_attempt_id = attempt_id.to_string();

  let hb_handle = tokio::spawn(async move {
    while !hb_cancel_clone.load(Ordering::Relaxed) {
      sleep(Duration::from_secs(30)).await;
      if hb_cancel_clone.load(Ordering::Relaxed) {
        break;
      }
      let req = HeartbeatLeaseRequest {
        job_id: hb_job_id.clone(),
        attempt_id: hb_attempt_id.clone(),
        ttl_seconds: Some(300),
      };
      if let Err(e) = heartbeat_lease(&hb_lease_id, req).await {
        warn!("[GrokVideoGenerate] Heartbeat error for lease {hb_lease_id}: {e}");
      }
    }
  });

  let exec_result: Result<(ArtifactRef, String, Option<f64>, String), String> = async {
    let source_path = input.vertical_image_artifact.location.clone();
    let source_file = std::path::Path::new(&source_path);
    if !source_file.exists() {
      return Err(format!("Vertical 9:16 source image artifact file does not exist at {source_path}"));
    }

    let source_bytes = tokio::fs::read(source_file)
      .await
      .map_err(|e| format!("Failed to read source image artifact file: {e}"))?;
    let (source_mime, _) = detect_image_mime(&source_bytes)
      .map_err(|e| format!("Invalid source image artifact: {e}"))?;
    let source_sha256 = compute_sha256(&source_bytes);

    use base64::Engine;
    let b64_source = base64::engine::general_purpose::STANDARD.encode(&source_bytes);
    let data_url = format!("data:{source_mime};base64,{b64_source}");

    let timeout_val = input.timeout_ms.unwrap_or(300000);
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
          data_url: Some(data_url),
          mime_type: source_mime.to_string(),
        },
        prompt: input.prompt.clone(),
        timeout_ms: timeout_val,
      },
      created_at: chrono::Utc::now().to_rfc3339(),
    };

    let client = Client::builder()
      .timeout(Duration::from_millis(timeout_val + 10000))
      .build()
      .map_err(|e| format!("Failed to create client: {e}"))?;

    let bridge_url = format!("{}/v1/workers/{profile_id}/dispatch", get_donut_browser_api_base_url());

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
      info!("[GrokVideoGenerate] Job cancelled in-flight! Dispatching cancel request to worker {profile_id}");
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
      guard.release().await;
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
      let err_msg = prod_result
        .error
        .map(|e| format!("{}: {}", e.code, e.message))
        .unwrap_or_else(|| "Unknown extension execution error".to_string());
      return Err(format!("Extension error: {err_msg}"));
    }

    let media = prod_result
      .result
      .ok_or_else(|| "ProductionResult missing result payload".to_string())?;

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

    let (detected_mime, ext) = detect_video_mime(&raw_bytes)
      .map_err(|e| format!("ARTIFACT_INVALID_MIME: {e}"))?;
    let video_sha256 = compute_sha256(&raw_bytes);

    let file_name = format!("{}_{}_{}.{}", job_id, step_id, attempt_id, ext);
    let file_path = workflow_root_buf.join(&file_name);
    tokio::fs::write(&file_path, &raw_bytes)
      .await
      .map_err(|e| format!("ARTIFACT_MATERIALIZATION_FAILED: Failed to write {file_path:?}: {e}"))?;

    let metadata = serde_json::json!({
      "sourceArtifactId": input.vertical_image_artifact.artifact_id,
      "sha256": video_sha256,
      "durationSec": media.duration_sec,
      "mimeType": detected_mime,
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

    let hb_final_req = HeartbeatLeaseRequest {
      job_id: job_id.clone(),
      attempt_id: attempt_id.to_string(),
      ttl_seconds: Some(60),
    };
    let hb_final = heartbeat_lease(&lease_id, hb_final_req).await;
    match hb_final {
      Ok(hb_resp) => {
        if hb_resp.status != "ACTIVE" {
          return Err(format!(
            "TERMINAL_OWNERSHIP_LOST: Final heartbeat status was {}, expected ACTIVE",
            hb_resp.status
          ));
        }
      }
      Err(e) => {
        return Err(format!(
          "TERMINAL_OWNERSHIP_LOST: Final heartbeat failed before stage completion: {e}"
        ));
      }
    }

    Ok((art_ref, video_sha256, media.duration_sec, detected_mime.to_string()))
  }
  .await;

  hb_cancel.store(true, Ordering::Relaxed);
  let _ = hb_handle.await;

  guard.release().await;

  let source_sha256 = input
    .vertical_image_artifact
    .metadata
    .get("sha256")
    .and_then(|v| v.as_str())
    .unwrap_or("")
    .to_string();

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
  fn test_detect_video_mime_webm() {
    let webm_header = b"\x1A\x45\xDF\xA3\x93\x42\x86\x81\x01\x42\xF7\x81";
    let (mime, ext) = detect_video_mime(webm_header).unwrap();
    assert_eq!(mime, "video/webm");
    assert_eq!(ext, "webm");
  }

  #[test]
  fn test_detect_video_mime_non_video_fails_closed() {
    // HTML
    assert!(detect_video_mime(b"<!DOCTYPE html><html><body>Error</body></html>").is_err());
    // JSON
    assert!(detect_video_mime(b"{\"error\": \"not found\"}").is_err());
    // PNG
    assert!(detect_video_mime(b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR").is_err());
    // JPEG
    assert!(detect_video_mime(b"\xFF\xD8\xFF\xE0\x00\x10JFIF").is_err());
    // Random bytes
    assert!(detect_video_mime(b"1234567890abcdef").is_err());
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
    let outcome = rt.block_on(execute_grok_video_generate(input, "1", None));
    assert!(outcome.is_err());
    assert!(outcome.unwrap_err().contains("WORKFLOW_ROOT_REQUIRED"));
  }
}
