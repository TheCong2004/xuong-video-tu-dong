use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use sqlite_tasks::connection::TaskDbConnection;
use sqlite_tasks::queries::content_pages::get_content_page_by_id::{get_content_page_by_id, GetContentPageByIdArgs};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};

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
  pub sqlite_ready: bool,
  pub artifact_storage_ready: bool,
  pub floword_scheduler_ready: bool,
  pub publishing_worker_ready: bool,
  pub donut_ready: bool,
  pub workers_online_count: usize,
  pub grok_profile_ready: bool,
  pub facebook_profile_ready: bool,
  pub tiktok_profile_ready: bool,
  pub youtube_profile_ready: bool,
  pub details: Vec<ProbeDetail>,
}

pub struct SystemHealthProbes;

impl SystemHealthProbes {
  /// Probes local storage health for a specific Page by performing a real filesystem write/delete test.
  pub async fn probe_page_storage(
    db: &TaskDbConnection,
    page_id: &str,
  ) -> StorageHealthReport {
    let page = match get_content_page_by_id(GetContentPageByIdArgs { db, id: page_id }).await {
      Ok(Some(p)) => p,
      _ => {
        return StorageHealthReport {
          page_id: page_id.to_string(),
          target_path: "".to_string(),
          exists: false,
          writable: false,
          free_space_bytes: None,
          last_save_success: false,
          error_message: Some(format!("Page '{}' not found in database", page_id)),
        };
      }
    };

    let target_dir = if !page.output_root.trim().is_empty() {
      page.output_root
    } else {
      std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|h| format!("{}/Videos/Floword/{}", h, page.name))
        .unwrap_or_else(|_| format!("./output/{}", page.name))
    };

    let path = Path::new(&target_dir);
    let exists = path.exists();

    // Test writability with actual temp file
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
      let test_file = path.join(".write_test.tmp");
      match OpenOptions::new().write(true).create(true).truncate(true).open(&test_file) {
        Ok(_) => {
          writable = true;
          let _ = std::fs::remove_file(&test_file);
        }
        Err(e) => {
          writable = false;
          error_msg = Some(format!("Directory is not writable: {e}"));
        }
      }
    }

    // Check last save status from SQLite DB
    let last_save_success = sqlx::query_scalar::<_, i64>(
      "SELECT COUNT(*) FROM pipeline_jobs WHERE page_id = $1 AND (business_status IN ('LOCAL_SAVED', 'READY_TO_POST', 'DONE') OR status IN ('DONE', 'COMPLETED', 'READY_TO_POST'))",
    )
    .bind(page_id)
    .fetch_one(db.get_pool())
    .await
    .unwrap_or(0) > 0;

    StorageHealthReport {
      page_id: page_id.to_string(),
      target_path: target_dir,
      exists,
      writable,
      free_space_bytes: None, // Honest None: avoid fake hardcoded numbers
      last_save_success,
      error_message: error_msg,
    }
  }

  /// Probes system readiness across all real subsystems with measured latencies and real Donut query.
  pub async fn probe_system_readiness(
    db: &TaskDbConnection,
  ) -> SystemReadinessReport {
    let mut details = Vec::new();

    // 1. Probe SQLite DB with real latency
    let t0 = std::time::Instant::now();
    let sqlite_ready = match sqlx::query("SELECT 1").execute(db.get_pool()).await {
      Ok(_) => {
        let latency = t0.elapsed().as_millis() as u64;
        details.push(ProbeDetail {
          service: "SQLite DB".to_string(),
          ready: true,
          message: format!("Authoritative database connected ({latency}ms)"),
          latency_ms: Some(latency),
        });
        true
      }
      Err(e) => {
        details.push(ProbeDetail {
          service: "SQLite DB".to_string(),
          ready: false,
          message: format!("SQLite error: {e}"),
          latency_ms: None,
        });
        false
      }
    };

    // 2. Probe Artifact Storage with real write/delete probe
    let artifact_dir = std::env::temp_dir().join("artcraft_artifacts");
    let artifact_storage_ready = match std::fs::create_dir_all(&artifact_dir) {
      Ok(_) => {
        let test_file = artifact_dir.join(".probe.tmp");
        match std::fs::write(&test_file, b"probe_ok") {
          Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            details.push(ProbeDetail {
              service: "Artifact Storage".to_string(),
              ready: true,
              message: format!("Storage writable at {:?}", artifact_dir),
              latency_ms: None,
            });
            true
          }
          Err(e) => {
            details.push(ProbeDetail {
              service: "Artifact Storage".to_string(),
              ready: false,
              message: format!("Storage directory not writable: {e}"),
              latency_ms: None,
            });
            false
          }
        }
      }
      Err(e) => {
        details.push(ProbeDetail {
          service: "Artifact Storage".to_string(),
          ready: false,
          message: format!("Artifact storage failed: {e}"),
          latency_ms: None,
        });
        false
      }
    };

    // 3. Real Scheduler Heartbeat
    let floword_scheduler_ready = is_scheduler_alive(10);
    details.push(ProbeDetail {
      service: "Floword Scheduler".to_string(),
      ready: floword_scheduler_ready,
      message: if floword_scheduler_ready {
        "Pipeline scheduler active & heartbeating".to_string()
      } else {
        "Pipeline scheduler not running or heartbeat expired".to_string()
      },
      latency_ms: None,
    });

    // 4. Real Publishing Worker Heartbeat
    let publishing_worker_ready = is_publishing_worker_alive(10);
    details.push(ProbeDetail {
      service: "Publishing Worker".to_string(),
      ready: publishing_worker_ready,
      message: if publishing_worker_ready {
        "Publishing worker thread active & polling".to_string()
      } else {
        "Publishing worker thread not running or heartbeat expired".to_string()
      },
      latency_ms: None,
    });

    // 5. Real Donut Runtime Query via canonical base URL
    let donut_base_url = crate::services::pipeline::clients::browser_runtime_client::get_donut_browser_api_base_url();
    let client = reqwest::Client::builder()
      .timeout(std::time::Duration::from_millis(1000))
      .build()
      .unwrap_or_default();

    let t_donut = std::time::Instant::now();
    let donut_url = format!("{donut_base_url}/v1/workers");
    let donut_probe = client.get(&donut_url).send().await;

    let mut donut_ready = false;
    let mut workers_online_count = 0;
    let mut grok_profile_ready = false;
    let mut facebook_profile_ready = false;
    let mut tiktok_profile_ready = false;
    let mut youtube_profile_ready = false;

    match donut_probe {
      Ok(resp) if resp.status().is_success() => {
        let latency = t_donut.elapsed().as_millis() as u64;
        donut_ready = true;

        if let Ok(workers) = resp.json::<serde_json::Value>().await {
          if let Some(arr) = workers.as_array() {
            workers_online_count = arr.len();
            for w in arr {
              let id = w.get("id").or_else(|| w.get("worker_id")).and_then(|v| v.as_str()).unwrap_or("");
              let caps = w.get("capabilities").and_then(|v| v.as_array()).cloned().unwrap_or_default();
              let cap_strings: Vec<String> = caps.iter().filter_map(|c| c.as_str().map(|s| s.to_string())).collect();

              if id.contains("grok") || cap_strings.iter().any(|c| c.contains("grok")) {
                grok_profile_ready = true;
              }
              if id.contains("facebook") || cap_strings.iter().any(|c| c.contains("facebook")) {
                facebook_profile_ready = true;
              }
              if id.contains("tiktok") || cap_strings.iter().any(|c| c.contains("tiktok")) {
                tiktok_profile_ready = true;
              }
              if id.contains("youtube") || cap_strings.iter().any(|c| c.contains("youtube")) {
                youtube_profile_ready = true;
              }
            }
          }
        }

        details.push(ProbeDetail {
          service: "Donut Worker Bridge".to_string(),
          ready: true,
          message: format!("Donut runtime reachable at {donut_base_url} ({workers_online_count} workers online, {latency}ms)"),
          latency_ms: Some(latency),
        });
      }
      Ok(resp) => {
        details.push(ProbeDetail {
          service: "Donut Worker Bridge".to_string(),
          ready: false,
          message: format!("Donut returned HTTP {}", resp.status()),
          latency_ms: None,
        });
      }
      Err(e) => {
        details.push(ProbeDetail {
          service: "Donut Worker Bridge".to_string(),
          ready: false,
          message: format!("Donut offline ({donut_base_url}): {e}"),
          latency_ms: None,
        });
      }
    }

    let overall_ready = sqlite_ready && artifact_storage_ready && floword_scheduler_ready;

    SystemReadinessReport {
      overall_ready,
      sqlite_ready,
      artifact_storage_ready,
      floword_scheduler_ready,
      publishing_worker_ready,
      donut_ready,
      workers_online_count,
      grok_profile_ready,
      facebook_profile_ready,
      tiktok_profile_ready,
      youtube_profile_ready,
      details,
    }
  }
}
