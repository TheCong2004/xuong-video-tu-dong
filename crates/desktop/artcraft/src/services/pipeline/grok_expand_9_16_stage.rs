use crate::services::pipeline::artifact_store::ArtifactStore;
use crate::services::pipeline::clients::browser_runtime_client::{acquire_worker, get_donut_browser_api_base_url, heartbeat_lease, release_lease, AcquireWorkerRequest, HeartbeatLeaseRequest};
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
pub struct GrokExpand916Input {
  pub job_id: String,
  pub page_id: String,
  pub browser_profile_id: Option<String>,
  /// MUST be the exact generated artifact output from IMAGE_DONE
  pub image_done_artifact: ArtifactRef,
  pub prompt: String,
  pub timeout_ms: Option<u64>,
  pub workflow_root: std::path::PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    Self { lease_id, released: false }
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
  params: ExtensionExpandParams,
  created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionExpandParams {
  source_artifact: ExtensionSourceArtifact,
  prompt: String,
  target_aspect_ratio: &'static str,
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
}

/// Decode actual image dimensions (width, height) directly from raw image bytes.
/// Supports PNG, JPEG, and WebP headers without heuristic guesses.
pub fn decode_image_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
  if bytes.len() < 8 {
    return Err("ARTIFACT_INVALID_IMAGE: Truncated image bytes".to_string());
  }

  // PNG
  if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
    if bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
      let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
      let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
      if width > 0 && height > 0 {
        return Ok((width, height));
      }
    }
    return Err("ARTIFACT_INVALID_IMAGE: Malformed PNG IHDR header".to_string());
  }

  // JPEG
  if bytes.starts_with(b"\xFF\xD8") {
    let mut i = 2;
    while i + 8 < bytes.len() {
      if bytes[i] != 0xFF {
        i += 1;
        continue;
      }
      let marker = bytes[i + 1];
      // SOF0 (0xC0), SOF1 (0xC1), SOF2 (0xC2)
      if marker == 0xC0 || marker == 0xC1 || marker == 0xC2 {
        let height = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
        let width = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
        if width > 0 && height > 0 {
          return Ok((width, height));
        }
      }
      if marker == 0xD9 || marker == 0xDA {
        break;
      }
      if i + 3 < bytes.len() {
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        i += 2 + len;
      } else {
        break;
      }
    }
    return Err("ARTIFACT_INVALID_IMAGE: Malformed JPEG frame header".to_string());
  }

  // WebP
  if bytes.starts_with(b"RIFF") && bytes.len() >= 30 && &bytes[8..12] == b"WEBP" {
    let chunk = &bytes[12..16];
    if chunk == b"VP8 " && bytes.len() >= 30 {
      let width = (u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF) as u32;
      let height = (u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF) as u32;
      if width > 0 && height > 0 {
        return Ok((width, height));
      }
    } else if chunk == b"VP8L" && bytes.len() >= 25 {
      let b1 = bytes[21] as u32;
      let b2 = bytes[22] as u32;
      let b3 = bytes[23] as u32;
      let b4 = bytes[24] as u32;
      let width = 1 + (((b2 & 0x3F) << 8) | b1);
      let height = 1 + (((b4 & 0xF) << 10) | (b3 << 2) | ((b2 & 0xC0) >> 6));
      return Ok((width, height));
    } else if chunk == b"VP8X" && bytes.len() >= 30 {
      let width = 1 + (bytes[24] as u32 | ((bytes[25] as u32) << 8) | ((bytes[26] as u32) << 16));
      let height = 1 + (bytes[27] as u32 | ((bytes[28] as u32) << 8) | ((bytes[29] as u32) << 16));
      return Ok((width, height));
    }
  }

  Err("ARTIFACT_INVALID_IMAGE: Unsupported image format or unreadable dimensions".to_string())
}

pub fn validate_aspect_ratio(width: u32, height: u32) -> Result<f64, String> {
  if width == 0 || height == 0 {
    return Err("ASPECT_RATIO_INVALID: Zero dimension".to_string());
  }
  let ratio = width as f64 / height as f64;
  let target = 9.0 / 16.0; // 0.5625
  let tolerance = 0.03; // [0.5325, 0.5925]
  if (ratio - target).abs() > tolerance {
    return Err(format!("ASPECT_RATIO_INVALID: Image ratio {ratio:.4} ({width}x{height}) is not 9:16 vertical ratio (expected ~0.5625)"));
  }
  Ok(ratio)
}

/// Executes single-job grok.image.expand_9_16 with 3-tier cleanup and terminal barrier.
pub async fn execute_grok_expand_9_16(input: GrokExpand916Input, attempt_id: &str, cancel_flag: Option<&Arc<AtomicBool>>) -> Result<GrokExpand916Output, String> {
  let job_id = input.job_id.clone();
  let step_id = "CONVERTING_9_16";
  let request_id = format!("REQ_{}", Uuid::new_v4().simple());

  info!("[GrokExpand916] Starting 9:16 expand: job_id={} attempt_id={} input_art={}", job_id, attempt_id, input.image_done_artifact.artifact_id);

  // Fail-closed canonical workflow root validation
  if input.workflow_root.as_os_str().is_empty() {
    return Err("WORKFLOW_ROOT_REQUIRED: GrokExpand916Input workflow_root must be a non-empty canonical PathBuf".to_string());
  }
  let workflow_root_buf = input.workflow_root.clone();

  // Tier 1: Acquire exclusive lease
  let acq_req = AcquireWorkerRequest { job_id: job_id.clone(), step_id: step_id.to_string(), attempt_id: attempt_id.to_string(), capability: "grok.expand.9_16".to_string(), pool_id: None, profile_id: input.browser_profile_id.clone(), ttl_seconds: Some(180) };

  let acq_res = acquire_worker(acq_req).await.map_err(|e| format!("Failed to acquire worker lease: {e}"))?;

  let lease_id = acq_res.lease_id.clone();
  let worker_id = acq_res.worker_id.clone();
  let profile_id = acq_res.profile_id.clone();
  info!("[GrokExpand916] Lease acquired: {lease_id} for worker {worker_id} (profile {profile_id})");

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
      let req = HeartbeatLeaseRequest { job_id: hb_job_id.clone(), attempt_id: hb_attempt_id.clone(), ttl_seconds: Some(180) };
      if let Err(e) = heartbeat_lease(&hb_lease_id, req).await {
        warn!("[GrokExpand916] Heartbeat error for lease {hb_lease_id}: {e}");
      }
    }
  });

  let exec_result: Result<(ArtifactRef, String, u32, u32, f64), String> = async {
    let source_path = input.image_done_artifact.location.clone();
    let source_file = std::path::Path::new(&source_path);
    if !source_file.exists() {
      return Err(format!("Source image artifact file does not exist at {source_path}"));
    }

    let source_bytes = tokio::fs::read(source_file).await.map_err(|e| format!("Failed to read source image artifact file: {e}"))?;
    let (source_mime, _) = detect_image_mime(&source_bytes).map_err(|e| format!("Invalid source image artifact: {e}"))?;
    let source_sha256 = compute_sha256(&source_bytes);

    use base64::Engine;
    let b64_source = base64::engine::general_purpose::STANDARD.encode(&source_bytes);
    let data_url = format!("data:{source_mime};base64,{b64_source}");

    let timeout_val = input.timeout_ms.unwrap_or(180000);
    let req_payload = ExtensionProductionRequest { protocol: "floword-production", protocol_version: 1, request_id: request_id.clone(), job_id: job_id.clone(), step_id: step_id.to_string(), attempt_id: attempt_id.to_string(), lease_id: lease_id.clone(), profile_id: profile_id.clone(), page_id: Some(input.page_id.clone()), method: "grok.image.expand_9_16", params: ExtensionExpandParams { source_artifact: ExtensionSourceArtifact { artifact_id: input.image_done_artifact.artifact_id.clone(), path: source_path.clone(), data_url: Some(data_url), mime_type: source_mime.to_string() }, prompt: input.prompt.clone(), target_aspect_ratio: "9:16", timeout_ms: timeout_val }, created_at: chrono::Utc::now().to_rfc3339() };

    let client = Client::builder().timeout(Duration::from_millis(timeout_val + 10000)).build().map_err(|e| format!("Failed to create client: {e}"))?;

    let bridge_url = format!("{}/v1/workers/{worker_id}/dispatch", get_donut_browser_api_base_url());

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
      info!("[GrokExpand916] Job cancelled in-flight! Dispatching cancel request to worker {profile_id}");
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

    let prod_result: ExtensionProductionResult = resp.json().await.map_err(|e| format!("Failed to parse production result: {e}"))?;

    if !prod_result.ok {
      let err_msg = prod_result.error.map(|e| format!("{}: {}", e.code, e.message)).unwrap_or_else(|| "Unknown extension execution error".to_string());
      return Err(format!("Extension error: {err_msg}"));
    }

    let media = prod_result.result.ok_or_else(|| "ProductionResult missing result payload".to_string())?;

    let raw_bytes = if media.locator.starts_with("data:") {
      let parts: Vec<&str> = media.locator.splitn(2, ',').collect();
      if parts.len() < 2 {
        return Err("ARTIFACT_INVALID: Malformed data URL".to_string());
      }
      use base64::Engine;
      base64::engine::general_purpose::STANDARD.decode(parts[1]).map_err(|e| format!("ARTIFACT_INVALID: Base64 decode error: {e}"))?
    } else {
      let dl_resp = client.get(&media.locator).send().await.map_err(|e| format!("ARTIFACT_DOWNLOAD_FAILED: {e}"))?;
      dl_resp.bytes().await.map_err(|e| format!("ARTIFACT_DOWNLOAD_FAILED: {e}"))?.to_vec()
    };

    if raw_bytes.is_empty() {
      return Err("ARTIFACT_INVALID: 0 byte artifact received".to_string());
    }

    let (detected_mime, ext) = detect_image_mime(&raw_bytes).map_err(|e| format!("ARTIFACT_INVALID_MIME: {e}"))?;
    let vertical_sha256 = compute_sha256(&raw_bytes);

    // Decode actual dimensions directly from downloaded image bytes
    let (width, height) = decode_image_dimensions(&raw_bytes)?;
    let ratio = validate_aspect_ratio(width, height)?;

    // Write file directly into canonical workflow directory
    let file_name = format!("{}_{}_{}.{}", job_id, step_id, attempt_id, ext);
    let file_path = workflow_root_buf.join(&file_name);
    tokio::fs::write(&file_path, &raw_bytes).await.map_err(|e| format!("ARTIFACT_MATERIALIZATION_FAILED: Failed to write {file_path:?}: {e}"))?;

    // Register typed artifact through canonical ArtifactStore
    let metadata = serde_json::json!({
      "sourceArtifactId": input.image_done_artifact.artifact_id,
      "sha256": vertical_sha256,
      "width": width,
      "height": height,
      "aspectRatio": ratio,
      "mimeType": detected_mime,
      "service": "grok"
    });

    let stored = ArtifactStore::register_typed_artifact(&workflow_root_buf, &job_id, StageId::StoryScript, "grok", ArtifactKind::VerticalImage, &file_path, metadata).map_err(|e| format!("ARTIFACT_STORE_REGISTRATION_FAILED: {e}"))?;

    let art_ref = stored.to_artifact_ref(StageId::StoryScript).map_err(|e| format!("ARTIFACT_REF_CONVERSION_FAILED: {e}"))?;

    // Final Heartbeat Barrier
    let hb_final_req = HeartbeatLeaseRequest { job_id: job_id.clone(), attempt_id: attempt_id.to_string(), ttl_seconds: Some(60) };
    let hb_final = heartbeat_lease(&lease_id, hb_final_req).await;
    match hb_final {
      Ok(hb_resp) => {
        if hb_resp.status != "ACTIVE" {
          return Err(format!("TERMINAL_OWNERSHIP_LOST: Final heartbeat status was {}, expected ACTIVE", hb_resp.status));
        }
      },
      Err(e) => {
        return Err(format!("TERMINAL_OWNERSHIP_LOST: Final heartbeat failed before stage completion: {e}"));
      },
    }

    Ok((art_ref, vertical_sha256, width, height, ratio))
  }
  .await;

  // Cleanup heartbeat
  hb_cancel.store(true, Ordering::Relaxed);
  let _ = hb_handle.await;

  // Cleanup lease
  guard.release().await;

  let source_sha256 = input.image_done_artifact.metadata.get("sha256").and_then(|v| v.as_str()).unwrap_or("").to_string();

  match exec_result {
    Ok((vertical_artifact, vertical_sha256, width, height, aspect_ratio)) => {
      info!("[GrokExpand916] Success! Vertical 9:16 artifact materialized: path={} ({width}x{height}, ratio={aspect_ratio:.4})", vertical_artifact.location);
      Ok(GrokExpand916Output { vertical_artifact, job_id, attempt_id: attempt_id.to_string(), lease_id, profile_id, source_sha256, vertical_sha256, width, height, aspect_ratio })
    },
    Err(err) => {
      error!("[GrokExpand916] Execution failed: {err}");
      Err(err)
    },
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_validate_aspect_ratio_standard_9_16() {
    assert!(validate_aspect_ratio(1080, 1920).is_ok());
    assert!(validate_aspect_ratio(720, 1280).is_ok());
    assert!(validate_aspect_ratio(864, 1536).is_ok());
  }

  #[test]
  fn test_validate_aspect_ratio_deviant_fails() {
    assert!(validate_aspect_ratio(1920, 1080).is_err()); // 16:9 landscape
    assert!(validate_aspect_ratio(1000, 1000).is_err()); // 1:1 square
    assert!(validate_aspect_ratio(0, 1000).is_err());
  }

  #[test]
  fn test_decode_image_dimensions_png_fixtures() {
    // 1080x1920 PNG fixture
    let mut png_1080_1920 = Vec::new();
    png_1080_1920.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    png_1080_1920.extend_from_slice(&[0, 0, 0, 13]); // length
    png_1080_1920.extend_from_slice(b"IHDR");
    png_1080_1920.extend_from_slice(&1080u32.to_be_bytes()); // width
    png_1080_1920.extend_from_slice(&1920u32.to_be_bytes()); // height
    png_1080_1920.extend_from_slice(&[8, 6, 0, 0, 0]); // bit depth etc

    let (w, h) = decode_image_dimensions(&png_1080_1920).unwrap();
    assert_eq!((w, h), (1080, 1920));
    assert!(validate_aspect_ratio(w, h).is_ok());

    // 720x1280 PNG fixture
    let mut png_720_1280 = Vec::new();
    png_720_1280.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    png_720_1280.extend_from_slice(&[0, 0, 0, 13]);
    png_720_1280.extend_from_slice(b"IHDR");
    png_720_1280.extend_from_slice(&720u32.to_be_bytes());
    png_720_1280.extend_from_slice(&1280u32.to_be_bytes());
    png_720_1280.extend_from_slice(&[8, 6, 0, 0, 0]);

    let (w2, h2) = decode_image_dimensions(&png_720_1280).unwrap();
    assert_eq!((w2, h2), (720, 1280));
    assert!(validate_aspect_ratio(w2, h2).is_ok());

    // 1024x1024 PNG fixture (Square -> should FAIL aspect ratio)
    let mut png_1024_1024 = Vec::new();
    png_1024_1024.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    png_1024_1024.extend_from_slice(&[0, 0, 0, 13]);
    png_1024_1024.extend_from_slice(b"IHDR");
    png_1024_1024.extend_from_slice(&1024u32.to_be_bytes());
    png_1024_1024.extend_from_slice(&1024u32.to_be_bytes());
    png_1024_1024.extend_from_slice(&[8, 6, 0, 0, 0]);

    let (w3, h3) = decode_image_dimensions(&png_1024_1024).unwrap();
    assert_eq!((w3, h3), (1024, 1024));
    assert!(validate_aspect_ratio(w3, h3).is_err());

    // 1920x1080 PNG fixture (Landscape -> should FAIL aspect ratio)
    let mut png_1920_1080 = Vec::new();
    png_1920_1080.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    png_1920_1080.extend_from_slice(&[0, 0, 0, 13]);
    png_1920_1080.extend_from_slice(b"IHDR");
    png_1920_1080.extend_from_slice(&1920u32.to_be_bytes());
    png_1920_1080.extend_from_slice(&1080u32.to_be_bytes());
    png_1920_1080.extend_from_slice(&[8, 6, 0, 0, 0]);

    let (w4, h4) = decode_image_dimensions(&png_1920_1080).unwrap();
    assert_eq!((w4, h4), (1920, 1080));
    assert!(validate_aspect_ratio(w4, h4).is_err());
  }

  #[test]
  fn test_empty_workflow_root_rejected_fail_closed() {
    let input = GrokExpand916Input { job_id: "JOB_EXP_001".to_string(), page_id: "PAGE_01".to_string(), browser_profile_id: None, image_done_artifact: ArtifactRef { artifact_id: "ART_GEN".to_string(), kind: ArtifactKind::GeneratedImage, produced_by_stage: StageId::StoryScript, location: "/non_existent.png".to_string(), mime_type: Some("image/png".to_string()), metadata: serde_json::json!({}) }, prompt: "expand to 9:16".to_string(), timeout_ms: Some(1000), workflow_root: std::path::PathBuf::from("") };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let outcome = rt.block_on(execute_grok_expand_9_16(input, "1", None));
    assert!(outcome.is_err());
    assert!(outcome.unwrap_err().contains("WORKFLOW_ROOT_REQUIRED"));
  }
}
