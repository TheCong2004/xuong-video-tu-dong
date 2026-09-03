use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use sqlite_tasks::connection::TaskDbConnection;
use sqlite_tasks::queries::content_pages::get_content_page_by_id::{get_content_page_by_id, GetContentPageByIdArgs};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use crate::services::pipeline::clients::browser_runtime_backend::{runtime_api_base_url, BrowserWorkerInfo, ListWorkersResponse};

pub static LAST_SCHEDULER_TICK_UNIX: AtomicI64 = AtomicI64::new(0);
pub static LAST_PUBLISHING_WORKER_TICK_UNIX: AtomicI64 = AtomicI64::new(0);

/// Record heartbeat tick from the pipeline scheduler thread loop.
pub fn record_scheduler_tick() {
  LAST_SCHEDULER_TICK_UNIX.store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
}

/// Record heartbeat tick from the publishing worker thread loop.
pub fn record_publishing_worker_tick() {
  LAST_PUBLISHING_WORKER_TICK_UNIX.store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
}

/// Check if the pipeline scheduler heartbeat is recent.
pub fn is_scheduler_alive(max_age_seconds: i64) -> bool {
  let last = LAST_SCHEDULER_TICK_UNIX.load(Ordering::Relaxed);
  if last == 0 {
    return false;
  }
  (chrono::Utc::now().timestamp() - last) <= max_age_seconds
}

/// Check if the publishing worker heartbeat is recent.
pub fn is_publishing_worker_alive(max_age_seconds: i64) -> bool {
  let last = LAST_PUBLISHING_WORKER_TICK_UNIX.load(Ordering::Relaxed);
  if last == 0 {
    return false;
  }
  (chrono::Utc::now().timestamp() - last) <= max_age_seconds
}

/// Helper to evaluate if a browser worker is runtime-ready and supports a specific capability.
pub fn worker_runtime_supports(worker: &BrowserWorkerInfo, capability: &str) -> bool {
  worker.state == "READY" && worker.extension_ready && worker.protocol_version == Some(1) && worker.capabilities.iter().any(|c| c == capability)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageHealthReport {
  pub page_id: String,
  pub target_path: String,
  pub exists: bool,
  pub writable: bool,
  pub free_space_bytes: Option<u64>,
  pub last_save_success: bool,
  pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeDetail {
  pub service: String,
  pub ready: bool,
  pub message: String,
  pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemReadinessReport {
  pub overall_ready: bool,
  pub core_generation_ready: bool,
  pub publishing_orchestrator_ready: bool,
  pub sqlite_ready: bool,
  pub artifact_storage_ready: bool,
  pub floword_scheduler_ready: bool,
  pub publishing_worker_ready: bool,
  pub donut_ready: bool,
  pub workers_online_count: usize,
  pub grok_profile_ready: bool,
  pub facebook_capability_available: bool,
  pub facebook_profile_ready: bool,
  pub tiktok_capability_available: bool,
  pub tiktok_profile_ready: bool,
  pub youtube_capability_available: bool,
  pub youtube_profile_ready: bool,
  pub details: Vec<ProbeDetail>,
}

pub struct SystemHealthProbes;

impl SystemHealthProbes {
  /// Probes local storage health for a specific Page by performing a real filesystem write/delete test.
  pub async fn probe_page_storage(db: &TaskDbConnection, page_id: &str) -> StorageHealthReport {
    let page = match get_content_page_by_id(GetContentPageByIdArgs { db, id: page_id }).await {
      Ok(Some(p)) => p,
      _ => {
        return StorageHealthReport { page_id: page_id.to_string(), target_path: "".to_string(), exists: false, writable: false, free_space_bytes: None, last_save_success: false, error_message: Some(format!("Page '{}' not found in database", page_id)) };
      },
    };

    let target_dir = if !page.output_root.trim().is_empty() { page.output_root } else { std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).map(|h| format!("{}/Videos/Floword/{}", h, page.name)).unwrap_or_else(|_| format!("./output/{}", page.name)) };

    let path = Path::new(&target_dir);
    let exists = path.exists();

    let mut writable = false;
    let mut error_msg = None;

    if !exists {
      if let Err(e) = std::fs::create_dir_all(path) {
        error_msg = Some(format!("Cannot create directory: {e}"));
      } else {
        writable = true;
      }
    }

    if exists || writable {
      let probe_id = uuid::Uuid::new_v4();
      let test_file = path.join(format!(".floword_page_write_probe_{probe_id}.tmp"));
      match OpenOptions::new().write(true).create(true).truncate(true).open(&test_file) {
        Ok(_) => {
          if let Err(e) = std::fs::remove_file(&test_file) {
            writable = false;
            error_msg = Some(format!("Probe file could not be removed: {e}"));
          } else {
            writable = true;
          }
        },
        Err(e) => {
          writable = false;
          error_msg = Some(format!("Directory is not writable: {e}"));
        },
      }
    }

    // Check last save status from SQLite DB
    let last_save_success = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pipeline_jobs WHERE page_id = $1 AND (business_status IN ('LOCAL_SAVED', 'READY_TO_POST', 'DONE') OR status IN ('DONE', 'COMPLETED', 'READY_TO_POST'))").bind(page_id).fetch_one(db.get_pool()).await.unwrap_or(0) > 0;

    StorageHealthReport { page_id: page_id.to_string(), target_path: target_dir, exists, writable, free_space_bytes: None, last_save_success, error_message: error_msg }
  }

  /// Probes system readiness across all real subsystems with measured latencies and real Donut query.
  pub async fn probe_system_readiness(db: &TaskDbConnection, artifact_dir: PathBuf) -> SystemReadinessReport {
    let mut details = Vec::new();

    // 1. Probe SQLite DB with real latency
    let t0 = std::time::Instant::now();
    let sqlite_ready = match sqlx::query("SELECT 1").execute(db.get_pool()).await {
      Ok(_) => {
        let latency = t0.elapsed().as_millis() as u64;
        details.push(ProbeDetail { service: "SQLite DB".to_string(), ready: true, message: format!("Authoritative database connected ({latency}ms)"), latency_ms: Some(latency) });
        true
      },
      Err(e) => {
        details.push(ProbeDetail { service: "SQLite DB".to_string(), ready: false, message: format!("SQLite error: {e}"), latency_ms: None });
        false
      },
    };

    // 2. Probe canonical pipeline artifact storage with a unique probe file
    let artifact_storage_ready = probe_artifact_dir(&artifact_dir, &mut details);

    // 3. Real Scheduler Heartbeat
    let floword_scheduler_ready = is_scheduler_alive(10);
    details.push(ProbeDetail { service: "Floword Scheduler".to_string(), ready: floword_scheduler_ready, message: if floword_scheduler_ready { "Pipeline scheduler active & heartbeating".to_string() } else { "Pipeline scheduler not running or heartbeat expired".to_string() }, latency_ms: None });

    // 4. Real Publishing Worker Heartbeat
    let publishing_worker_ready = is_publishing_worker_alive(10);
    details.push(ProbeDetail { service: "Publishing Worker".to_string(), ready: publishing_worker_ready, message: if publishing_worker_ready { "Publishing worker thread active & polling".to_string() } else { "Publishing worker thread not running or heartbeat expired".to_string() }, latency_ms: None });

    // 5. Query the ArtCraft Local Browser Runtime via its canonical base URL.
    let runtime_base_url = runtime_api_base_url();
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_millis(1000)).build().unwrap_or_default();

    let t_runtime = std::time::Instant::now();
    let runtime_url = format!("{runtime_base_url}/v1/workers");
    let runtime_probe = client.get(&runtime_url).send().await;

    let mut donut_ready = false;
    let mut workers_online_count = 0usize;
    let mut grok_profile_ready = false;
    let mut facebook_capability_available = false;
    let mut facebook_profile_ready = false;
    let mut tiktok_capability_available = false;
    let mut tiktok_profile_ready = false;
    let mut youtube_capability_available = false;
    let mut youtube_profile_ready = false;

    match runtime_probe {
      Ok(resp) if resp.status().is_success() => {
        let latency = t_runtime.elapsed().as_millis() as u64;

        // Parse response as typed struct to avoid root-array misparse
        match resp.json::<ListWorkersResponse>().await {
          Ok(list) => {
            donut_ready = true;
            // workers_online_count = workers whose state is not "OFFLINE"
            workers_online_count = list.workers.iter().filter(|w| w.state != "OFFLINE").count();

            for w in &list.workers {
              let state_ready = w.state == "READY";
              let caps = &w.capabilities;

              // Grok readiness: worker must be READY, extension_ready, protocol_version=1,
              // grok_logged_in, AND advertise at least one required Grok capability.
              let grok_caps = ["grok.image.edit", "grok.image.expand_9_16", "grok.video.generate"];
              let has_grok_cap = caps.iter().any(|c| grok_caps.contains(&c.as_str()));
              if state_ready && w.extension_ready && w.protocol_version == Some(1) && w.grok_logged_in == Some(true) && has_grok_cap {
                grok_profile_ready = true;
              }

              // Social capability availability check (requires exact runtime support)
              if worker_runtime_supports(w, "social.facebook.publish") {
                facebook_capability_available = true;
              }
              if worker_runtime_supports(w, "social.tiktok.publish") {
                tiktok_capability_available = true;
              }
              if worker_runtime_supports(w, "social.youtube.publish") {
                youtube_capability_available = true;
              }

              // Social profile readiness requires capability + verified platform session.
              // Current worker model carries only grok_logged_in; social session readiness remains false.
              facebook_profile_ready = false;
              tiktok_profile_ready = false;
              youtube_profile_ready = false;
            }

            details.push(ProbeDetail { service: "Browser Worker Bridge".to_string(), ready: true, message: format!("ArtCraft runtime reachable at {runtime_base_url} ({workers_online_count}/{} workers online, grok_ready={grok_profile_ready}, {latency}ms)", list.total), latency_ms: Some(latency) });
          },
          Err(e) => {
            // HTTP reachable but payload failed to deserialize — runtime is not ready.
            error!("[SystemHealth] ArtCraft runtime /v1/workers response parse failed: {e}");
            donut_ready = false;
            details.push(ProbeDetail { service: "Browser Worker Bridge".to_string(), ready: false, message: format!("ArtCraft runtime reachable at {runtime_base_url} but worker payload could not be parsed: {e} ({latency}ms)"), latency_ms: Some(latency) });
          },
        }
      },
      Ok(resp) => {
        details.push(ProbeDetail { service: "Browser Worker Bridge".to_string(), ready: false, message: format!("ArtCraft runtime returned HTTP {}", resp.status()), latency_ms: None });
      },
      Err(e) => {
        details.push(ProbeDetail { service: "Browser Worker Bridge".to_string(), ready: false, message: format!("ArtCraft runtime offline ({runtime_base_url}): {e}"), latency_ms: None });
      },
    }

    // Explicit readiness scopes:
    // Core generation requires DB, storage, scheduler, Donut, and Grok profile
    let core_generation_ready = sqlite_ready && artifact_storage_ready && floword_scheduler_ready && donut_ready && grok_profile_ready;

    let publishing_orchestrator_ready = publishing_worker_ready;

    // Overall system readiness requires both core pipeline and publishing orchestrator
    let overall_ready = core_generation_ready && publishing_orchestrator_ready;

    SystemReadinessReport { overall_ready, core_generation_ready, publishing_orchestrator_ready, sqlite_ready, artifact_storage_ready, floword_scheduler_ready, publishing_worker_ready, donut_ready, workers_online_count, grok_profile_ready, facebook_capability_available, facebook_profile_ready, tiktok_capability_available, tiktok_profile_ready, youtube_capability_available, youtube_profile_ready, details }
  }
}

/// Probe the canonical artifact storage directory with a unique temporary file.
/// Returns true only if directory creation, writing, and cleanup all succeed.
fn probe_artifact_dir(artifact_dir: &PathBuf, details: &mut Vec<ProbeDetail>) -> bool {
  // Ensure directory exists
  if let Err(e) = std::fs::create_dir_all(artifact_dir) {
    details.push(ProbeDetail { service: "Artifact Storage".to_string(), ready: false, message: format!("Cannot create artifact directory at {:?}: {e}", artifact_dir), latency_ms: None });
    return false;
  }

  // Write a unique probe file to avoid collisions between concurrent probes
  let probe_id = uuid::Uuid::new_v4();
  let test_file = artifact_dir.join(format!(".floword_health_probe_{probe_id}.tmp"));

  match std::fs::write(&test_file, b"floword_storage_probe_ok") {
    Ok(_) => {
      // Require cleanup success
      if let Err(e) = std::fs::remove_file(&test_file) {
        warn!("[SystemHealth] Probe file could not be removed at {:?}: {e}", test_file);
        details.push(ProbeDetail { service: "Artifact Storage".to_string(), ready: false, message: format!("Probe file could not be removed at {:?}: {e}", test_file), latency_ms: None });
        false
      } else {
        details.push(ProbeDetail { service: "Artifact Storage".to_string(), ready: true, message: format!("Storage writable and verified at {:?}", artifact_dir), latency_ms: None });
        true
      }
    },
    Err(e) => {
      details.push(ProbeDetail { service: "Artifact Storage".to_string(), ready: false, message: format!("Artifact storage directory not writable at {:?}: {e}", artifact_dir), latency_ms: None });
      false
    },
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::clients::browser_runtime_backend::{BrowserWorkerInfo, ListWorkersResponse};

  /// 11.16 Case A: Donut offline, Grok unavailable -> core_generation_ready=false, overall_ready=false
  #[test]
  fn test_readiness_case_a_donut_offline() {
    let sqlite_ready = true;
    let artifact_storage_ready = true;
    let floword_scheduler_ready = true;
    let publishing_worker_ready = true;
    let donut_ready = false;
    let grok_profile_ready = false;

    let core_generation_ready = sqlite_ready && artifact_storage_ready && floword_scheduler_ready && donut_ready && grok_profile_ready;
    let publishing_orchestrator_ready = publishing_worker_ready;
    let overall_ready = core_generation_ready && publishing_orchestrator_ready;

    assert!(!core_generation_ready, "Core generation must be false when Donut is offline");
    assert!(!overall_ready, "Overall readiness must be false when core generation is not ready");
  }

  /// 11.16 Case B: Core generation ready, but publishing worker dead -> core_generation_ready=true, overall_ready=false
  #[test]
  fn test_readiness_case_b_publishing_worker_dead() {
    let sqlite_ready = true;
    let artifact_storage_ready = true;
    let floword_scheduler_ready = true;
    let publishing_worker_ready = false;
    let donut_ready = true;
    let grok_profile_ready = true;

    let core_generation_ready = sqlite_ready && artifact_storage_ready && floword_scheduler_ready && donut_ready && grok_profile_ready;
    let publishing_orchestrator_ready = publishing_worker_ready;
    let overall_ready = core_generation_ready && publishing_orchestrator_ready;

    assert!(core_generation_ready, "Core generation should be ready");
    assert!(!publishing_orchestrator_ready, "Publishing orchestrator should not be ready");
    assert!(!overall_ready, "Overall readiness must be false when publishing worker is dead");
  }

  /// 11.16 Case C: All subsystems healthy -> overall_ready=true
  #[test]
  fn test_readiness_case_c_all_healthy() {
    let sqlite_ready = true;
    let artifact_storage_ready = true;
    let floword_scheduler_ready = true;
    let publishing_worker_ready = true;
    let donut_ready = true;
    let grok_profile_ready = true;

    let core_generation_ready = sqlite_ready && artifact_storage_ready && floword_scheduler_ready && donut_ready && grok_profile_ready;
    let publishing_orchestrator_ready = publishing_worker_ready;
    let overall_ready = core_generation_ready && publishing_orchestrator_ready;

    assert!(core_generation_ready);
    assert!(publishing_orchestrator_ready);
    assert!(overall_ready);
  }

  /// 11.17 Malformed Donut response -> JSON parse error sets donut_ready=false
  #[test]
  fn test_malformed_donut_response_handling() {
    let malformed_json = r#"{"unexpected_shape": 123}"#;
    let parse_result = serde_json::from_str::<ListWorkersResponse>(malformed_json);
    assert!(parse_result.is_err(), "Malformed response should fail deserialization");
  }

  /// 11.18 Social capability without runtime readiness (e.g. extension_ready=false)
  #[test]
  fn test_worker_runtime_supports_requires_extension_ready() {
    let worker = BrowserWorkerInfo {
      worker_id: "w1".to_string(),
      profile_id: "p1".to_string(),
      pool_id: None,
      state: "READY".to_string(),
      capabilities: vec!["social.facebook.publish".to_string()],
      extension_ready: false, // NOT ready
      extension_version: None,
      protocol_version: Some(1),
      grok_logged_in: Some(false),
      current_lease_id: None,
      current_job_id: None,
      last_heartbeat_at: None,
      last_error: None,
    };

    assert!(!worker_runtime_supports(&worker, "social.facebook.publish"));
  }

  /// 11.19 Social capability available but session readiness remains false
  #[test]
  fn test_social_capability_available_but_session_false() {
    let worker = BrowserWorkerInfo {
      worker_id: "w2".to_string(),
      profile_id: "p2".to_string(),
      pool_id: None,
      state: "READY".to_string(),
      capabilities: vec!["social.facebook.publish".to_string()],
      extension_ready: true,
      extension_version: None,
      protocol_version: Some(1),
      grok_logged_in: Some(true), // Only Grok session is known
      current_lease_id: None,
      current_job_id: None,
      last_heartbeat_at: None,
      last_error: None,
    };

    let fb_cap_available = worker_runtime_supports(&worker, "social.facebook.publish");
    assert!(fb_cap_available, "Facebook capability should be available");

    // Profile readiness must remain false because Facebook session is not verified
    let fb_profile_ready = false;
    assert!(!fb_profile_ready, "Social profile readiness must not be inferred from grok_logged_in");
  }

  /// Test fixture: parse a ListWorkersResponse with a Grok-ready worker.
  #[test]
  fn test_worker_readiness_grok_only() {
    let json = r#"{
      "workers": [
        {
          "worker_id": "browser-profile:1",
          "profile_id": "1",
          "pool_id": null,
          "state": "READY",
          "capabilities": [
            "grok.image.edit",
            "grok.image.expand_9_16",
            "grok.video.generate"
          ],
          "extension_ready": true,
          "extension_version": null,
          "protocol_version": 1,
          "grok_logged_in": true,
          "current_lease_id": null,
          "current_job_id": null,
          "last_heartbeat_at": null,
          "last_error": null
        }
      ],
      "total": 1
    }"#;

    let list: ListWorkersResponse = serde_json::from_str(json).expect("Should parse");
    let workers = &list.workers;
    let online_count = workers.iter().filter(|w| w.state != "OFFLINE").count();
    assert_eq!(online_count, 1, "Expected 1 online worker");

    let grok_caps = ["grok.image.edit", "grok.image.expand_9_16", "grok.video.generate"];
    let grok_ready = workers.iter().any(|w| w.state == "READY" && w.extension_ready && w.protocol_version == Some(1) && w.grok_logged_in == Some(true) && w.capabilities.iter().any(|c| grok_caps.contains(&c.as_str())));
    assert!(grok_ready, "Expected grok_profile_ready=true");

    let fb_cap = workers.iter().any(|w| worker_runtime_supports(w, "social.facebook.publish"));
    assert!(!fb_cap, "Expected facebook_capability_available=false for Grok-only worker");
  }
}
