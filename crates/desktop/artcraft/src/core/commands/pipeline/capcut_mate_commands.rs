//! Explicit lifecycle entry point for the legacy CapCut Mate provider.
//!
//! Native Automation owns its local queue/runtime and must never start the
//! legacy Python service as a side effect of ArtCraft startup. The legacy UI
//! calls this command once before mounting `CapCutMateProvider`.

use crate::core::lifecycle::startup::tasks::spawn_capcut_mate_backend::{health_ready, spawn_capcut_mate_backend};
use tauri::AppHandle;

/// Start (or reuse) capcut-mate for the explicitly selected Legacy UI.
///
/// The underlying launcher is idempotent with respect to an already healthy
/// :30000 service. Running it on a blocking thread keeps the Tauri command
/// executor responsive while the packaged backend waits for readiness.
#[tauri::command]
pub async fn ensure_legacy_capcut_mate(app: AppHandle) -> Result<(), String> {
  if health_ready(30000) {
    return Ok(());
  }

  tauri::async_runtime::spawn_blocking(move || spawn_capcut_mate_backend(&app)).await.map_err(|error| format!("CAPCUT_MATE_START_TASK_FAILED: {error}"))?;

  if health_ready(30000) {
    Ok(())
  } else {
    Err("CAPCUT_MATE_UNAVAILABLE".to_string())
  }
}
