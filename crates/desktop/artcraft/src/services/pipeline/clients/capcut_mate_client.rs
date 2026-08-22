//! HTTP client for the CapCut Mate (JianYing) backend — used by the pipeline
//! worker to create drafts, inject captions, save projects, and query export capabilities.

use crate::services::pipeline::capcut::MediaPlacement;
use std::collections::HashMap;
use crate::services::pipeline::caption_segmenter::{segment_script_to_captions, CaptionSegment};
use errors::AnyhowResult;
use log::{error, info, warn};
use reqwest::Client;
use serde_json::{json, Value};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_CAPCUT_MATE_BASE_URL: &str = "http://127.0.0.1:30000";
pub const CAPCUT_PREFIX: &str = "/openapi/capcut-mate/v1";

pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 2000;
pub const DEFAULT_JOB_TIMEOUT_SECS: u64 = 600;

pub const DEFAULT_WIDTH: u32 = 1080;
pub const DEFAULT_HEIGHT: u32 = 1920;

fn get_capcut_mate_base_url() -> String {
  env::var("CAPCUT_MATE_BASE_URL").unwrap_or_else(|_| DEFAULT_CAPCUT_MATE_BASE_URL.to_string())
}

fn get_timeout() -> Duration {
  let secs = env::var("REQUEST_TIMEOUT_SECONDS").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(DEFAULT_TIMEOUT_SECS);
  Duration::from_secs(secs)
}

fn get_poll_interval() -> Duration {
  let ms = env::var("PIPELINE_POLL_INTERVAL_MS").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(DEFAULT_POLL_INTERVAL_MS);
  Duration::from_millis(ms)
}

fn get_job_timeout() -> Duration {
  let secs = env::var("PIPELINE_JOB_TIMEOUT_SECONDS").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(DEFAULT_JOB_TIMEOUT_SECS);
  Duration::from_secs(secs)
}

/// Health check to verify CapCut Mate backend reachability.
pub async fn health_check() -> Result<(), String> {
  let base_url = get_capcut_mate_base_url();
  let url = format!("{}/health", base_url.trim_end_matches('/'));
  let client = Client::builder().timeout(Duration::from_secs(5)).build().map_err(|e| format!("CAPCUT_UNAVAILABLE: Failed to build HTTP client: {e}"))?;

  match client.get(&url).send().await {
    Ok(res) => {
      let status = res.status();
      if status.is_success() {
        Ok(())
      } else {
        Err(format!("CAPCUT_UNAVAILABLE: HTTP status {}", status.as_u16()))
      }
    },
    Err(err) => Err(format!("CAPCUT_UNAVAILABLE: Connection failed to {url}: {err}")),
  }
}

/// Assembly flow: create_draft -> add_captions -> save_draft -> verify_draft -> (gen_video if supported).
/// Individual steps are `pub` so the pipeline worker can drive the state machine stage-by-stage.

#[derive(Debug, Clone)]
pub struct CreatedDraft {
  pub draft_url: String,
  pub draft_id: String,
  pub draft_path: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PublishedDraft {
  pub draft_id: String,
  pub staging_path: PathBuf,
  pub desktop_root: PathBuf,
  pub final_path: PathBuf,
  pub video_count: u64,
  pub audio_count: u64,
  pub caption_count: u64,
}

fn parse_published_draft(response: &Value) -> AnyhowResult<PublishedDraft> {
  let required_path = |key: &str| response.get(key).and_then(Value::as_str).filter(|value| !value.trim().is_empty()).map(PathBuf::from).ok_or_else(|| anyhow::anyhow!("CAPCUT_DRAFT_VERIFY_FAILED: publish response missing {key}"));
  let media = response.get("media").and_then(Value::as_object).ok_or_else(|| anyhow::anyhow!("CAPCUT_DRAFT_VERIFY_FAILED: publish response missing media validation"))?;
  Ok(PublishedDraft { draft_id: response.get("draft_id").and_then(Value::as_str).filter(|value| !value.trim().is_empty()).ok_or_else(|| anyhow::anyhow!("CAPCUT_DRAFT_VERIFY_FAILED: publish response missing draft_id"))?.to_string(), staging_path: required_path("staging_path")?, desktop_root: required_path("desktop_root")?, final_path: required_path("final_path")?, video_count: media.get("video").and_then(Value::as_u64).unwrap_or(0), audio_count: media.get("audio").and_then(Value::as_u64).unwrap_or(0), caption_count: media.get("captions").and_then(Value::as_u64).unwrap_or(0) })
}

fn parse_created_draft(response: &Value) -> AnyhowResult<CreatedDraft> {
  let draft_url = response.get("draft_url").and_then(Value::as_str).filter(|value| !value.trim().is_empty()).ok_or_else(|| anyhow::anyhow!("CAPCUT_DRAFT_CREATE_FAILED: create_draft response missing draft_url"))?.to_string();
  let draft_id = response.get("draft_id").and_then(Value::as_str).filter(|value| !value.trim().is_empty()).ok_or_else(|| anyhow::anyhow!("CAPCUT_DRAFT_CREATE_FAILED: create_draft response missing draft_id"))?.to_string();
  let draft_path = response.get("draft_path").and_then(Value::as_str).filter(|value| !value.trim().is_empty()).map(PathBuf::from).ok_or_else(|| anyhow::anyhow!("CAPCUT_DRAFT_CREATE_FAILED: create_draft response missing backend-owned draft_path"))?;
  Ok(CreatedDraft { draft_url, draft_id, draft_path })
}

fn validate_created_draft(created: &CreatedDraft) -> AnyhowResult<()> {
  if !created.draft_path.is_dir() {
    return Err(anyhow::anyhow!("CAPCUT_DRAFT_CREATE_FAILED: backend returned a draft path that does not exist: {}", created.draft_path.display()));
  }
  for required in ["draft_content.json", "draft_info.json"] {
    if !created.draft_path.join(required).is_file() {
      return Err(anyhow::anyhow!("CAPCUT_DRAFT_CREATE_FAILED: created draft is missing required file {required}"));
    }
  }
  Ok(())
}

/// Create a draft and preserve the backend-owned id/path contract.
pub async fn create_draft(client: &Client, width: u32, height: u32) -> AnyhowResult<CreatedDraft> {
  let body = json!({ "width": width, "height": height });
  let response = post(client, "/create_draft", &body).await.map_err(|error| anyhow::anyhow!("CAPCUT_DRAFT_CREATE_FAILED: {error}"))?;
  let mut created = parse_created_draft(&response)?;
  validate_created_draft(&created)?;
  created.draft_path = std::fs::canonicalize(&created.draft_path).map_err(|error| anyhow::anyhow!("CAPCUT_DRAFT_CREATE_FAILED: failed to normalize draft path: {error}"))?;
  info!("[CAPCUT][CREATE DRAFT] draft_id={} draft_path={} exists=true", created.draft_id, created.draft_path.display());
  Ok(created)
}

/// Add structured captions array to draft.
pub async fn add_captions(client: &Client, draft_url: &str, captions: &[CaptionSegment]) -> AnyhowResult<()> {
  let captions_json = serde_json::to_string(captions)?;
  let body = json!({
    "draft_url": draft_url,
    "captions": captions_json,
  });

  post(client, "/add_captions", &body).await.map_err(|e| anyhow::anyhow!("CAPTION_ADD_FAILED: {e}"))?;
  info!("[CAPCUT][CAPTIONS_ADDED] Injected {} captions into timeline", captions.len());
  Ok(())
}

/// Register an ArtifactStore file with the unified backend's ephemeral local
/// transport. This is transport only; Rust remains the canonical artifact store.
pub async fn register_artifact_asset(client: &Client, artifact_id: &str, path: &Path) -> AnyhowResult<String> {
  let response = post_gateway(client, "/api/capcut/assets", &json!({ "artifactId": artifact_id, "path": path })).await?;
  response.get("url").and_then(Value::as_str).map(str::to_string).ok_or_else(|| anyhow::anyhow!("CAPCUT_ASSET_TRANSPORT_FAILED: response missing url"))
}

pub async fn add_videos(client: &Client, draft_url: &str, asset_url: &str, segments: &[MediaPlacement]) -> AnyhowResult<()> {
  let videos = segments.iter().map(|segment| json!({ "video_url": asset_url, "start": segment.source_start_us, "end": segment.source_start_us + segment.duration_us, "duration": segment.duration_us, "volume": 0.0 })).collect::<Vec<_>>();
  let scene_timelines = segments.iter().map(|segment| json!({ "start": segment.start_us, "end": segment.start_us + segment.duration_us })).collect::<Vec<_>>();
  let body = json!({ "draft_url": draft_url, "video_infos": serde_json::to_string(&videos)?, "scene_timelines": scene_timelines });
  post(client, "/add_videos", &body).await.map_err(|error| anyhow::anyhow!("VIDEO_ADD_FAILED: {error}"))?;
  Ok(())
}

pub async fn add_video_assets(client: &Client, draft_url: &str, asset_urls: &HashMap<String, String>, segments: &[MediaPlacement]) -> AnyhowResult<()> {
  let videos = segments
    .iter()
    .map(|segment| {
      let asset_url = asset_urls.get(&segment.artifact_id).ok_or_else(|| anyhow::anyhow!("CAPCUT_MEDIA_REFERENCE_INVALID: no transport URL for {}", segment.artifact_id))?;
      Ok(json!({ "video_url": asset_url, "start": segment.source_start_us, "end": segment.source_start_us + segment.duration_us, "duration": segment.duration_us, "volume": 0.0 }))
    })
    .collect::<AnyhowResult<Vec<_>>>()?;
  let scene_timelines = segments.iter().map(|segment| json!({ "start": segment.start_us, "end": segment.start_us + segment.duration_us })).collect::<Vec<_>>();
  let body = json!({ "draft_url": draft_url, "video_infos": serde_json::to_string(&videos)?, "scene_timelines": scene_timelines });
  post(client, "/add_videos", &body).await.map_err(|error| anyhow::anyhow!("VIDEO_ADD_FAILED: {error}"))?;
  Ok(())
}

pub async fn add_audio(client: &Client, draft_url: &str, asset_url: &str, segment: &MediaPlacement) -> AnyhowResult<()> {
  let audios = json!([{ "audio_url": asset_url, "start": segment.start_us, "end": segment.start_us + segment.duration_us, "duration": segment.duration_us, "volume": 1.0 }]);
  let body = json!({ "draft_url": draft_url, "audio_infos": serde_json::to_string(&audios)? });
  post(client, "/add_audios", &body).await.map_err(|error| anyhow::anyhow!("AUDIO_ADD_FAILED: {error}"))?;
  Ok(())
}

/// Save draft.
pub async fn save_draft(client: &Client, draft_url: &str) -> AnyhowResult<String> {
  let body = json!({ "draft_url": draft_url });
  let response = post(client, "/save_draft", &body).await.map_err(|e| anyhow::anyhow!("DRAFT_SAVE_FAILED: {e}"))?;

  let saved_url = response.get("draft_url").and_then(|v| v.as_str()).unwrap_or(draft_url).to_string();

  Ok(saved_url)
}

/// Publish a fully saved staging draft into the CapCut Desktop project root.
/// CapCut Mate owns root resolution, atomic copy, metadata repair, and validation.
pub async fn publish_draft(client: &Client, created: &CreatedDraft) -> AnyhowResult<PublishedDraft> {
  let response = post(client, "/publish_draft", &json!({ "draft_id": created.draft_id, "staging_path": created.draft_path })).await?;
  let published = parse_published_draft(&response)?;
  if published.draft_id != created.draft_id || published.staging_path == published.final_path {
    return Err(anyhow::anyhow!("CAPCUT_DRAFT_VERIFY_FAILED: published draft identity/path contract is invalid"));
  }
  if !published.final_path.is_dir() {
    return Err(anyhow::anyhow!("CAPCUT_DRAFT_VERIFY_FAILED: final CapCut Desktop draft directory does not exist: {}", published.final_path.display()));
  }
  for required in ["draft_content.json", "draft_info.json", "draft_meta_info.json"] {
    if !published.final_path.join(required).is_file() {
      return Err(anyhow::anyhow!("CAPCUT_DRAFT_VERIFY_FAILED: final draft is missing {required}"));
    }
  }
  if published.video_count == 0 || published.audio_count == 0 || published.caption_count == 0 {
    return Err(anyhow::anyhow!("CAPCUT_MEDIA_REFERENCE_INVALID: published draft is missing verified video, audio, or captions"));
  }
  info!("[CAPCUT][PUBLISH] draft_id={} staging_path={} desktop_root={} final_path={} video={} audio={} captions={}", published.draft_id, published.staging_path.display(), published.desktop_root.display(), published.final_path.display(), published.video_count, published.audio_count, published.caption_count);
  Ok(published)
}

/// Verify draft existence via get_draft API endpoint.
pub async fn verify_draft_exists(client: &Client, draft_id: &str) -> AnyhowResult<()> {
  let base_url = get_capcut_mate_base_url();
  let url = format!("{}/openapi/capcut-mate/v1/get_draft?draft_id={}", base_url.trim_end_matches('/'), draft_id);

  match client.get(&url).send().await {
    Ok(res) if res.status().is_success() => Ok(()),
    Ok(res) => {
      let status = res.status().as_u16();
      Err(anyhow::anyhow!("DRAFT_SAVE_FAILED: get_draft validation failed with HTTP {}", status))
    },
    Err(e) => Err(anyhow::anyhow!("DRAFT_SAVE_FAILED: get_draft validation error: {e}")),
  }
}

/// Real draft properties read back from CapCut Mate's `get_draft` response.
/// Track counts are `None` when the backend does not report them — the worker
/// must not substitute hard-coded numbers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DraftManifest {
  pub draft_id: String,
  pub visual_track_count: Option<u64>,
  pub audio_track_count: Option<u64>,
  pub caption_track_count: Option<u64>,
  pub timeline_duration_us: Option<u64>,
  /// Where the counts came from: "capcut_get_draft_tracks" when the backend
  /// reported a tracks array, "capcut_get_draft_no_tracks" when it did not.
  pub source: String,
}

/// Inspect a saved draft via `get_draft`, extracting whatever track/timeline
/// metadata the backend actually reports. Missing fields stay `None`.
pub async fn inspect_draft(client: &Client, draft_id: &str, draft_path: &Path) -> AnyhowResult<DraftManifest> {
  let base_url = get_capcut_mate_base_url();
  let url = format!("{}/openapi/capcut-mate/v1/get_draft?draft_id={}", base_url.trim_end_matches('/'), draft_id);

  let response = client.get(&url).send().await.map_err(|e| anyhow::anyhow!("DRAFT_SAVE_FAILED: get_draft inspection error: {e}"))?;
  let status = response.status();
  let text = response.text().await.unwrap_or_default();
  if !status.is_success() {
    return Err(anyhow::anyhow!("DRAFT_SAVE_FAILED: get_draft inspection HTTP {}", status.as_u16()));
  }

  let parsed: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
  // Tracks may live at the top level, under `data`, or under `draft` depending on
  // the backend version — probe each without inventing values.
  let root = parsed.get("data").or_else(|| parsed.get("draft")).unwrap_or(&parsed);

  let tracks = root.get("tracks").and_then(|v| v.as_array());
  let (mut visual, mut audio, mut caption) = (None::<u64>, None::<u64>, None::<u64>);
  let source;
  if let Some(tracks) = tracks {
    let (mut v, mut a, mut c) = (0u64, 0u64, 0u64);
    for track in tracks {
      match track.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "video" | "image" | "visual" => v += 1,
        "audio" | "voice" | "music" => a += 1,
        "text" | "caption" | "subtitle" => c += 1,
        _ => {},
      }
    }
    visual = Some(v);
    audio = Some(a);
    caption = Some(c);
    source = "capcut_get_draft_tracks".to_string();
  } else {
    info!("[CAPCUT][BEFORE /local/tracks] draft_id={} draft_path={} exists={}", draft_id, draft_path.display(), draft_path.is_dir());
    if !draft_path.is_dir() {
      return Err(anyhow::anyhow!("CAPCUT_DRAFT_NOT_FOUND: backend-owned draft path no longer exists: {}", draft_path.display()));
    }
    let local = post(client, "/local/tracks", &json!({ "project": draft_path })).await?;
    let local_tracks = local.get("tracks").and_then(Value::as_array).ok_or_else(|| anyhow::anyhow!("DRAFT_INSPECT_FAILED: local tracks response is missing tracks"))?;
    let (mut v, mut a, mut c) = (0u64, 0u64, 0u64);
    for track in local_tracks {
      if track.get("segment_count").and_then(Value::as_u64).unwrap_or(0) == 0 {
        continue;
      }
      match track.get("type").and_then(Value::as_str).unwrap_or("") {
        "video" | "image" | "visual" => v += 1,
        "audio" | "voice" | "music" => a += 1,
        "text" | "caption" | "subtitle" => c += 1,
        _ => {},
      }
    }
    visual = Some(v);
    audio = Some(a);
    caption = Some(c);
    source = "capcut_local_tracks".to_string();
  }

  let timeline_duration_us = root.get("duration").or_else(|| root.get("timeline_duration_us")).and_then(|v| v.as_u64());

  Ok(DraftManifest { draft_id: draft_id.to_string(), visual_track_count: visual, audio_track_count: audio, caption_track_count: caption, timeline_duration_us, source })
}

/// Kick off video rendering task if supported.
pub async fn gen_video(client: &Client, draft_url: &str) -> AnyhowResult<()> {
  let body = json!({ "draft_url": draft_url });
  post(client, "/gen_video", &body).await?;
  Ok(())
}

/// Poll video rendering status with cancellation check and strict deadline.
pub async fn poll_gen_video_status(client: &Client, draft_url: &str, cancel_flag: Option<Arc<AtomicBool>>) -> AnyhowResult<String> {
  let deadline = std::time::Instant::now() + get_job_timeout();
  let poll_interval = get_poll_interval();
  let body = json!({ "draft_url": draft_url });

  loop {
    // Check cancellation requested
    if let Some(ref flag) = cancel_flag {
      if flag.load(Ordering::Relaxed) {
        info!("[CAPCUT][RENDER_CANCEL] Cancellation requested during render polling");
        return Err(anyhow::anyhow!("RENDER_CANCELLED: User requested job cancellation"));
      }
    }

    let response = match post(client, "/gen_video_status", &body).await {
      Ok(res) => res,
      Err(err) => {
        warn!("[CAPCUT][POLL_ERR] Error querying gen_video_status: {err}");
        if std::time::Instant::now() >= deadline {
          return Err(anyhow::anyhow!("RENDER_TIMEOUT: Polling failed repeatedly and deadline exceeded: {err}"));
        }
        tokio::time::sleep(poll_interval).await;
        continue;
      },
    };

    let status = response.get("status").and_then(|v| v.as_str()).unwrap_or("");

    match status {
      "success" | "completed" | "done" => {
        let video_url = response.get("video_url").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("RENDER_FAILED: gen_video_status completed but missing video_url"))?;
        return Ok(video_url.to_string());
      },
      "failed" | "error" => {
        let err_msg = response.get("error_message").and_then(|v| v.as_str()).unwrap_or("unknown render error");
        return Err(anyhow::anyhow!("RENDER_FAILED: CapCut render failed: {err_msg}"));
      },
      _ => {
        if std::time::Instant::now() >= deadline {
          return Err(anyhow::anyhow!("RENDER_TIMEOUT: Render did not complete within deadline"));
        }
        tokio::time::sleep(poll_interval).await;
      },
    }
  }
}

/// Materialize a successful render into the workflow artifact directory before
/// it can be registered as `rendered_video`.
pub async fn materialize_rendered_video(client: &Client, video_url: &str, destination: &Path) -> AnyhowResult<()> {
  if let Some(parent) = destination.parent() {
    std::fs::create_dir_all(parent)?;
  }
  if video_url.starts_with("http://") || video_url.starts_with("https://") {
    let response = client.get(video_url).send().await.map_err(|error| anyhow::anyhow!("RENDER_FAILED: rendered video download failed: {error}"))?;
    if !response.status().is_success() {
      return Err(anyhow::anyhow!("RENDER_FAILED: rendered video download returned HTTP {}", response.status().as_u16()));
    }
    std::fs::write(destination, response.bytes().await?)?;
  } else {
    std::fs::copy(video_url, destination).map_err(|error| anyhow::anyhow!("RENDER_FAILED: rendered video path is unavailable: {error}"))?;
  }
  if !destination.is_file() || destination.metadata()?.len() == 0 {
    return Err(anyhow::anyhow!("RENDER_FAILED: rendered video artifact is empty"));
  }
  Ok(())
}

/// Send POST request to `{base_url}{CAPCUT_PREFIX}{path}` and validate `code == 0` convention.
fn classify_http_error(path: &str, status: u16, text: &str) -> anyhow::Error {
  if let Ok(body) = serde_json::from_str::<Value>(text) {
    let detail = body.get("detail").unwrap_or(&body);
    if let Some(code) = detail.get("code").and_then(Value::as_str).filter(|code| code.starts_with("CAPCUT_")) {
      let message = detail.get("message").and_then(Value::as_str).unwrap_or("CapCut operation failed");
      return anyhow::anyhow!("{code}: {message}");
    }
  }
  if path == "/local/tracks" && status == 404 {
    anyhow::anyhow!("CAPCUT_DRAFT_NOT_FOUND: CapCut Mate could not find the created draft for /local/tracks: {text}")
  } else {
    anyhow::anyhow!("CAPCUT_UNAVAILABLE: CapCut Mate HTTP error {status} for {path}: {text}")
  }
}

async fn post(client: &Client, path: &str, body: &Value) -> AnyhowResult<Value> {
  let base_url = get_capcut_mate_base_url();
  let url = format!("{}/openapi/capcut-mate/v1{}", base_url.trim_end_matches('/'), path);
  let body_string = serde_json::to_string(body)?;

  let response = client.post(&url).header("Content-Type", "application/json").header("Accept", "application/json").body(body_string).send().await.map_err(|e| anyhow::anyhow!("CAPCUT_UNAVAILABLE: Connection failed to {url}: {e}"))?;

  let status = response.status();
  let text = response.text().await?;

  if !status.is_success() {
    return Err(classify_http_error(path, status.as_u16(), &text));
  }

  let parsed: Value = serde_json::from_str(&text)?;

  // Some deployments wrap responses in `{code,message}`, while the existing
  // local FastAPI runtime returns its Pydantic response directly.
  if let Some(code) = parsed.get("code").and_then(|value| value.as_i64()) {
    if code != 0 {
      let message = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("unknown backend error");
      return Err(anyhow::anyhow!("CAPCUT_API_ERROR: Path {path} failed (code {code}): {message}"));
    }
  }

  Ok(parsed)
}

async fn post_gateway(client: &Client, path: &str, body: &Value) -> AnyhowResult<Value> {
  let base_url = get_capcut_mate_base_url();
  let url = format!("{}{}", base_url.trim_end_matches('/'), path);
  let response = client.post(&url).json(body).send().await.map_err(|error| anyhow::anyhow!("CAPCUT_UNAVAILABLE: Connection failed to {url}: {error}"))?;
  let status = response.status();
  let text = response.text().await?;
  if !status.is_success() {
    return Err(anyhow::anyhow!("CAPCUT_ASSET_TRANSPORT_FAILED: HTTP {}: {text}", status.as_u16()));
  }
  serde_json::from_str(&text).map_err(Into::into)
}

#[cfg(test)]
mod tests {
  use super::{classify_http_error, parse_created_draft, parse_published_draft, validate_created_draft, CreatedDraft};
  use serde_json::json;

  #[test]
  fn create_response_preserves_backend_owned_canonical_path() {
    let response = json!({
      "draft_url": "http://127.0.0.1:30000/openapi/capcut-mate/v1/get_draft?draft_id=draft-123",
      "draft_id": "draft-123",
      "draft_path": "D:\\runtime\\capcut-mate\\output\\draft\\draft-123"
    });

    let created = parse_created_draft(&response).unwrap();
    assert_eq!(created.draft_id, "draft-123");
    assert_eq!(created.draft_path.to_string_lossy(), "D:\\runtime\\capcut-mate\\output\\draft\\draft-123");
  }

  #[test]
  fn create_response_without_backend_path_is_rejected() {
    let response = json!({ "draft_url": "http://localhost/get_draft?draft_id=draft-123" });
    let error = parse_created_draft(&response).unwrap_err().to_string();
    assert!(error.contains("CAPCUT_DRAFT_CREATE_FAILED"));
  }

  #[test]
  fn created_draft_requires_directory_and_draft_json() {
    let root = tempfile::tempdir().unwrap();
    let draft_path = root.path().join("draft-123");
    std::fs::create_dir_all(&draft_path).unwrap();
    let created = CreatedDraft { draft_url: "url".into(), draft_id: "draft-123".into(), draft_path: draft_path.clone() };

    assert!(validate_created_draft(&created).unwrap_err().to_string().contains("CAPCUT_DRAFT_CREATE_FAILED"));
    std::fs::write(draft_path.join("draft_content.json"), b"{}").unwrap();
    std::fs::write(draft_path.join("draft_info.json"), b"{}").unwrap();
    validate_created_draft(&created).unwrap();
  }

  #[test]
  fn missing_local_tracks_draft_is_not_classified_as_backend_unavailable() {
    let error = classify_http_error("/local/tracks", 404, "draft missing").to_string();
    assert!(error.starts_with("CAPCUT_DRAFT_NOT_FOUND:"));
    assert!(!error.contains("CAPCUT_UNAVAILABLE"));
  }

  #[test]
  fn publish_response_uses_final_desktop_path() {
    let response = json!({
      "draft_id": "draft-123",
      "staging_path": "D:\\runtime\\output\\draft\\draft-123",
      "final_path": "C:\\Users\\user\\AppData\\Local\\CapCut\\User Data\\Projects\\com.lveditor.draft\\draft-123",
      "desktop_root": "C:\\Users\\user\\AppData\\Local\\CapCut\\User Data\\Projects\\com.lveditor.draft",
      "media": { "video": 1, "audio": 1, "captions": 2 }
    });

    let published = parse_published_draft(&response).unwrap();
    assert!(published.final_path.ends_with("com.lveditor.draft\\draft-123"));
    assert_ne!(published.final_path, published.staging_path);
  }
}
