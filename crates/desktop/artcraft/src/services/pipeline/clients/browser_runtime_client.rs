use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_DONUT_BROWSER_API_URL: &str = "http://127.0.0.1:10108";

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
}

/// Send auto-launch request to Donut runtime to boot up the browser profile.
pub async fn launch_donut_profile(profile_id: &str, target_url: Option<&str>) -> Result<(), String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/profiles/{profile_id}/run");
  info!("[DonutAutoLaunch] Auto-launching profile: {profile_id} (target_url={:?})", target_url);

  let req_body = RunProfileRequest {
    url: target_url.map(|u| u.to_string()),
    headless: Some(false),
  };

  let resp = client.post(&url).json(&req_body).send().await.map_err(|e| format!("Auto-launch profile request failed: {e}"))?;
  if resp.status().is_success() {
    info!("[DonutAutoLaunch] Profile {profile_id} launched successfully");
    Ok(())
  } else {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    warn!("[DonutAutoLaunch] Auto-launch returned {status}: {body}");
    Err(format!("Auto-launch returned status {status}: {body}"))
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

  // 4. Dev debug assertion fallbacks only
  #[cfg(debug_assertions)]
  {
    candidates.push(std::path::PathBuf::from(r"D:\capcutpolot\donutbrowser\src-tauri\target\debug\floword-donut-runtime.exe"));
    candidates.push(std::path::PathBuf::from(r"D:\capcutpolot\artcraft\resources\donut-runtime\floword-donut-runtime.exe"));
    candidates.push(std::path::PathBuf::from(r"D:\capcutpolot\artcraft\target\debug\resources\donut-runtime\floword-donut-runtime.exe"));
  }

  candidates
}

pub async fn ensure_runtime_alive() {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder().timeout(Duration::from_millis(600)).build().ok();
  if let Some(c) = client {
    if let Ok(res) = c.get(format!("{base_url}/v1/health")).send().await {
      if res.status().is_success() {
        return;
      }
    }
  }

  info!("[BrowserRuntime] Donut runtime is not responding on 10108; attempting auto-spawn...");
  let candidates = resolve_runtime_executable_candidates();

  for exe in &candidates {
    if exe.exists() {
      info!("[BrowserRuntime] Spawning runtime binary: {:?}", exe);
      #[cfg(windows)]
      {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let _ = std::process::Command::new(exe)
          .creation_flags(CREATE_NO_WINDOW)
          .spawn();
      }
      #[cfg(not(windows))]
      {
        let _ = std::process::Command::new(exe).spawn();
      }
      break;
    }
  }

  for _ in 0..10 {
    tokio::time::sleep(Duration::from_millis(400)).await;
    if let Some(c) = Client::builder().timeout(Duration::from_millis(400)).build().ok() {
      if let Ok(res) = c.get(format!("{base_url}/v1/health")).send().await {
        if res.status().is_success() {
          info!("[BrowserRuntime] Runtime online and responsive on 10108");
          break;
        }
      }
    }
  }
}

/// Acquire an exclusive worker lease from donutbrowser runtime.
/// Automatically boots up the target browser profile if it is currently offline/stopped.
pub async fn acquire_worker(req: AcquireWorkerRequest) -> Result<AcquireWorkerResponse, String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder().timeout(Duration::from_secs(10)).build().map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/workers/acquire");
  info!("[BrowserRuntime] Acquiring worker: url={url} job_id={} capability={} profile_id={:?}", req.job_id, req.capability, req.profile_id);

  ensure_runtime_alive().await;
  let resp = match client.post(&url).json(&req).send().await {
    Ok(r) => r,
    Err(_) => {
      ensure_runtime_alive().await;
      client.post(&url).json(&req).send().await.map_err(|e| format!("BrowserRuntime unavailable: {e}"))?
    }
  };

  if resp.status().is_success() {
    let res = resp.json::<AcquireWorkerResponse>().await.map_err(|e| format!("Failed to parse AcquireWorkerResponse: {e}"))?;
    info!("[BrowserRuntime] Worker acquired: lease_id={} profile_id={}", res.lease_id, res.profile_id);
    return Ok(res);
  }

  let status = resp.status();
  let body = resp.text().await.unwrap_or_default();

  // Auto-launch handling: If worker is offline or capability is unavailable and profile_id is specified
  let is_not_ready = status.as_u16() == 409 || status.as_u16() == 501 || status.as_u16() == 503 || body.contains("CAPABILITY_UNAVAILABLE") || body.contains("NO_AVAILABLE_WORKER") || body.contains("BRIDGE_DISCONNECTED") || body.contains("OFFLINE");

  if is_not_ready && req.profile_id.is_some() {
    let pid = req.profile_id.as_deref().unwrap();
    let target_url = if req.capability.starts_with("grok") {
      "https://grok.com/imagine"
    } else if req.capability.contains("facebook") {
      "https://www.facebook.com"
    } else if req.capability.contains("tiktok") {
      "https://www.tiktok.com"
    } else if req.capability.contains("youtube") {
      "https://studio.youtube.com"
    } else {
      "https://grok.com/imagine"
    };

    info!("[BrowserRuntime] Worker not ready for profile {pid}; triggering auto-launch with {target_url}...");
    let _ = launch_donut_profile(pid, Some(target_url)).await;

    // Poll for up to 15 seconds for extension to handshake
    for attempt in 1..=15 {
      tokio::time::sleep(Duration::from_secs(1)).await;
      info!("[BrowserRuntime] Polling worker ready after auto-launch (attempt {attempt}/15)...");
      if let Ok(retry_resp) = client.post(&url).json(&req).send().await {
        if retry_resp.status().is_success() {
          if let Ok(res) = retry_resp.json::<AcquireWorkerResponse>().await {
            info!("[BrowserRuntime] Worker acquired successfully after auto-launch: lease_id={}", res.lease_id);
            return Ok(res);
          }
        }
      }
    }
  }

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

  let url = format!("{base_url}/v1/profiles");
  info!("[DonutProfiles] Fetching profile catalog: {url}");

  let resp = client.get(&url).send().await.map_err(|e| format!("List profiles failed: {e}"))?;

  if resp.status().is_success() {
    resp.json::<ListDonutProfilesResponse>().await.map_err(|e| format!("Failed to parse ListDonutProfilesResponse: {e}"))
  } else {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    warn!("[DonutProfiles] GET /v1/profiles returned {status}: {body}");
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

