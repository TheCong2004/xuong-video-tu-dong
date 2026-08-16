//! Headless Youwee acquisition used by the canonical ArtCraft pipeline worker.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;
use tokio::process::Command;

use super::{build_cookie_args, build_proxy_args, get_ffmpeg_path, get_ytdlp_path};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Default)]
pub struct PipelineDownloadConfig {
  pub cookie_mode: Option<String>,
  pub cookie_browser: Option<String>,
  pub cookie_browser_profile: Option<String>,
  pub cookie_file_path: Option<String>,
  pub cookie_skip_patterns: Vec<String>,
  pub proxy_url: Option<String>,
}

impl PipelineDownloadConfig {
  fn has_auth(&self) -> bool {
    matches!(self.cookie_mode.as_deref(), Some("browser")) && self.cookie_browser.as_ref().is_some_and(|value| !value.trim().is_empty()) || matches!(self.cookie_mode.as_deref(), Some("file")) && self.cookie_file_path.as_ref().is_some_and(|value| !value.trim().is_empty())
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct YouweeDownloadResult {
  pub source_path: PathBuf,
  pub source_url: String,
  pub title: Option<String>,
  pub duration_seconds: Option<u64>,
  pub extractor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PipelineDownloadError {
  pub code: &'static str,
  pub message: String,
  pub retryable: bool,
  pub stderr_summary: Option<String>,
  pub service: &'static str,
  pub provider: Option<&'static str>,
  pub cancelled: bool,
}

impl PipelineDownloadError {
  fn new(code: &'static str, message: impl Into<String>, retryable: bool, provider: Option<&'static str>) -> Self {
    Self { code, message: message.into(), retryable, stderr_summary: None, service: "youwee", provider, cancelled: false }
  }

  fn with_stderr(mut self, stderr: &str) -> Self {
    self.stderr_summary = stderr_summary(stderr);
    self
  }

  fn cancelled() -> Self {
    Self { code: "INGEST_CANCELLED", message: "User requested job cancellation".to_string(), retryable: false, stderr_summary: None, service: "youwee", provider: None, cancelled: true }
  }
}

impl std::fmt::Display for PipelineDownloadError {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(formatter, "{}: {}", self.code, self.message)
  }
}

impl std::error::Error for PipelineDownloadError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PipelineSourceKind {
  TikTokDirectVideo,
  TikTokOther,
  Other,
}

/// Preserve the existing Youwee resolver and default unauthenticated behavior.
pub async fn download_pipeline_source(app: &AppHandle, url: &str, output_path: &Path, cancel_flag: Arc<AtomicBool>) -> Result<YouweeDownloadResult, PipelineDownloadError> {
  download_pipeline_source_with_config(app, url, output_path, cancel_flag, &PipelineDownloadConfig::default()).await
}

/// Download one URL to a validated canonical file. Authentication is attempted
/// only after a normal TikTok extractor/auth failure and only when configured.
pub async fn download_pipeline_source_with_config(app: &AppHandle, url: &str, output_path: &Path, cancel_flag: Arc<AtomicBool>, config: &PipelineDownloadConfig) -> Result<YouweeDownloadResult, PipelineDownloadError> {
  let url = url.trim();
  let source_kind = classify_pipeline_source(url)?;
  check_cancelled(&cancel_flag)?;

  let parent = output_path.parent().ok_or_else(|| PipelineDownloadError::new("UNSUPPORTED_SOURCE_URL", "Output path has no parent", false, provider_for(source_kind)))?;
  std::fs::create_dir_all(parent).map_err(|error| PipelineDownloadError::new("YOUWEE_STORAGE_FAILED", format!("Cannot create output directory: {error}"), false, provider_for(source_kind)))?;
  let (binary, _) = get_ytdlp_path(app).await.ok_or_else(|| PipelineDownloadError::new("YTDLP_NOT_FOUND", "Youwee yt-dlp executable is unavailable", false, provider_for(source_kind)))?;
  let temp_dir = parent.join(".youwee-download");
  cleanup_temp_dir(&temp_dir);
  std::fs::create_dir_all(&temp_dir).map_err(|error| PipelineDownloadError::new("YOUWEE_STORAGE_FAILED", format!("Cannot create temporary download directory: {error}"), false, provider_for(source_kind)))?;
  let temp_output = temp_dir.join("source_video.mp4");

  let normal_result = run_download_attempt(app, &binary, url, &temp_output, &temp_dir, config, false, &cancel_flag, source_kind).await;
  let result = match normal_result {
    Ok(()) => Ok(()),
    Err(error) if config.has_auth() && should_retry_with_auth(&error) => {
      cleanup_temp_dir(&temp_dir);
      std::fs::create_dir_all(&temp_dir).map_err(|storage| PipelineDownloadError::new("YOUWEE_STORAGE_FAILED", format!("Cannot recreate temporary download directory: {storage}"), false, provider_for(source_kind)))?;
      run_download_attempt(app, &binary, url, &temp_output, &temp_dir, config, true, &cancel_flag, source_kind).await
    },
    Err(error) => Err(error),
  };

  if let Err(error) = result {
    cleanup_temp_dir(&temp_dir);
    return Err(error);
  }
  check_cancelled(&cancel_flag).inspect_err(|_| cleanup_temp_dir(&temp_dir))?;
  validate_downloaded_file(&temp_output, source_kind).inspect_err(|_| cleanup_temp_dir(&temp_dir))?;

  if output_path.exists() {
    if let Err(error) = std::fs::remove_file(output_path) {
      cleanup_temp_dir(&temp_dir);
      return Err(PipelineDownloadError::new("YOUWEE_STORAGE_FAILED", format!("Cannot replace prior source video: {error}"), false, provider_for(source_kind)));
    }
  }
  if let Err(error) = std::fs::rename(&temp_output, output_path) {
    cleanup_temp_dir(&temp_dir);
    return Err(PipelineDownloadError::new("YOUWEE_STORAGE_FAILED", format!("Cannot commit downloaded source video: {error}"), false, provider_for(source_kind)));
  }
  cleanup_temp_dir(&temp_dir);
  validate_downloaded_file(output_path, source_kind)?;

  Ok(YouweeDownloadResult { source_path: output_path.to_path_buf(), source_url: url.to_string(), title: None, duration_seconds: None, extractor: provider_for(source_kind).map(str::to_string) })
}

async fn run_download_attempt(app: &AppHandle, binary: &Path, url: &str, temp_output: &Path, cwd: &Path, config: &PipelineDownloadConfig, authenticated: bool, cancel_flag: &AtomicBool, source_kind: PipelineSourceKind) -> Result<(), PipelineDownloadError> {
  check_cancelled(cancel_flag)?;
  let mut args = vec!["--no-playlist".to_string(), "--no-progress".to_string(), "--force-overwrites".to_string(), "--format".to_string(), "bv*+ba/b".to_string(), "--merge-output-format".to_string(), "mp4".to_string(), "--output".to_string(), temp_output.to_string_lossy().to_string(), "--print".to_string(), "after_move:filepath".to_string()];
  if let Some(ffmpeg) = get_ffmpeg_path(app).await {
    if let Some(directory) = ffmpeg.parent() {
      args.push("--ffmpeg-location".to_string());
      args.push(directory.to_string_lossy().to_string());
    }
  }
  if authenticated {
    args.extend(build_cookie_args(url, config.cookie_mode.as_deref(), config.cookie_browser.as_deref(), config.cookie_browser_profile.as_deref(), config.cookie_file_path.as_deref(), Some(&config.cookie_skip_patterns)));
  }
  args.extend(build_proxy_args(config.proxy_url.as_deref()));
  args.push("--".to_string());
  args.push(url.to_string());

  let mut command = Command::new(binary);
  command.kill_on_drop(true).current_dir(cwd).args(&args);
  let output = tokio::select! {
    result = tokio::time::timeout(DOWNLOAD_TIMEOUT, command.output()) => match result {
      Ok(Ok(output)) => output,
      Ok(Err(error)) => return Err(PipelineDownloadError::new("YOUWEE_DOWNLOAD_FAILED", format!("Cannot start yt-dlp: {error}"), true, provider_for(source_kind))),
      Err(_) => return Err(PipelineDownloadError::new("YTDLP_TIMEOUT", "yt-dlp exceeded the download timeout", true, provider_for(source_kind))),
    },
    _ = wait_for_cancellation(cancel_flag) => return Err(PipelineDownloadError::cancelled()),
  };
  if !output.status.success() {
    return Err(classify_ytdlp_failure(&String::from_utf8_lossy(&output.stderr)));
  }
  Ok(())
}

fn classify_pipeline_source(url: &str) -> Result<PipelineSourceKind, PipelineDownloadError> {
  let parsed = reqwest::Url::parse(url).map_err(|_| PipelineDownloadError::new("UNSUPPORTED_SOURCE_URL", "Source URL must be a valid HTTP or HTTPS URL", false, None))?;
  if !matches!(parsed.scheme(), "http" | "https") {
    return Err(PipelineDownloadError::new("UNSUPPORTED_SOURCE_URL", "Source URL must use HTTP or HTTPS", false, None));
  }
  let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
  if host == "tiktok.com" || host.ends_with(".tiktok.com") {
    let segments = parsed.path_segments().map(|items| items.filter(|item| !item.is_empty()).collect::<Vec<_>>()).unwrap_or_default();
    if segments.len() == 1 && segments[0].starts_with('@') {
      return Err(PipelineDownloadError::new("TIKTOK_PROFILE_UNSUPPORTED", "Direct public TikTok video URL required.", false, Some("tiktok")));
    }
    if segments.len() >= 3 && segments[0].starts_with('@') && segments[1] == "video" && segments[2].chars().all(|character| character.is_ascii_digit()) {
      return Ok(PipelineSourceKind::TikTokDirectVideo);
    }
    return Ok(PipelineSourceKind::TikTokOther);
  }
  Ok(PipelineSourceKind::Other)
}

fn classify_ytdlp_failure(stderr: &str) -> PipelineDownloadError {
  let normalized = stderr.to_ascii_lowercase();
  let provider = if normalized.contains("tiktok") { Some("tiktok") } else { None };
  let error = if normalized.contains("[tiktok:user]") || normalized.contains("extract secondary user id") {
    PipelineDownloadError::new("TIKTOK_PROFILE_UNSUPPORTED", "Direct public TikTok video URL required.", false, Some("tiktok"))
  } else if normalized.contains("private") || normalized.contains("embedding disabled") || normalized.contains("login required") || normalized.contains("sign in") || normalized.contains("cookies required") || normalized.contains("fresh cookies") {
    PipelineDownloadError::new("TIKTOK_AUTH_REQUIRED", "TikTok video requires configured authentication cookies or is not public.", false, Some("tiktok"))
  } else if normalized.contains("universal data for rehydration") {
    PipelineDownloadError::new("TIKTOK_EXTRACTOR_FAILED", "TikTok video could not be extracted. Try another public video, configure TikTok cookies, or use a local MP4.", true, Some("tiktok"))
  } else if normalized.contains("unexpected response from webpage request") {
    PipelineDownloadError::new("TIKTOK_RESPONSE_UNEXPECTED", "TikTok returned an unexpected webpage response. Retry or use authenticated cookies.", true, Some("tiktok"))
  } else if normalized.contains("unsupported url") {
    PipelineDownloadError::new("UNSUPPORTED_SOURCE_URL", "This source URL is not supported by Youwee.", false, provider)
  } else {
    PipelineDownloadError::new("YOUWEE_DOWNLOAD_FAILED", "Youwee could not download the source video.", true, provider)
  };
  error.with_stderr(stderr)
}

fn provider_for(kind: PipelineSourceKind) -> Option<&'static str> {
  match kind {
    PipelineSourceKind::TikTokDirectVideo | PipelineSourceKind::TikTokOther => Some("tiktok"),
    PipelineSourceKind::Other => None,
  }
}

fn should_retry_with_auth(error: &PipelineDownloadError) -> bool {
  matches!(error.code, "TIKTOK_AUTH_REQUIRED" | "TIKTOK_EXTRACTOR_FAILED" | "TIKTOK_RESPONSE_UNEXPECTED")
}

fn validate_downloaded_file(path: &Path, source_kind: PipelineSourceKind) -> Result<(), PipelineDownloadError> {
  let metadata = std::fs::metadata(path).map_err(|error| PipelineDownloadError::new("YOUWEE_INVALID_OUTPUT", format!("Downloaded source video is unavailable: {error}"), false, provider_for(source_kind)))?;
  if !metadata.is_file() || metadata.len() == 0 {
    return Err(PipelineDownloadError::new("YOUWEE_INVALID_OUTPUT", "Downloaded source video is empty or invalid", false, provider_for(source_kind)));
  }
  Ok(())
}

fn cleanup_temp_dir(temp_dir: &Path) {
  if temp_dir.is_dir() {
    let _ = std::fs::remove_dir_all(temp_dir);
  }
}

fn stderr_summary(stderr: &str) -> Option<String> {
  let lines = stderr.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>();
  if lines.is_empty() {
    return None;
  }
  let start = lines.len().saturating_sub(8);
  let mut summary = lines[start..].join(" ").replace(['\r', '\n'], " ");
  if summary.chars().count() > 1200 {
    summary = summary.chars().take(1197).collect::<String>() + "...";
  }
  Some(summary)
}

fn check_cancelled(cancel_flag: &AtomicBool) -> Result<(), PipelineDownloadError> {
  if cancel_flag.load(Ordering::SeqCst) {
    Err(PipelineDownloadError::cancelled())
  } else {
    Ok(())
  }
}

async fn wait_for_cancellation(cancel_flag: &AtomicBool) {
  let mut interval = tokio::time::interval(Duration::from_millis(100));
  loop {
    interval.tick().await;
    if cancel_flag.load(Ordering::SeqCst) {
      return;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn recognizes_direct_tiktok_video_url() {
    assert_eq!(classify_pipeline_source("https://www.tiktok.com/@creator/video/7671239543722478869").unwrap(), PipelineSourceKind::TikTokDirectVideo);
  }

  #[test]
  fn rejects_tiktok_profile_before_spawn() {
    let error = classify_pipeline_source("https://www.tiktok.com/@creator").unwrap_err();
    assert_eq!(error.code, "TIKTOK_PROFILE_UNSUPPORTED");
    assert!(!error.retryable);
  }

  #[test]
  fn leaves_non_tiktok_sources_supported() {
    assert_eq!(classify_pipeline_source("https://www.youtube.com/watch?v=abc").unwrap(), PipelineSourceKind::Other);
  }

  #[test]
  fn classifies_tiktok_extractor_and_auth_failures() {
    let extractor = classify_ytdlp_failure("ERROR: [TikTok] Unable to extract universal data for rehydration");
    assert_eq!(extractor.code, "TIKTOK_EXTRACTOR_FAILED");
    assert!(extractor.retryable);

    let auth = classify_ytdlp_failure("ERROR: [TikTok] This video is private; cookies are required");
    assert_eq!(auth.code, "TIKTOK_AUTH_REQUIRED");
    assert!(!auth.retryable);
  }

  #[test]
  fn classifies_unexpected_response_as_retryable_runtime_failure() {
    let error = classify_ytdlp_failure("ERROR: [TikTok] Unexpected response from webpage request");
    assert_eq!(error.code, "TIKTOK_RESPONSE_UNEXPECTED");
    assert!(error.retryable);
  }

  #[test]
  fn invalid_or_partial_output_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("empty.mp4");
    std::fs::write(&output, []).unwrap();
    assert_eq!(validate_downloaded_file(&output, PipelineSourceKind::TikTokDirectVideo).unwrap_err().code, "YOUWEE_INVALID_OUTPUT");
  }
}
