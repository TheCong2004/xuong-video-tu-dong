use crate::types::{DependencySource, FfmpegStatus};
use crate::utils::{find_system_binary, unix_system_binary_dirs, CommandExt};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::process::Command;

const SOURCE_CONFIG_FILE: &str = "ffmpeg-source.txt";
const RELEASE_VERSION_FILE: &str = "ffmpeg-release-version.txt";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegRuntimeSource {
  Explicit,
  AppData,
  BundledResource,
  RuntimeTools,
  SystemPath,
}

impl fmt::Display for FfmpegRuntimeSource {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let value = match self {
      Self::Explicit => "explicit",
      Self::AppData => "app-data",
      Self::BundledResource => "bundled-resource",
      Self::RuntimeTools => "runtime-tools",
      Self::SystemPath => "PATH",
    };
    formatter.write_str(value)
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FfmpegRuntime {
  pub ffmpeg_path: PathBuf,
  pub ffprobe_path: PathBuf,
  pub source: FfmpegRuntimeSource,
  pub ffmpeg_version: String,
  pub ffprobe_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FfmpegRuntimeError {
  code: &'static str,
  message: String,
}

impl FfmpegRuntimeError {
  fn new(code: &'static str, message: impl Into<String>) -> Self {
    Self { code, message: message.into() }
  }

  pub fn code(&self) -> &'static str {
    self.code
  }
}

impl fmt::Display for FfmpegRuntimeError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}: {}", self.code, self.message)
  }
}

impl std::error::Error for FfmpegRuntimeError {}

fn binary_names() -> (&'static str, &'static str) {
  if cfg!(windows) {
    ("ffmpeg.exe", "ffprobe.exe")
  } else {
    ("ffmpeg", "ffprobe")
  }
}

fn resolve_ffmpeg_pair_in_directories(candidates: &[(FfmpegRuntimeSource, PathBuf)]) -> Result<FfmpegRuntime, FfmpegRuntimeError> {
  let (ffmpeg_name, ffprobe_name) = binary_names();
  let mut found_ffmpeg = false;
  for (source, directory) in candidates {
    let ffmpeg_path = directory.join(ffmpeg_name);
    let ffprobe_path = directory.join(ffprobe_name);
    found_ffmpeg |= ffmpeg_path.is_file();
    if ffmpeg_path.is_file() && ffprobe_path.is_file() {
      return Ok(FfmpegRuntime { ffmpeg_path, ffprobe_path, source: *source, ffmpeg_version: String::new(), ffprobe_version: String::new() });
    }
  }
  if found_ffmpeg {
    Err(FfmpegRuntimeError::new("FFPROBE_NOT_FOUND", "FFmpeg was found, but its required sibling ffprobe binary was not found"))
  } else {
    Err(FfmpegRuntimeError::new("FFMPEG_NOT_FOUND", "FFmpeg was not found in configured, app-data, bundled, runtime-tools, or PATH locations"))
  }
}

fn configured_binary(keys: &[&str]) -> Option<PathBuf> {
  keys.iter().find_map(|key| std::env::var_os(key).filter(|value| !value.is_empty()).map(PathBuf::from))
}

fn explicit_ffmpeg_pair() -> Result<Option<FfmpegRuntime>, FfmpegRuntimeError> {
  let (ffmpeg_name, ffprobe_name) = binary_names();
  let ffmpeg = configured_binary(&["FFMPEG_PATH", "VYNARO_FFMPEG_PATH", "YOUWEE_FFMPEG_PATH"]);
  let ffprobe = configured_binary(&["FFPROBE_PATH", "VYNARO_FFPROBE_PATH", "YOUWEE_FFPROBE_PATH"]);
  if ffmpeg.is_none() && ffprobe.is_none() {
    return Ok(None);
  }
  let ffmpeg_path = ffmpeg.or_else(|| ffprobe.as_ref().and_then(|path| path.parent()).map(|parent| parent.join(ffmpeg_name))).ok_or_else(|| FfmpegRuntimeError::new("FFMPEG_NOT_FOUND", "Configured FFmpeg path could not be resolved"))?;
  if !ffmpeg_path.is_file() {
    return Err(FfmpegRuntimeError::new("FFMPEG_NOT_FOUND", format!("Configured FFmpeg binary does not exist: {}", ffmpeg_path.display())));
  }
  let ffprobe_path = ffprobe.or_else(|| ffmpeg_path.parent().map(|parent| parent.join(ffprobe_name))).ok_or_else(|| FfmpegRuntimeError::new("FFPROBE_NOT_FOUND", "Configured ffprobe path could not be resolved"))?;
  if !ffprobe_path.is_file() {
    return Err(FfmpegRuntimeError::new("FFPROBE_NOT_FOUND", format!("Configured ffprobe binary does not exist: {}", ffprobe_path.display())));
  }
  Ok(Some(FfmpegRuntime { ffmpeg_path, ffprobe_path, source: FfmpegRuntimeSource::Explicit, ffmpeg_version: String::new(), ffprobe_version: String::new() }))
}

async fn binary_version(path: &Path, label: &str) -> Result<String, FfmpegRuntimeError> {
  let mut command = Command::new(path);
  command.arg("-version").stdout(Stdio::piped()).stderr(Stdio::piped());
  command.hide_window();
  let output = tokio::time::timeout(std::time::Duration::from_secs(10), command.output()).await.map_err(|_| FfmpegRuntimeError::new("FFMPEG_TIMEOUT", format!("{label} -version timed out")))?.map_err(|error| FfmpegRuntimeError::new("FFMPEG_PROCESS_FAILED", format!("Failed to execute {label}: {error}")))?;
  if !output.status.success() {
    return Err(FfmpegRuntimeError::new("FFMPEG_PROCESS_FAILED", format!("{label} -version exited with status {}", output.status)));
  }
  Ok(String::from_utf8_lossy(&output.stdout).lines().next().unwrap_or("unknown").trim().to_string())
}

/// Resolve one verified FFmpeg/ffprobe pair for all ArtCraft runtime stages.
pub async fn resolve_ffmpeg_runtime(app: &AppHandle) -> Result<FfmpegRuntime, FfmpegRuntimeError> {
  let mut resolved = if let Some(explicit) = explicit_ffmpeg_pair()? {
    explicit
  } else {
    let mut candidates = Vec::new();
    if let Ok(app_data) = app.path().app_data_dir() {
      candidates.push((FfmpegRuntimeSource::AppData, app_data.join("bin")));
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
      candidates.push((FfmpegRuntimeSource::BundledResource, resource_dir.join("ffmpeg").join("bin")));
      candidates.push((FfmpegRuntimeSource::BundledResource, resource_dir.join("resources").join("ffmpeg").join("bin")));
    }
    if let Some(project_root) = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent() {
      candidates.push((FfmpegRuntimeSource::RuntimeTools, project_root.join(".runtime").join("ffmpeg").join("bin")));
      candidates.push((FfmpegRuntimeSource::RuntimeTools, project_root.join("tools").join("ffmpeg").join("bin")));
    }
    match resolve_ffmpeg_pair_in_directories(&candidates) {
      Ok(runtime) => runtime,
      Err(directory_error) => {
        let (ffmpeg_name, ffprobe_name) = binary_names();
        let ffmpeg_path = get_system_ffmpeg_path().ok_or(directory_error)?;
        let ffprobe_path = find_system_binary(ffprobe_name, &unix_system_binary_dirs()).or_else(|| ffmpeg_path.parent().map(|parent| parent.join(ffprobe_name)).filter(|path| path.is_file())).ok_or_else(|| FfmpegRuntimeError::new("FFPROBE_NOT_FOUND", format!("{ffprobe_name} was not found next to {ffmpeg_name} or in PATH")))?;
        FfmpegRuntime { ffmpeg_path, ffprobe_path, source: FfmpegRuntimeSource::SystemPath, ffmpeg_version: String::new(), ffprobe_version: String::new() }
      },
    }
  };

  resolved.ffmpeg_version = binary_version(&resolved.ffmpeg_path, "ffmpeg").await?;
  resolved.ffprobe_version = binary_version(&resolved.ffprobe_path, "ffprobe").await?;
  println!("[FFMPEG RESOLVER] source={} ffmpeg={} ffprobe={}", resolved.source, resolved.ffmpeg_path.display(), resolved.ffprobe_path.display());
  println!("[FFMPEG RESOLVER] {} | {}", resolved.ffmpeg_version, resolved.ffprobe_version);
  Ok(resolved)
}

pub fn system_ffmpeg_upgrade_message() -> String {
  #[cfg(target_os = "macos")]
  {
    return "System FFmpeg is managed externally. Update it with Homebrew (`brew upgrade ffmpeg`) or switch source to App managed.".to_string();
  }
  #[cfg(target_os = "windows")]
  {
    return "System FFmpeg is managed externally. Update it with your package manager (e.g. `winget`, `choco`, or `scoop`) or switch source to App managed.".to_string();
  }
  #[cfg(target_os = "linux")]
  {
    return "System FFmpeg is managed externally. Update it with your distro package manager or switch source to App managed.".to_string();
  }
  #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
  {
    "System FFmpeg is managed externally. Update it with your package manager or switch source to App managed.".to_string()
  }
}

fn get_ffmpeg_source_config_path(app: &AppHandle) -> Option<PathBuf> {
  app.path().app_data_dir().ok().map(|p| p.join("bin").join(SOURCE_CONFIG_FILE))
}

fn get_ffmpeg_release_version_path(app: &AppHandle) -> Option<PathBuf> {
  app.path().app_data_dir().ok().map(|p| p.join("bin").join(RELEASE_VERSION_FILE))
}

pub async fn read_app_ffmpeg_release_version(app: &AppHandle) -> Option<String> {
  let version_path = get_ffmpeg_release_version_path(app)?;
  let content = tokio::fs::read_to_string(&version_path).await.ok()?;
  let version = content.trim();

  if version.is_empty() {
    None
  } else {
    Some(version.to_string())
  }
}

pub async fn write_app_ffmpeg_release_version(app: &AppHandle, version: &str) -> Result<(), String> {
  let version_path = get_ffmpeg_release_version_path(app).ok_or("Failed to get FFmpeg version path")?;

  if let Some(parent) = version_path.parent() {
    tokio::fs::create_dir_all(parent).await.map_err(|e| format!("Failed to create bin directory: {}", e))?;
  }

  tokio::fs::write(&version_path, version).await.map_err(|e| format!("Failed to save FFmpeg release version: {}", e))?;

  Ok(())
}

pub async fn get_ffmpeg_source(app: &AppHandle) -> DependencySource {
  if let Some(config_path) = get_ffmpeg_source_config_path(app) {
    if let Ok(content) = tokio::fs::read_to_string(&config_path).await {
      return DependencySource::from_str(content.trim());
    }
  }
  DependencySource::Auto
}

pub async fn set_ffmpeg_source(app: &AppHandle, source: &DependencySource) -> Result<(), String> {
  let config_path = get_ffmpeg_source_config_path(app).ok_or("Failed to get config path")?;

  if let Some(parent) = config_path.parent() {
    tokio::fs::create_dir_all(parent).await.map_err(|e| format!("Failed to create bin directory: {}", e))?;
  }

  tokio::fs::write(&config_path, source.as_str()).await.map_err(|e| format!("Failed to save source config: {}", e))?;

  Ok(())
}

fn get_app_ffmpeg_path(app: &AppHandle) -> Option<PathBuf> {
  let app_data_dir = app.path().app_data_dir().ok()?;
  let bin_dir = app_data_dir.join("bin");
  #[cfg(windows)]
  let ffmpeg_path = bin_dir.join("ffmpeg.exe");
  #[cfg(not(windows))]
  let ffmpeg_path = bin_dir.join("ffmpeg");

  if ffmpeg_path.exists() {
    Some(ffmpeg_path)
  } else {
    None
  }
}

fn get_system_ffmpeg_path() -> Option<PathBuf> {
  #[cfg(windows)]
  let binary_name = "ffmpeg.exe";
  #[cfg(not(windows))]
  let binary_name = "ffmpeg";

  find_system_binary(binary_name, &unix_system_binary_dirs())
}

/// Get the FFmpeg binary path (app data or system)
pub async fn get_ffmpeg_path(app: &AppHandle) -> Option<PathBuf> {
  match get_ffmpeg_source(app).await {
    DependencySource::System => get_system_ffmpeg_path(),
    DependencySource::App => get_app_ffmpeg_path(app),
    DependencySource::Auto => get_app_ffmpeg_path(app).or_else(get_system_ffmpeg_path),
  }
}

/// Check FFmpeg status
pub async fn check_ffmpeg_internal(app: &AppHandle) -> Result<FfmpegStatus, String> {
  if let Some(ffmpeg_path) = get_ffmpeg_path(app).await {
    let mut cmd = Command::new(&ffmpeg_path);
    cmd.args(["-version"]).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.hide_window();

    if let Ok(output) = cmd.output().await {
      if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let binary_version = parse_ffmpeg_version(&stdout);
        let app_path = get_app_ffmpeg_path(app);
        let is_system = app_path.as_ref().map(|p| p != &ffmpeg_path).unwrap_or(true);
        let version = if is_system { binary_version } else { read_app_ffmpeg_release_version(app).await.unwrap_or(binary_version) };

        return Ok(FfmpegStatus { installed: true, version: Some(version), binary_path: Some(ffmpeg_path.to_string_lossy().to_string()), is_system });
      }
    }
  }

  Ok(FfmpegStatus { installed: false, version: None, binary_path: None, is_system: false })
}

/// Parse FFmpeg version from output
pub fn parse_ffmpeg_version(output: &str) -> String {
  if let Some(line) = output.lines().next() {
    if let Some(version_part) = line.strip_prefix("ffmpeg version ") {
      return version_part.split_whitespace().next().unwrap_or("unknown").to_string();
    }
  }
  "unknown".to_string()
}

/// Extract date string (YYYY-MM-DD) from version string
/// Examples:
/// - "git-2026-01-25-1e1dde8" -> "2026-01-25"
/// - "2026-01-25" -> "2026-01-25"
fn extract_date_from_version(version: &str) -> Option<String> {
  // Look for YYYY-MM-DD pattern
  let re = regex::Regex::new(r"(\d{4})-(\d{2})-(\d{2})").ok()?;
  if let Some(caps) = re.captures(version) {
    Some(format!("{}-{}-{}", &caps[1], &caps[2], &caps[3]))
  } else {
    None
  }
}

fn ffmpeg_version_has_update(current_version: &str, latest_version: &str) -> bool {
  let current_normalized = current_version.replace('.', "-");
  let latest_normalized = latest_version.replace('.', "-");
  let current_date = extract_date_from_version(&current_normalized);
  let latest_date = extract_date_from_version(&latest_normalized);

  match (current_date, latest_date) {
    (Some(curr), Some(lat)) => lat > curr,
    _ => false,
  }
}

/// FFmpeg download info with checksum support
pub struct FfmpegDownloadInfo {
  pub url: &'static str,
  pub archive_type: &'static str,
  pub checksum_url: &'static str,
  pub checksum_filename: &'static str,
}

/// Get FFmpeg download URL for current platform
/// All platforms now support SHA256 checksum verification
pub fn get_ffmpeg_download_info() -> FfmpegDownloadInfo {
  #[cfg(target_os = "macos")]
  {
    #[cfg(target_arch = "aarch64")]
    {
      FfmpegDownloadInfo { url: "https://github.com/vanloctech/ffmpeg-macos/releases/latest/download/ffmpeg-macos-arm64.tar.gz", archive_type: "tar.gz", checksum_url: "https://github.com/vanloctech/ffmpeg-macos/releases/latest/download/ffmpeg-macos-arm64.tar.gz.sha256", checksum_filename: "ffmpeg-macos-arm64.tar.gz" }
    }
    #[cfg(target_arch = "x86_64")]
    {
      FfmpegDownloadInfo { url: "https://github.com/vanloctech/ffmpeg-macos/releases/latest/download/ffmpeg-macos-x64.tar.gz", archive_type: "tar.gz", checksum_url: "https://github.com/vanloctech/ffmpeg-macos/releases/latest/download/ffmpeg-macos-x64.tar.gz.sha256", checksum_filename: "ffmpeg-macos-x64.tar.gz" }
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
      FfmpegDownloadInfo { url: "https://github.com/vanloctech/ffmpeg-macos/releases/latest/download/ffmpeg-macos-arm64.tar.gz", archive_type: "tar.gz", checksum_url: "https://github.com/vanloctech/ffmpeg-macos/releases/latest/download/ffmpeg-macos-arm64.tar.gz.sha256", checksum_filename: "ffmpeg-macos-arm64.tar.gz" }
    }
  }
  #[cfg(target_os = "windows")]
  {
    FfmpegDownloadInfo { url: "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-gpl.zip", archive_type: "zip", checksum_url: "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/checksums.sha256", checksum_filename: "ffmpeg-master-latest-win64-gpl.zip" }
  }
  #[cfg(target_os = "linux")]
  {
    #[cfg(target_arch = "aarch64")]
    {
      FfmpegDownloadInfo { url: "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-linuxarm64-gpl.tar.xz", archive_type: "tar.xz", checksum_url: "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/checksums.sha256", checksum_filename: "ffmpeg-master-latest-linuxarm64-gpl.tar.xz" }
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
      FfmpegDownloadInfo { url: "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-linux64-gpl.tar.xz", archive_type: "tar.xz", checksum_url: "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/checksums.sha256", checksum_filename: "ffmpeg-master-latest-linux64-gpl.tar.xz" }
    }
  }
  #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
  {
    FfmpegDownloadInfo { url: "", archive_type: "", checksum_url: "", checksum_filename: "" }
  }
}

/// FFmpeg update info
#[derive(Debug, Clone, serde::Serialize)]
pub struct FfmpegUpdateInfo {
  pub has_update: bool,
  pub current_version: Option<String>,
  pub latest_version: Option<String>,
  pub release_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FfmpegReleaseInfo {
  pub version: String,
  pub html_url: Option<String>,
}

/// Get the GitHub API URL for checking latest release
fn get_ffmpeg_release_api_url() -> &'static str {
  #[cfg(target_os = "macos")]
  {
    "https://api.github.com/repos/vanloctech/ffmpeg-macos/releases/latest"
  }
  #[cfg(target_os = "windows")]
  {
    "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest"
  }
  #[cfg(target_os = "linux")]
  {
    "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest"
  }
  #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
  {
    ""
  }
}

pub fn normalize_ffmpeg_release_version(tag_name: &str) -> String {
  tag_name.trim().strip_prefix("ffmpeg-").unwrap_or(tag_name.trim()).trim_start_matches('v').to_string()
}

pub async fn get_latest_ffmpeg_release_info() -> Result<FfmpegReleaseInfo, String> {
  let api_url = get_ffmpeg_release_api_url();
  if api_url.is_empty() {
    return Err("Unsupported platform".to_string());
  }

  let client = reqwest::Client::builder().user_agent("Youwee/0.6.0").timeout(std::time::Duration::from_secs(30)).build().map_err(|e| format!("Failed to create HTTP client: {}", e))?;

  let response = client.get(api_url).send().await.map_err(|e| format!("Failed to fetch release info: {}", e))?;

  if !response.status().is_success() {
    return Err(format!("Failed to fetch release info: HTTP {}", response.status()));
  }

  let json: serde_json::Value = response.json().await.map_err(|e| format!("Failed to parse release info: {}", e))?;

  let tag_name = json["tag_name"].as_str().ok_or("No tag_name in release")?;

  Ok(FfmpegReleaseInfo { version: normalize_ffmpeg_release_version(tag_name), html_url: json["html_url"].as_str().map(|s| s.to_string()) })
}

/// Check if FFmpeg update is available
pub async fn check_ffmpeg_update_internal(app: &AppHandle) -> Result<FfmpegUpdateInfo, String> {
  // Get current installed version
  let current_status = check_ffmpeg_internal(app).await?;

  if !current_status.installed {
    return Ok(FfmpegUpdateInfo { has_update: false, current_version: None, latest_version: None, release_url: None });
  }

  let current_version = current_status.version.clone();

  // Only check updates for bundled FFmpeg (not system)
  if current_status.is_system {
    return Ok(FfmpegUpdateInfo { has_update: false, current_version, latest_version: None, release_url: Some("System FFmpeg - update via package manager".to_string()) });
  }

  let latest_release = get_latest_ffmpeg_release_info().await?;
  let latest_version = latest_release.version;

  // Compare versions by extracting date parts
  // Current version format: "git-2026-01-25-1e1dde8" -> extract "2026-01-25"
  // Latest version format: "2026.01.25" or "ffmpeg-2026.01.25" -> extract "2026.01.25"
  let has_update = if let Some(ref current) = current_version { ffmpeg_version_has_update(current, &latest_version) } else { false };

  Ok(FfmpegUpdateInfo { has_update, current_version, latest_version: Some(latest_version), release_url: latest_release.html_url })
}

#[cfg(test)]
mod tests {
  use super::{ffmpeg_version_has_update, normalize_ffmpeg_release_version, resolve_ffmpeg_pair_in_directories, FfmpegRuntimeSource};
  use tempfile::tempdir;

  #[test]
  fn normalizes_ffmpeg_macos_release_tags() {
    assert_eq!(normalize_ffmpeg_release_version("ffmpeg-2026.06.11"), "2026.06.11");
    assert_eq!(normalize_ffmpeg_release_version("v2026.06.11"), "2026.06.11");
  }

  #[test]
  fn compares_binary_git_versions_with_release_versions() {
    assert!(ffmpeg_version_has_update("git-2026-06-10-5f998e3", "2026.06.11"));
    assert!(!ffmpeg_version_has_update("2026.06.11", "2026.06.11"));
  }

  #[test]
  fn resolves_a_complete_ffmpeg_pair_from_one_canonical_directory() {
    let root = tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }), b"binary").unwrap();
    std::fs::write(bin.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" }), b"binary").unwrap();

    let resolved = resolve_ffmpeg_pair_in_directories(&[(FfmpegRuntimeSource::RuntimeTools, bin)]).unwrap();
    assert_eq!(resolved.source, FfmpegRuntimeSource::RuntimeTools);
    assert!(resolved.ffmpeg_path.is_file());
    assert!(resolved.ffprobe_path.is_file());
  }

  #[test]
  fn rejects_an_ffmpeg_directory_without_ffprobe() {
    let root = tempdir().unwrap();
    std::fs::write(root.path().join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }), b"binary").unwrap();

    let error = resolve_ffmpeg_pair_in_directories(&[(FfmpegRuntimeSource::RuntimeTools, root.path().to_path_buf())]).unwrap_err();
    assert_eq!(error.code(), "FFPROBE_NOT_FOUND");
  }
}
