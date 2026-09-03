//! Provider-neutral browser runtime boundary.
//!
//! The pipeline talks to Donut Desktop's local free browser manager through
//! this facade. The client never spawns a browser or runtime process; it only
//! requests lifecycle operations and hands the returned exact target to the
//! attach-only Sidecar.

use serde::{Deserialize, Serialize};
use url::Url;

pub use super::browser_runtime_client::{AcquireWorkerRequest, AcquireWorkerResponse, BrowserWorkerInfo, HeartbeatLeaseRequest, HeartbeatLeaseResponse, LeaseStatus, ListWorkersResponse, ReleaseLeaseResponse, GROK_IMAGE_EDIT_CAPABILITY, GROK_IMAGE_EXPAND_9_16_CAPABILITY, GROK_VIDEO_GENERATE_CAPABILITY};

const DEFAULT_LOCAL_RUNTIME_URL: &str = "http://127.0.0.1:10108";

/// Canonical browser identity exchanged between the local runtime and Sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserIdentity {
  #[serde(alias = "profile_id")]
  pub profile_id: String,
  #[serde(alias = "browser_pid")]
  pub browser_pid: u32,
  #[serde(alias = "remote_debugging_port")]
  pub remote_debugging_port: u16,
  #[serde(alias = "cdp_endpoint")]
  pub cdp_endpoint: String,
  #[serde(alias = "launch_generation")]
  pub launch_generation: u64,
  #[serde(alias = "browser_engine")]
  pub browser_engine: String,
  #[serde(alias = "grok_target_id")]
  pub grok_target_id: String,
  #[serde(alias = "grok_page_url")]
  pub grok_page_url: String,
  pub reused: bool,
}

/// Browser-only identity returned by the local runtime before a page is
/// selected. Page ownership is represented separately by `PageReceipt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSession {
  #[serde(alias = "profile_id")]
  pub profile_id: String,
  #[serde(alias = "browser_pid")]
  pub browser_pid: u32,
  #[serde(alias = "remote_debugging_port")]
  pub remote_debugging_port: u16,
  #[serde(alias = "cdp_endpoint")]
  pub cdp_endpoint: String,
  #[serde(alias = "launch_generation")]
  pub launch_generation: u64,
  #[serde(alias = "browser_engine")]
  pub browser_engine: String,
  #[serde(alias = "grok_target_id")]
  pub grok_target_id: Option<String>,
  #[serde(alias = "grok_page_url")]
  pub grok_page_url: Option<String>,
  pub reused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageReceipt {
  pub target_id: String,
  #[serde(rename = "type", alias = "pageType")]
  pub page_type: Option<String>,
  pub url: String,
  pub title: Option<String>,
  pub hostname: Option<String>,
  pub purpose: String,
  pub managed: bool,
  pub state: String,
  pub browser_pid: u32,
  pub launch_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPagesResponse {
  pub profile_id: String,
  pub browser_pid: u32,
  pub remote_debugging_port: u16,
  pub cdp_endpoint: String,
  pub launch_generation: u64,
  pub pages: Vec<PageReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLease {
  pub profile_id: String,
  pub browser_pid: u32,
  pub remote_debugging_port: u16,
  pub cdp_endpoint: String,
  pub launch_generation: u64,
  pub target_id: String,
  pub page_lease_id: String,
  pub page_reused: bool,
  pub purpose: String,
}

impl BrowserIdentity {
  pub fn validate(&self) -> Result<(), String> {
    if self.profile_id.trim().is_empty() {
      return Err("PROFILE_ID_REQUIRED".to_string());
    }
    if self.browser_pid == 0 {
      return Err("BROWSER_PID_INVALID".to_string());
    }
    if self.remote_debugging_port == 0 {
      return Err("REMOTE_DEBUGGING_PORT_INVALID".to_string());
    }
    if self.launch_generation == 0 {
      return Err("LAUNCH_GENERATION_INVALID".to_string());
    }
    if self.browser_engine != "CHROME_FOR_TESTING" {
      return Err("BROWSER_ENGINE_UNSUPPORTED".to_string());
    }
    if self.grok_target_id.trim().is_empty() {
      return Err("GROK_TARGET_ID_REQUIRED".to_string());
    }

    let cdp = Url::parse(&self.cdp_endpoint).map_err(|_| "CDP_ENDPOINT_INVALID".to_string())?;
    if cdp.scheme() != "http" || cdp.username() != "" || cdp.password().is_some() || cdp.query().is_some() || cdp.fragment().is_some() || cdp.port() != Some(self.remote_debugging_port) {
      return Err("CDP_ENDPOINT_NOT_LOOPBACK_HTTP".to_string());
    }
    let host = cdp.host_str().unwrap_or_default().to_ascii_lowercase();
    if !matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1") {
      return Err("CDP_ENDPOINT_NOT_LOOPBACK_HTTP".to_string());
    }

    let grok = Url::parse(&self.grok_page_url).map_err(|_| "GROK_PAGE_URL_INVALID".to_string())?;
    let grok_host = grok.host_str().unwrap_or_default().to_ascii_lowercase();
    if grok.scheme() != "https" || !(grok_host == "grok.com" || grok_host.ends_with(".grok.com")) {
      return Err("GROK_PAGE_URL_NOT_ALLOWED".to_string());
    }
    if self.grok_page_url.contains("[https://") || self.grok_page_url.contains("](") {
      return Err("GROK_PAGE_URL_MARKDOWN_CORRUPTED".to_string());
    }
    Ok(())
  }
}

/// Runtime provider selected for the local free deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserRuntimeKind {
  DonutDesktopLocalManager,
}

/// Provider-neutral configuration passed to a runtime lifecycle operation.
pub type BrowserRuntimeConfiguration = serde_json::Value;

/// Correlation carried by lease, dispatch and cancellation operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeCorrelation {
  pub request_id: String,
  pub job_id: String,
  pub step_id: String,
  pub attempt_id: String,
  pub lease_id: Option<String>,
  pub profile_id: String,
}

impl BrowserRuntimeCorrelation {
  pub fn validate(&self) -> Result<(), String> {
    for (field, value) in [("request_id", self.request_id.as_str()), ("job_id", self.job_id.as_str()), ("step_id", self.step_id.as_str()), ("attempt_id", self.attempt_id.as_str()), ("profile_id", self.profile_id.as_str())] {
      if value.trim().is_empty() {
        return Err(format!("{field}_REQUIRED"));
      }
    }
    if matches!(self.lease_id.as_deref(), Some(value) if value.trim().is_empty()) {
      return Err("lease_id_INVALID".to_string());
    }
    Ok(())
  }
}

/// Provider-neutral facade used by pipeline stages.
#[derive(Debug, Clone, Copy)]
pub struct BrowserRuntimeBackend {
  kind: BrowserRuntimeKind,
}

impl BrowserRuntimeBackend {
  pub const fn local() -> Self {
    Self { kind: BrowserRuntimeKind::DonutDesktopLocalManager }
  }

  pub const fn kind(&self) -> BrowserRuntimeKind {
    self.kind
  }

  /// Verify that Donut's local manager and the attach-only Sidecar are healthy.
  /// Process ownership remains with Donut Desktop; this client never spawns
  /// processes.
  pub async fn ensure_runtime(&self) -> Result<(), String> {
    let runtime = reqwest::Client::builder().timeout(std::time::Duration::from_secs(3)).build().map_err(|e| format!("RUNTIME_CLIENT_INIT_FAILED:{e}"))?;
    // Donut's local manager exposes its readiness contract under the versioned
    // runtime namespace.  `/health` is reserved for the attach-only Sidecar.
    let local = runtime.get(format!("{}/v1/runtime/health", runtime_api_base_url())).send().await.map_err(|e| format!("RUNTIME_UNAVAILABLE:{e}"))?;
    if !local.status().is_success() {
      return Err(format!("RUNTIME_UNAVAILABLE:local status {}", local.status()));
    }
    let sidecar = runtime.get(format!("{}/health", extension_bridge_base_url())).send().await.map_err(|e| format!("SIDECAR_UNAVAILABLE:{e}"))?;
    if !sidecar.status().is_success() {
      return Err(format!("SIDECAR_UNAVAILABLE:status {}", sidecar.status()));
    }
    Ok(())
  }

  pub async fn run_profile(&self, profile_id: &str, configuration: BrowserRuntimeConfiguration) -> Result<BrowserIdentity, String> {
    if self.kind != BrowserRuntimeKind::DonutDesktopLocalManager {
      return Err("RUNTIME_PROVIDER_UNAVAILABLE".to_string());
    }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| format!("RUNTIME_CLIENT_INIT_FAILED:{e}"))?;
    let response = client.post(format!("{}/v1/local/browser/profiles/{}/run", runtime_api_base_url(), profile_id)).json(&configuration).send().await.map_err(|e| format!("RUNTIME_UNAVAILABLE:{e}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
      return Err(format!("RUNTIME_RUN_FAILED:{}", status));
    }
    let value: serde_json::Value = serde_json::from_str(&body).map_err(|_| "LOCAL_BROWSER_RUNTIME_CONTRACT_INVALID".to_string())?;
    let identity: BrowserIdentity = serde_json::from_value(value).map_err(|_| "LOCAL_BROWSER_RUNTIME_CONTRACT_INVALID".to_string())?;
    identity.validate().map_err(|_| "LOCAL_BROWSER_RUNTIME_CONTRACT_INVALID".to_string())?;
    if identity.profile_id != profile_id {
      return Err("LOCAL_BROWSER_RUNTIME_CONTRACT_INVALID".to_string());
    }
    Ok(identity)
  }

  /// Start/reuse only the browser process. Page creation is an explicit
  /// operation through `ensure_page`, so one profile can own many targets.
  pub async fn run_browser(&self, profile_id: &str, configuration: BrowserRuntimeConfiguration) -> Result<BrowserSession, String> {
    if self.kind != BrowserRuntimeKind::DonutDesktopLocalManager { return Err("RUNTIME_PROVIDER_UNAVAILABLE".to_string()); }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| format!("RUNTIME_CLIENT_INIT_FAILED:{e}"))?;
    let response = client.post(format!("{}/v1/local/browser/profiles/{}/run", runtime_api_base_url(), profile_id)).json(&configuration).send().await.map_err(|e| format!("RUNTIME_UNAVAILABLE:{e}"))?;
    let status = response.status(); let body = response.text().await.unwrap_or_default();
    if !status.is_success() { return Err(format!("RUNTIME_RUN_FAILED:{}", status)); }
    let session: BrowserSession = serde_json::from_str(&body).map_err(|_| "LOCAL_BROWSER_RUNTIME_CONTRACT_INVALID".to_string())?;
    if session.profile_id != profile_id || session.browser_pid == 0 || session.remote_debugging_port == 0 || session.launch_generation == 0 || session.browser_engine != "CHROME_FOR_TESTING" { return Err("LOCAL_BROWSER_RUNTIME_CONTRACT_INVALID".to_string()); }
    Ok(session)
  }

  pub async fn list_pages(&self, profile_id: &str) -> Result<BrowserPagesResponse, String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|e| format!("RUNTIME_CLIENT_INIT_FAILED:{e}"))?;
    let response = client.get(format!("{}/v1/local/browser/profiles/{}/pages", runtime_api_base_url(), profile_id)).send().await.map_err(|e| format!("RUNTIME_UNAVAILABLE:{e}"))?;
    let status = response.status(); let body = response.text().await.unwrap_or_default();
    if !status.is_success() { return Err(format!("RUNTIME_PAGES_FAILED:{}", status)); }
    serde_json::from_str(&body).map_err(|_| "LOCAL_BROWSER_RUNTIME_CONTRACT_INVALID".to_string())
  }

  pub async fn ensure_page(&self, profile_id: &str, url: &str, purpose: &str, reuse_existing: bool) -> Result<PageReceipt, String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| format!("RUNTIME_CLIENT_INIT_FAILED:{e}"))?;
    let body = serde_json::json!({ "url": url, "purpose": purpose, "reuseExisting": reuse_existing });
    let response = client.post(format!("{}/v1/local/browser/profiles/{}/pages", runtime_api_base_url(), profile_id)).json(&body).send().await.map_err(|e| format!("RUNTIME_UNAVAILABLE:{e}"))?;
    let status = response.status(); let value: serde_json::Value = response.json().await.map_err(|_| "RUNTIME_RESPONSE_INVALID".to_string())?;
    if !status.is_success() { return Err(value.get("error").and_then(|v| v.get("code")).and_then(|v| v.as_str()).unwrap_or("RUNTIME_PAGE_FAILED").to_string()); }
    serde_json::from_value(value.get("page").cloned().ok_or_else(|| "RUNTIME_PAGE_RESPONSE_INVALID".to_string())?).map_err(|_| "RUNTIME_PAGE_RESPONSE_INVALID".to_string())
  }

  /// Atomically claim an idle managed page, or create one when capacity is
  /// available. The local runtime owns the browser and target lifecycle.
  pub async fn claim_page(&self, profile_id: &str, job_id: &str, request_id: &str, purpose: &str, max_pages: usize) -> Result<PageLease, String> {
    if profile_id.trim().is_empty() || job_id.trim().is_empty() || request_id.trim().is_empty() {
      return Err("PAGE_CLAIM_CORRELATION_REQUIRED".to_string());
    }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| format!("RUNTIME_CLIENT_INIT_FAILED:{e}"))?;
    let body = serde_json::json!({ "jobId": job_id, "requestId": request_id, "purpose": purpose, "maxPages": max_pages });
    let response = client.post(format!("{}/v1/local/browser/profiles/{}/pages/claim", runtime_api_base_url(), profile_id)).json(&body).send().await.map_err(|e| format!("RUNTIME_UNAVAILABLE:{e}"))?;
    let status = response.status();
    let value: serde_json::Value = response.json().await.map_err(|_| "RUNTIME_RESPONSE_INVALID".to_string())?;
    if !status.is_success() {
      return Err(value.get("error").and_then(|v| v.get("code")).and_then(|v| v.as_str()).unwrap_or("RUNTIME_PAGE_CLAIM_FAILED").to_string());
    }
    serde_json::from_value(value).map_err(|_| "RUNTIME_PAGE_CLAIM_RESPONSE_INVALID".to_string())
  }

  pub async fn release_page(&self, profile_id: &str, target_id: &str, page_lease_id: &str, job_id: &str, request_id: &str) -> Result<(), String> {
    if profile_id.trim().is_empty() || target_id.trim().is_empty() || page_lease_id.trim().is_empty() || job_id.trim().is_empty() || request_id.trim().is_empty() {
      return Err("PAGE_LEASE_CORRELATION_REQUIRED".to_string());
    }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|e| format!("RUNTIME_CLIENT_INIT_FAILED:{e}"))?;
    let body = serde_json::json!({ "pageLeaseId": page_lease_id, "jobId": job_id, "requestId": request_id });
    let response = client.post(format!("{}/v1/local/browser/profiles/{}/pages/{}/release", runtime_api_base_url(), profile_id, target_id)).json(&body).send().await.map_err(|e| format!("RUNTIME_UNAVAILABLE:{e}"))?;
    if response.status().is_success() { Ok(()) } else { Err(format!("RUNTIME_PAGE_RELEASE_FAILED:{}", response.status())) }
  }

  pub async fn delete_page(&self, profile_id: &str, target_id: &str) -> Result<(), String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|e| format!("RUNTIME_CLIENT_INIT_FAILED:{e}"))?;
    if target_id.trim().is_empty() || !target_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return Err("TARGET_ID_INVALID".to_string()); }
    let response = client.delete(format!("{}/v1/local/browser/profiles/{}/pages/{}", runtime_api_base_url(), profile_id, target_id)).send().await.map_err(|e| format!("RUNTIME_UNAVAILABLE:{e}"))?;
    if response.status().is_success() { Ok(()) } else { Err(format!("RUNTIME_PAGE_DELETE_FAILED:{}", response.status())) }
  }

  pub async fn stop_profile(&self, profile_id: &str) -> Result<(), String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|e| format!("RUNTIME_CLIENT_INIT_FAILED:{e}"))?;
    let response = client.post(format!("{}/v1/local/browser/profiles/{}/stop", runtime_api_base_url(), profile_id)).json(&serde_json::json!({})).send().await.map_err(|e| format!("RUNTIME_UNAVAILABLE:{e}"))?;
    if response.status().is_success() {
      Ok(())
    } else {
      Err(format!("RUNTIME_STOP_FAILED:{}", response.status()))
    }
  }

  pub async fn start_worker(&self, profile_id: &str, identity: &BrowserIdentity) -> Result<BrowserWorkerInfo, String> {
    identity.validate().map_err(|_| "LOCAL_BROWSER_RUNTIME_CONTRACT_INVALID".to_string())?;
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| format!("SIDECAR_CLIENT_INIT_FAILED:{e}"))?;
    let body = serde_json::json!({
      "cdpEndpoint": identity.cdp_endpoint,
      "browserPid": identity.browser_pid,
      "launchGeneration": identity.launch_generation,
      "browserEngine": identity.browser_engine,
      "grokTargetId": identity.grok_target_id,
      "grokPageUrl": identity.grok_page_url
    });
    let response = client.post(format!("{}/v1/profiles/{}/start", extension_bridge_base_url(), profile_id)).json(&body).send().await.map_err(|e| format!("SIDECAR_UNAVAILABLE:{e}"))?;
    let status = response.status();
    let value: serde_json::Value = response.json().await.map_err(|_| "SIDECAR_RESPONSE_INVALID".to_string())?;
    if !status.is_success() {
      return Err(value.get("error").and_then(|e| e.get("code")).and_then(|v| v.as_str()).unwrap_or("SIDECAR_UNAVAILABLE").to_string());
    }
    worker_info_from_value(profile_id, value)
  }

  pub async fn status(&self, profile_id: &str) -> Result<BrowserWorkerInfo, String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().map_err(|e| format!("SIDECAR_CLIENT_INIT_FAILED:{e}"))?;
    let response = client.get(format!("{}/v1/profiles/{}/status", extension_bridge_base_url(), profile_id)).send().await.map_err(|e| format!("SIDECAR_UNAVAILABLE:{e}"))?;
    let status = response.status();
    let value: serde_json::Value = response.json().await.map_err(|_| "SIDECAR_RESPONSE_INVALID".to_string())?;
    if !status.is_success() {
      return Err(value.get("error").and_then(|e| e.get("code")).and_then(|v| v.as_str()).unwrap_or("SIDECAR_UNAVAILABLE").to_string());
    }
    worker_info_from_value(profile_id, value)
  }

  pub async fn acquire_lease(&self, request: AcquireWorkerRequest) -> Result<AcquireWorkerResponse, String> {
    if self.kind != BrowserRuntimeKind::DonutDesktopLocalManager {
      return Err("RUNTIME_PROVIDER_UNAVAILABLE".to_string());
    }
    acquire_worker(request).await
  }

  pub async fn dispatch(&self, _request: serde_json::Value) -> Result<serde_json::Value, String> {
    Err(self.not_implemented("dispatch"))
  }

  pub async fn cancel(&self, _correlation: BrowserRuntimeCorrelation) -> Result<(), String> {
    Err(self.not_implemented("cancel"))
  }

  pub async fn release_lease(&self, correlation: &BrowserRuntimeCorrelation) -> Result<ReleaseLeaseResponse, String> {
    correlation.validate()?;
    let lease_id = correlation.lease_id.as_deref().ok_or_else(|| "lease_id_REQUIRED".to_string())?;
    release_lease(lease_id).await
  }

  pub async fn shutdown(&self) -> Result<(), String> {
    // The local runtime owns shutdown in PHASE 2.  A no-op declaration keeps
    // the facade safe for callers during the migration boundary.
    Ok(())
  }

  fn not_implemented(&self, operation: &str) -> String {
    format!("{:?}_{operation}_NOT_IMPLEMENTED", self.kind)
  }
}

fn worker_info_from_value(profile_id: &str, value: serde_json::Value) -> Result<BrowserWorkerInfo, String> {
  let result = match value.get("result") {
    Some(v) => v.clone(),
    None => value,
  };
  let get_string = |key: &str| result.get(key).and_then(|v| v.as_str()).map(str::to_string);
  Ok(BrowserWorkerInfo {
    worker_id: get_string("workerId").unwrap_or_else(|| format!("playwright:{profile_id}")),
    profile_id: get_string("profileId").unwrap_or_else(|| profile_id.to_string()),
    pool_id: get_string("poolId"),
    state: get_string("state").or_else(|| get_string("status")).unwrap_or_else(|| "READY".to_string()),
    capabilities: result.get("capabilities").and_then(|v| v.as_array()).map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default(),
    extension_ready: result.get("extensionReady").and_then(|v| v.as_bool()).unwrap_or(true),
    extension_version: get_string("extensionVersion"),
    protocol_version: result.get("protocolVersion").and_then(|v| v.as_u64()).map(|v| v as u32),
    grok_logged_in: result.get("loggedIn").and_then(|v| v.as_bool()).or_else(|| result.get("grokLoggedIn").and_then(|v| v.as_bool())),
    current_lease_id: get_string("currentLeaseId"),
    current_job_id: get_string("currentJobId"),
    last_heartbeat_at: get_string("lastHeartbeatAt"),
    last_error: get_string("lastError"),
  })
}

pub fn runtime_api_base_url() -> String {
  std::env::var("BROWSER_RUNTIME_URL").unwrap_or_else(|_| DEFAULT_LOCAL_RUNTIME_URL.to_string()).trim_end_matches('/').to_string()
}

pub fn extension_bridge_base_url() -> String {
  std::env::var("EXTENSION_BRIDGE_URL").unwrap_or_else(|_| "http://127.0.0.1:9223".to_string()).trim_end_matches('/').to_string()
}

pub fn build_dispatch_url(base: &str, worker_id: &str) -> String {
  let clean_base = base.trim_end_matches('/');
  format!("{clean_base}/v1/workers/{}/dispatch", worker_id.trim())
}

pub async fn acquire_worker(req: AcquireWorkerRequest) -> Result<AcquireWorkerResponse, String> {
  // The local manager is the authoritative lease owner. This facade keeps
  // transport details out of pipeline stages while preserving the existing
  // worker/lease protocol.
  super::browser_runtime_client::acquire_worker(req).await
}

pub async fn heartbeat_lease(lease_id: &str, req: HeartbeatLeaseRequest) -> Result<HeartbeatLeaseResponse, String> {
  super::browser_runtime_client::heartbeat_lease(lease_id, req).await
}

pub async fn release_lease(lease_id: &str) -> Result<ReleaseLeaseResponse, String> {
  super::browser_runtime_client::release_lease(lease_id).await
}

pub async fn list_workers() -> Result<ListWorkersResponse, String> {
  Err("ARTCRAFT_LOCAL_RUNTIME_WORKER_UNAVAILABLE".to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn valid_identity() -> BrowserIdentity {
    BrowserIdentity { profile_id: "profile-1".into(), browser_pid: 42, remote_debugging_port: 9222, cdp_endpoint: "http://127.0.0.1:9222".into(), launch_generation: 1, browser_engine: "CHROME_FOR_TESTING".into(), grok_target_id: "target-1".into(), grok_page_url: "https://grok.com/imagine".into(), reused: false }
  }

  #[test]
  fn canonical_identity_accepts_valid_values() {
    assert!(valid_identity().validate().is_ok());
  }

  #[test]
  fn canonical_identity_rejects_invalid_values() {
    let mut identity = valid_identity();
    identity.browser_pid = 0;
    assert_eq!(identity.validate().unwrap_err(), "BROWSER_PID_INVALID");

    let mut identity = valid_identity();
    identity.cdp_endpoint = "https://example.com:9222".into();
    assert_eq!(identity.validate().unwrap_err(), "CDP_ENDPOINT_NOT_LOOPBACK_HTTP");

    let mut identity = valid_identity();
    identity.cdp_endpoint = "http://127.0.0.1:9223?q=redacted".into();
    assert_eq!(identity.validate().unwrap_err(), "CDP_ENDPOINT_NOT_LOOPBACK_HTTP");

    let mut identity = valid_identity();
    identity.grok_page_url = "[https://grok.com/imagine](https://grok.com/imagine)".into();
    assert_eq!(identity.validate().unwrap_err(), "GROK_PAGE_URL_INVALID");
  }

  #[test]
  fn identity_serializes_with_canonical_camel_case_fields() {
    let value = serde_json::to_value(valid_identity()).expect("identity serializes");
    for key in ["profileId", "browserPid", "remoteDebuggingPort", "cdpEndpoint", "launchGeneration", "browserEngine", "grokTargetId", "grokPageUrl", "reused"] {
      assert!(value.get(key).is_some(), "missing canonical key {key}");
    }
    assert!(value.get("profile_id").is_none());
  }

  #[test]
  fn correlation_requires_non_empty_identity_fields() {
    let correlation = BrowserRuntimeCorrelation { request_id: String::new(), job_id: "job-1".into(), step_id: "step-1".into(), attempt_id: "attempt-1".into(), lease_id: Some("lease-1".into()), profile_id: "profile-1".into() };
    assert_eq!(correlation.validate().unwrap_err(), "request_id_REQUIRED");
  }

  #[test]
  fn pipeline_uses_facade_without_legacy_header_or_auto_launch() {
    let root = env!("CARGO_MANIFEST_DIR");
    for stage in ["src/services/pipeline/grok_image_edit_stage.rs", "src/services/pipeline/grok_expand_9_16_stage.rs", "src/services/pipeline/grok_video_generate_stage.rs"] {
      let source = std::fs::read_to_string(format!("{root}/{stage}")).expect("stage source");
      assert!(source.contains("browser_runtime_backend"), "{stage} must use facade");
      assert!(!source.contains("browser_runtime_client"), "{stage} imports legacy adapter");
      let integration_header = ["X-Floword", "Integration"].join("-");
      assert!(!source.contains(&integration_header), "{stage} owns integration header");
      let legacy_launch_symbol = ["launch", "donut", "profile"].join("_");
      assert!(!source.contains(&legacy_launch_symbol), "{stage} auto-launches legacy runtime");
    }
  }

  #[test]
  fn provider_neutral_urls_do_not_depend_on_donut_names() {
    assert_eq!(runtime_api_base_url(), "http://127.0.0.1:10108");
    assert_eq!(build_dispatch_url("http://127.0.0.1:10108/", "worker:1"), "http://127.0.0.1:10108/v1/workers/worker:1/dispatch");
  }

  #[test]
  fn local_provider_is_the_only_runtime_kind() {
    assert_eq!(BrowserRuntimeBackend::local().kind(), BrowserRuntimeKind::DonutDesktopLocalManager);
  }

  #[test]
  fn page_lease_contract_preserves_exact_target_and_generation() {
    let lease: PageLease = serde_json::from_value(serde_json::json!({
      "profileId": "profile-1",
      "browserPid": 42,
      "remoteDebuggingPort": 9222,
      "cdpEndpoint": "http://127.0.0.1:9222",
      "launchGeneration": 9,
      "targetId": "target-exact",
      "pageLeaseId": "lease-1",
      "pageReused": true,
      "purpose": "GROK_AUTOMATION"
    })).expect("page lease contract");
    assert_eq!(lease.target_id, "target-exact");
    assert_eq!(lease.launch_generation, 9);
    assert!(lease.page_reused);
  }
  #[test]
  fn extension_bridge_defaults_to_sidecar_port() {
    assert_eq!(extension_bridge_base_url(), "http://127.0.0.1:9223");
  }
  #[test]
  fn dispatch_url_trims_base_slashes() {
    assert_eq!(build_dispatch_url("http://127.0.0.1:10108///", " worker"), "http://127.0.0.1:10108/v1/workers/worker/dispatch");
  }
  #[test]
  fn identity_rejects_zero_debug_port() {
    let mut i = valid_identity();
    i.remote_debugging_port = 0;
    assert_eq!(i.validate().unwrap_err(), "REMOTE_DEBUGGING_PORT_INVALID");
  }
  #[test]
  fn identity_rejects_zero_generation() {
    let mut i = valid_identity();
    i.launch_generation = 0;
    assert_eq!(i.validate().unwrap_err(), "LAUNCH_GENERATION_INVALID");
  }
  #[test]
  fn identity_rejects_wrong_engine() {
    let mut i = valid_identity();
    i.browser_engine = "CHROMIUM".into();
    assert_eq!(i.validate().unwrap_err(), "BROWSER_ENGINE_UNSUPPORTED");
  }
  #[test]
  fn identity_rejects_empty_target() {
    let mut i = valid_identity();
    i.grok_target_id.clear();
    assert_eq!(i.validate().unwrap_err(), "GROK_TARGET_ID_REQUIRED");
  }
  #[test]
  fn identity_rejects_non_https_grok() {
    let mut i = valid_identity();
    i.grok_page_url = "http://grok.com/imagine".into();
    assert_eq!(i.validate().unwrap_err(), "GROK_PAGE_URL_NOT_ALLOWED");
  }
  #[test]
  fn identity_rejects_lookalike_grok_domain() {
    let mut i = valid_identity();
    i.grok_page_url = "https://grok.com.evil.test/imagine".into();
    assert_eq!(i.validate().unwrap_err(), "GROK_PAGE_URL_NOT_ALLOWED");
  }
  #[test]
  fn identity_accepts_grok_subdomain() {
    let mut i = valid_identity();
    i.grok_page_url = "https://imagine.grok.com/imagine".into();
    assert!(i.validate().is_ok());
  }
  #[test]
  fn identity_rejects_cdp_credentials() {
    let mut i = valid_identity();
    i.cdp_endpoint = "http://user:pass@127.0.0.1:9222".into();
    assert_eq!(i.validate().unwrap_err(), "CDP_ENDPOINT_NOT_LOOPBACK_HTTP");
  }
  #[test]
  fn identity_rejects_cdp_query() {
    let mut i = valid_identity();
    i.cdp_endpoint = "http://127.0.0.1:9222?token=secret".into();
    assert_eq!(i.validate().unwrap_err(), "CDP_ENDPOINT_NOT_LOOPBACK_HTTP");
  }
  #[test]
  fn identity_rejects_non_loopback_cdp() {
    let mut i = valid_identity();
    i.cdp_endpoint = "http://192.0.2.1:9222".into();
    assert_eq!(i.validate().unwrap_err(), "CDP_ENDPOINT_NOT_LOOPBACK_HTTP");
  }
  #[test]
  fn identity_rejects_cdp_port_mismatch() {
    let mut i = valid_identity();
    i.cdp_endpoint = "http://127.0.0.1:9223".into();
    assert_eq!(i.validate().unwrap_err(), "CDP_ENDPOINT_NOT_LOOPBACK_HTTP");
  }
  #[test]
  fn correlation_accepts_complete_identity() {
    let c = BrowserRuntimeCorrelation { request_id: "r".into(), job_id: "j".into(), step_id: "s".into(), attempt_id: "a".into(), lease_id: Some("l".into()), profile_id: "p".into() };
    assert!(c.validate().is_ok());
  }
  #[test]
  fn correlation_rejects_empty_job() {
    let c = BrowserRuntimeCorrelation { request_id: "r".into(), job_id: " ".into(), step_id: "s".into(), attempt_id: "a".into(), lease_id: None, profile_id: "p".into() };
    assert_eq!(c.validate().unwrap_err(), "job_id_REQUIRED");
  }
  #[test]
  fn correlation_rejects_empty_step() {
    let c = BrowserRuntimeCorrelation { request_id: "r".into(), job_id: "j".into(), step_id: "".into(), attempt_id: "a".into(), lease_id: None, profile_id: "p".into() };
    assert_eq!(c.validate().unwrap_err(), "step_id_REQUIRED");
  }
  #[test]
  fn correlation_rejects_empty_attempt() {
    let c = BrowserRuntimeCorrelation { request_id: "r".into(), job_id: "j".into(), step_id: "s".into(), attempt_id: "".into(), lease_id: None, profile_id: "p".into() };
    assert_eq!(c.validate().unwrap_err(), "attempt_id_REQUIRED");
  }
  #[test]
  fn correlation_rejects_empty_profile() {
    let c = BrowserRuntimeCorrelation { request_id: "r".into(), job_id: "j".into(), step_id: "s".into(), attempt_id: "a".into(), lease_id: None, profile_id: "".into() };
    assert_eq!(c.validate().unwrap_err(), "profile_id_REQUIRED");
  }
  #[test]
  fn correlation_rejects_blank_lease() {
    let c = BrowserRuntimeCorrelation { request_id: "r".into(), job_id: "j".into(), step_id: "s".into(), attempt_id: "a".into(), lease_id: Some(" ".into()), profile_id: "p".into() };
    assert_eq!(c.validate().unwrap_err(), "lease_id_INVALID");
  }
  #[test]
  fn stages_do_not_reference_legacy_client() {
    let root = env!("CARGO_MANIFEST_DIR");
    for stage in ["grok_image_edit_stage.rs", "grok_expand_9_16_stage.rs", "grok_video_generate_stage.rs"] {
      let text = std::fs::read_to_string(format!("{root}/src/services/pipeline/{stage}")).unwrap();
      assert!(!text.contains("browser_runtime_client"));
    }
  }
  #[test]
  fn startup_does_not_call_legacy_supervisor() {
    let root = env!("CARGO_MANIFEST_DIR");
    let text = std::fs::read_to_string(format!("{root}/src/core/lifecycle/startup/handle_tauri_startup.rs")).unwrap();
    assert!(!text.contains("start_runtime_supervisor"));
    assert!(!text.contains("start_playwright_runtime"));
  }
  #[test]
  fn active_production_call_graph_has_no_donut_automation_calls() {
    let root = env!("CARGO_MANIFEST_DIR");
    let files = ["src/lib.rs", "src/core/lifecycle/startup/handle_tauri_startup.rs", "src/core/commands/pipeline/floword_commands.rs", "src/services/pipeline/system_health_probes.rs", "src/services/publishing/adapters/youtube_adapter.rs", "src/services/publishing/adapters/facebook_adapter.rs", "src/services/publishing/adapters/tiktok_adapter.rs"];
    let forbidden = ["launch_donut_profile", "X-Floword-Integration", "floword-donut-runtime", "donutbrowser.exe", "start_runtime_supervisor", "start_playwright_runtime", "stop_playwright_runtime", "try_state::<RuntimeSupervisor>"];
    for relative in files {
      let text = std::fs::read_to_string(format!("{root}/{relative}")).unwrap();
      for token in forbidden {
        assert!(!text.contains(token), "{relative} contains active legacy token {token}");
      }
    }
  }
  #[test]
  fn backend_does_not_emit_integration_header() {
    let root = env!("CARGO_MANIFEST_DIR");
    let text = std::fs::read_to_string(format!("{root}/src/services/pipeline/clients/browser_runtime_backend.rs")).unwrap();
    let implementation = text.split("#[cfg(test)]").next().unwrap_or(&text);
    assert!(!implementation.contains("X-Floword-Integration"));
  }
}
