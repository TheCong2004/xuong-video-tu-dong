use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_DONUT_BROWSER_API_URL: &str = "http://127.0.0.1:10108";

pub const GROK_IMAGE_EDIT_CAPABILITY: &str = "grok.image.edit";
pub const GROK_IMAGE_EXPAND_9_16_CAPABILITY: &str = "grok.image.expand_9_16";
pub const GROK_VIDEO_GENERATE_CAPABILITY: &str = "grok.video.generate";

pub fn get_donut_browser_api_base_url() -> String {
  std::env::var("DONUT_BROWSER_API_URL").or_else(|_| std::env::var("BROWSER_RUNTIME_URL")).unwrap_or_else(|_| DEFAULT_DONUT_BROWSER_API_URL.to_string()).trim_end_matches('/').to_string()
}

pub fn get_extension_bridge_base_url() -> String {
  std::env::var("EXTENSION_BRIDGE_URL").unwrap_or_else(|_| get_donut_browser_api_base_url()).trim_end_matches('/').to_string()
}

/// Safely constructs the canonical worker dispatch URL using the strict lease worker_id path segment.
pub fn build_worker_dispatch_url(base: &str, worker_id: &str) -> String {
  let clean_base = base.trim_end_matches('/');
  let clean_worker = worker_id.trim();
  format!("{clean_base}/v1/workers/{clean_worker}/dispatch")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LeaseStatus {
  Active,
  Released,
  Expired,
  Revoked,
}

impl LeaseStatus {
  pub fn is_active(&self) -> bool {
    matches!(self, LeaseStatus::Active)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquireWorkerRequest {
  pub job_id: String,
  pub step_id: String,
  pub attempt_id: String,
  pub capability: String,
  pub pool_id: Option<String>,
  pub profile_id: Option<String>,
  pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquireWorkerResponse {
  pub lease_id: String,
  pub worker_id: String,
  pub profile_id: String,
  pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatLeaseRequest {
  pub job_id: String,
  pub attempt_id: String,
  pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatLeaseResponse {
  pub lease_id: String,
  pub status: LeaseStatus,
  pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseLeaseResponse {
  pub lease_id: String,
  pub status: LeaseStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserWorkerInfo {
  pub worker_id: String,
  pub profile_id: String,
  pub pool_id: Option<String>,
  pub state: String,
  pub capabilities: Vec<String>,
  pub extension_ready: bool,
  pub extension_version: Option<String>,
  pub protocol_version: Option<u32>,
  pub grok_logged_in: Option<bool>,
  pub current_lease_id: Option<String>,
  pub current_job_id: Option<String>,
  pub last_heartbeat_at: Option<String>,
  pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWorkersResponse {
  pub workers: Vec<BrowserWorkerInfo>,
  pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunProfileRequest {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub headless: Option<bool>,
  pub cold_start_only: Option<bool>,
  pub browser_engine: Option<String>,
}

/// Send auto-launch request to Donut runtime to boot up the browser profile.
pub async fn launch_donut_profile(profile_id: &str, target_url: Option<&str>) -> Result<(), String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  // `/run` is also the navigation boundary for an existing profile. Do not
  // return early when it is already running: a blank tab still needs to be
  // navigated to the target site so the extension can report capabilities.

  let url = format!("{base_url}/v1/local/browser/profiles/{profile_id}/run");
  info!("[LocalBrowser] Requesting Donut Desktop to launch profile: {profile_id} (target_url={:?})", target_url);

  let req_body = RunProfileRequest {
    url: target_url.map(|u| u.to_string()),
    headless: Some(false),
    cold_start_only: Some(true),
    browser_engine: Some("chromium".to_string()),
  };

  let resp = client.post(&url).json(&req_body).send().await.map_err(|e| format!("Local browser launch request failed: {e}"))?;
  if resp.status().is_success() {
    info!("[LocalBrowser] Profile {profile_id} launched successfully");
    Ok(())
  } else {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    warn!("[LocalBrowser] Launch returned {status}: {body}");
    Err(format!("Local browser launch returned status {status}: {body}"))
  }
}

pub fn resolve_runtime_executable_candidates() -> Vec<std::path::PathBuf> {
  let mut candidates = Vec::new();

  // 1. Explicit Environment Variable
  if let Ok(env_path) = std::env::var("DONUT_RUNTIME_PATH").or_else(|_| std::env::var("FLOWORD_RUNTIME_PATH")) {
    if !env_path.trim().is_empty() {
      candidates.push(std::path::PathBuf::from(env_path.trim()));
    }
  }

  // 2. Relative to current executable
  if let Ok(current_exe) = std::env::current_exe() {
    if let Some(exe_dir) = current_exe.parent() {
      candidates.push(exe_dir.join("floword-donut-runtime.exe"));
      candidates.push(exe_dir.join("resources").join("donut-runtime").join("floword-donut-runtime.exe"));
      candidates.push(exe_dir.join("donut-runtime").join("floword-donut-runtime.exe"));
    }
  }

  // 3. Relative to current working directory
  candidates.push(std::path::PathBuf::from("resources/donut-runtime/floword-donut-runtime.exe"));
  candidates.push(std::path::PathBuf::from("crates/desktop/artcraft/resources/donut-runtime/floword-donut-runtime.exe"));

  candidates
}

/// Read-only readiness probe. Browser ownership belongs to Donut Desktop;
/// this compatibility helper must never spawn a runtime or browser.
pub async fn ensure_runtime_alive() {
  let base_url = get_donut_browser_api_base_url();
  let Ok(client) = Client::builder().timeout(Duration::from_millis(600)).build() else { return; };
  if let Ok(response) = client.get(format!("{base_url}/v1/runtime/health")).send().await {
    if response.status().is_success() {
      info!("[BrowserRuntime] Donut Desktop local manager is online");
    }
  }
}

fn resolve_shared_donut_data_dir() -> Option<std::path::PathBuf> {
  if let Some(path) = std::env::var_os("FLOWORD_DONUT_DATA_DIR") {
    if !path.is_empty() {
      return Some(path.into());
    }
  }

  #[cfg(debug_assertions)]
  {
    // `cargo tauri dev` runs the Donut Manager against the Dev catalog. Keep
    // the Floword client on that same catalog; otherwise the runtime can see
    // a different profile registry than the GUI (e.g. `aa` instead of `6fb…`).
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
      let local = std::path::PathBuf::from(local_app_data);
      let dev = local.join("DonutBrowserDev");
      if dev.join("profiles").is_dir() {
        return Some(dev);
      }
      return Some(local.join("DonutBrowser"));
    }
    return None;
  }

  #[cfg(not(debug_assertions))]
  None
}

/// Acquire an exclusive worker lease from Donut Desktop's local manager.
/// ArtCraft never starts a browser/runtime as a side effect of this call.
pub async fn acquire_worker(req: AcquireWorkerRequest) -> Result<AcquireWorkerResponse, String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder().timeout(Duration::from_secs(60)).build().map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/workers/acquire");
  info!("[BrowserRuntime] Acquiring worker: url={url} job_id={} capability={} profile_id={:?}", req.job_id, req.capability, req.profile_id);

  let resp = match client.post(&url).json(&req).send().await {
    Ok(r) => r,
    Err(e) => return Err(format!("BrowserRuntime unavailable: {e}")),
  };

  if resp.status().is_success() {
    let res = resp.json::<AcquireWorkerResponse>().await.map_err(|e| format!("Failed to parse AcquireWorkerResponse: {e}"))?;
    info!("[BrowserRuntime] Worker acquired: lease_id={} profile_id={}", res.lease_id, res.profile_id);
    return Ok(res);
  }

  let status = resp.status();
  let body = resp.text().await.unwrap_or_default();

  error!("[BrowserRuntime] Acquire failed ({status}): {body}");
  Err(format!("Acquire worker failed ({status}): {body}"))
}

/// Renew an active lease heartbeat.
pub async fn heartbeat_lease(lease_id: &str, req: HeartbeatLeaseRequest) -> Result<HeartbeatLeaseResponse, String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder().timeout(Duration::from_secs(5)).build().map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/workers/leases/{lease_id}/heartbeat");
  let resp = client.post(&url).json(&req).send().await.map_err(|e| format!("Heartbeat request failed: {e}"))?;

  if resp.status().is_success() {
    resp.json::<HeartbeatLeaseResponse>().await.map_err(|e| format!("Failed to parse HeartbeatLeaseResponse: {e}"))
  } else {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    warn!("[BrowserRuntime] Heartbeat failed ({status}): {body}");
    Err(format!("Heartbeat failed ({status}): {body}"))
  }
}

/// Idempotently release a worker lease.
pub async fn release_lease(lease_id: &str) -> Result<ReleaseLeaseResponse, String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder().timeout(Duration::from_secs(5)).build().map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/workers/leases/{lease_id}/release");
  info!("[BrowserRuntime] Releasing worker lease: lease_id={lease_id}");

  let resp = client.post(&url).send().await.map_err(|e| format!("Release request failed: {e}"))?;

  if resp.status().is_success() {
    resp.json::<ReleaseLeaseResponse>().await.map_err(|e| format!("Failed to parse ReleaseLeaseResponse: {e}"))
  } else {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    error!("[BrowserRuntime] Release failed ({status}): {body}");
    Err(format!("Release failed ({status}): {body}"))
  }
}

/// Query worker health and list all workers.
pub async fn list_workers() -> Result<ListWorkersResponse, String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder().timeout(Duration::from_secs(5)).build().map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/workers");
  let resp = client.get(&url).send().await.map_err(|e| format!("List workers failed: {e}"))?;

  if resp.status().is_success() {
    resp.json::<ListWorkersResponse>().await.map_err(|e| format!("Failed to parse ListWorkersResponse: {e}"))
  } else {
    let status = resp.status();
    Err(format!("List workers request returned status {status}"))
  }
}

// ---------------------------------------------------------------------------
// Donut Profile Catalog — persistent browser identities (exist even when off)
// ---------------------------------------------------------------------------

/// Persistent Donut browser profile (identity, cookies, fingerprint).
/// These exist even when the browser process is not running.
/// Runtime state (worker, extension, grok session) must be joined separately
/// via `list_workers()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DonutProfileInfo {
  pub id: String,
  pub name: String,
  pub browser: String,
  pub is_running: bool,
  pub process_id: Option<u32>,
  pub tags: Vec<String>,
  pub group_id: Option<String>,
  pub last_launch: Option<u64>,
  pub proxy_id: Option<String>,
  pub vpn_id: Option<String>,
  pub sync_mode: String,
  pub cloud_sync_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDonutProfilesResponse {
  pub profiles: Vec<DonutProfileInfo>,
  pub total: usize,
}

/// Fetch the full Donut Profile Catalog from `GET /v1/profiles`.
/// Returns all profiles regardless of running state — profiles that are
/// currently offline will have `is_running = false` and `process_id = None`.
pub async fn list_donut_profiles() -> Result<ListDonutProfilesResponse, String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder().timeout(Duration::from_secs(8)).build().map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/local/browser/profiles");
  info!("[DonutProfiles] Fetching profile catalog: {url}");

  let resp = client.get(&url).send().await.map_err(|e| format!("List profiles failed: {e}"))?;

  if resp.status().is_success() {
    resp.json::<ListDonutProfilesResponse>().await.map_err(|e| format!("Failed to parse ListDonutProfilesResponse: {e}"))
  } else {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    warn!("[LocalBrowser] GET /v1/local/browser/profiles returned {status}: {body}");
    Err(format!("List profiles request returned status {status}"))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_build_worker_dispatch_url_with_prefixed_worker_id() {
    let base = "http://127.0.0.1:10108";
    let worker_id = "browser-profile:1";
    let url = build_worker_dispatch_url(base, worker_id);
    assert_eq!(url, "http://127.0.0.1:10108/v1/workers/browser-profile:1/dispatch");
  }

  #[test]
  fn test_build_worker_dispatch_url_trim_trailing_slash() {
    let base = "http://127.0.0.1:10108/";
    let worker_id = "browser-profile:abc_123";
    let url = build_worker_dispatch_url(base, worker_id);
    assert_eq!(url, "http://127.0.0.1:10108/v1/workers/browser-profile:abc_123/dispatch");
  }

  #[test]
  fn test_lease_status_deserialization_screaming_snake_case() {
    let json_active = r#"{"lease_id":"L_1","status":"ACTIVE","expires_at":"2026-08-22T12:00:00Z"}"#;
    let resp: HeartbeatLeaseResponse = serde_json::from_str(json_active).expect("should deserialize ACTIVE");
    assert_eq!(resp.status, LeaseStatus::Active);
    assert!(resp.status.is_active());

    let json_released = r#"{"lease_id":"L_1","status":"RELEASED","expires_at":"2026-08-22T12:00:00Z"}"#;
    let resp: HeartbeatLeaseResponse = serde_json::from_str(json_released).expect("should deserialize RELEASED");
    assert_eq!(resp.status, LeaseStatus::Released);
    assert!(!resp.status.is_active());

    let json_expired = r#"{"lease_id":"L_1","status":"EXPIRED","expires_at":"2026-08-22T12:00:00Z"}"#;
    let resp: HeartbeatLeaseResponse = serde_json::from_str(json_expired).expect("should deserialize EXPIRED");
    assert_eq!(resp.status, LeaseStatus::Expired);

    let json_revoked = r#"{"lease_id":"L_1","status":"REVOKED","expires_at":"2026-08-22T12:00:00Z"}"#;
    let resp: HeartbeatLeaseResponse = serde_json::from_str(json_revoked).expect("should deserialize REVOKED");
    assert_eq!(resp.status, LeaseStatus::Revoked);
  }
}
