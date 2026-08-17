use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_DONUT_BROWSER_API_URL: &str = "http://127.0.0.1:10108";

pub fn get_donut_browser_api_base_url() -> String {
  std::env::var("DONUT_BROWSER_API_URL")
    .or_else(|_| std::env::var("BROWSER_RUNTIME_URL"))
    .unwrap_or_else(|_| DEFAULT_DONUT_BROWSER_API_URL.to_string())
    .trim_end_matches('/')
    .to_string()
}

pub fn get_extension_bridge_base_url() -> String {
  std::env::var("EXTENSION_BRIDGE_URL")
    .unwrap_or_else(|_| get_donut_browser_api_base_url())
    .trim_end_matches('/')
    .to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquireWorkerRequest {
  pub job_id: String,
  pub step_id: String,
  pub attempt_id: String,
  pub capability: String,
  pub pool_id: Option<String>,
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
  pub status: String,
  pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseLeaseResponse {
  pub lease_id: String,
  pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserWorkerInfo {
  pub worker_id: String,
  pub profile_id: String,
  pub state: String,
  pub capabilities: Vec<String>,
  pub extension_ready: bool,
  pub grok_logged_in: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWorkersResponse {
  pub workers: Vec<BrowserWorkerInfo>,
  pub total: usize,
}

/// Acquire an exclusive worker lease from donutbrowser runtime.
pub async fn acquire_worker(req: AcquireWorkerRequest) -> Result<AcquireWorkerResponse, String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder()
    .timeout(Duration::from_secs(10))
    .build()
    .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/workers/acquire");
  info!("[BrowserRuntime] Acquiring worker: url={url} job_id={} capability={}", req.job_id, req.capability);

  let resp = client
    .post(&url)
    .json(&req)
    .send()
    .await
    .map_err(|e| format!("BrowserRuntime unavailable: {e}"))?;

  if resp.status().is_success() {
    let res = resp
      .json::<AcquireWorkerResponse>()
      .await
      .map_err(|e| format!("Failed to parse AcquireWorkerResponse: {e}"))?;
    info!("[BrowserRuntime] Worker acquired: lease_id={} profile_id={}", res.lease_id, res.profile_id);
    Ok(res)
  } else {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    error!("[BrowserRuntime] Acquire failed ({status}): {body}");
    Err(format!("Acquire worker failed ({status}): {body}"))
  }
}

/// Renew an active lease heartbeat.
pub async fn heartbeat_lease(lease_id: &str, req: HeartbeatLeaseRequest) -> Result<HeartbeatLeaseResponse, String> {
  let base_url = get_donut_browser_api_base_url();
  let client = Client::builder()
    .timeout(Duration::from_secs(5))
    .build()
    .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/workers/leases/{lease_id}/heartbeat");
  let resp = client
    .post(&url)
    .json(&req)
    .send()
    .await
    .map_err(|e| format!("Heartbeat request failed: {e}"))?;

  if resp.status().is_success() {
    resp
      .json::<HeartbeatLeaseResponse>()
      .await
      .map_err(|e| format!("Failed to parse HeartbeatLeaseResponse: {e}"))
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
  let client = Client::builder()
    .timeout(Duration::from_secs(5))
    .build()
    .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/workers/leases/{lease_id}/release");
  info!("[BrowserRuntime] Releasing worker lease: lease_id={lease_id}");

  let resp = client
    .post(&url)
    .send()
    .await
    .map_err(|e| format!("Release request failed: {e}"))?;

  if resp.status().is_success() {
    resp
      .json::<ReleaseLeaseResponse>()
      .await
      .map_err(|e| format!("Failed to parse ReleaseLeaseResponse: {e}"))
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
  let client = Client::builder()
    .timeout(Duration::from_secs(5))
    .build()
    .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

  let url = format!("{base_url}/v1/workers");
  let resp = client
    .get(&url)
    .send()
    .await
    .map_err(|e| format!("List workers failed: {e}"))?;

  if resp.status().is_success() {
    resp
      .json::<ListWorkersResponse>()
      .await
      .map_err(|e| format!("Failed to parse ListWorkersResponse: {e}"))
  } else {
    let status = resp.status();
    Err(format!("List workers request returned status {status}"))
  }
}
