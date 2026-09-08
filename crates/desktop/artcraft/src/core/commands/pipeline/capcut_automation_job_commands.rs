use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::services::pipeline::capcut_automation_job_manager::{CapcutAutomationJob, CapcutAutomationJobManager, PreviewResponse, StartCapcutAutomationRequest};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};

#[derive(Debug, Deserialize)]
pub struct JobIdRequest {
  pub job_id: String,
}

#[tauri::command]
pub fn start_capcut_automation_job(app: AppHandle, root: State<'_, AppDataRoot>, manager: State<'_, CapcutAutomationJobManager>, request: StartCapcutAutomationRequest) -> Result<CapcutAutomationJob, String> {
  manager.start(app, root.inner().clone(), request)
}

#[tauri::command]
pub fn list_capcut_automation_jobs(app: AppHandle, root: State<'_, AppDataRoot>, manager: State<'_, CapcutAutomationJobManager>) -> Vec<CapcutAutomationJob> {
  manager.restore(app, root.inner().clone())
}

#[tauri::command]
pub fn get_capcut_automation_job(manager: State<'_, CapcutAutomationJobManager>, request: JobIdRequest) -> Result<CapcutAutomationJob, String> {
  manager.get(&request.job_id).ok_or_else(|| "JOB_NOT_FOUND".to_string())
}

#[tauri::command]
pub fn cancel_capcut_automation_job(manager: State<'_, CapcutAutomationJobManager>, request: JobIdRequest) -> Result<CapcutAutomationJob, String> {
  manager.cancel(&request.job_id)
}

#[tauri::command]
pub fn retry_capcut_automation_job(manager: State<'_, CapcutAutomationJobManager>, request: JobIdRequest) -> Result<CapcutAutomationJob, String> {
  manager.retry(&request.job_id)
}

#[tauri::command]
pub fn remove_capcut_automation_job(manager: State<'_, CapcutAutomationJobManager>, request: JobIdRequest) -> Result<(), String> {
  manager.remove(&request.job_id)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
  pub input_path: String,
  pub preset: Option<crate::services::pipeline::capcut_automation::CapcutAutomationPresetV1>,
  #[serde(default)]
  pub start_ms: Option<u64>,
  #[serde(default)]
  pub duration_ms: Option<u64>,
}

/// Generate a short local preview. It is deliberately not a production job
/// and therefore never creates a completion receipt.
#[tauri::command]
pub async fn preview_capcut_automation(app: AppHandle, root: State<'_, AppDataRoot>, request: PreviewRequest) -> Result<PreviewResponse, String> {
  let input = std::path::PathBuf::from(request.input_path.trim()).canonicalize().map_err(|e| format!("INPUT_NOT_READABLE: {e}"))?;
  if !input.is_file() {
    return Err("INPUT_NOT_FOUND".into());
  }
  let mut preset = request.preset.unwrap_or_default();
  preset.normalize_audio_contract();
  preset.validate()?;
  preset.output.width = preset.output.width.min(540).max(2) & !1;
  preset.output.height = preset.output.height.min(540).max(2) & !1;
  preset.hook.end_ms = preset.hook.end_ms.min(8_000);
  let (input_sha, _) = crate::services::pipeline::capcut_automation::file_hashes(&input)?;
  let metadata = std::fs::metadata(&input).map_err(|e| format!("INPUT_NOT_READABLE: {e}"))?;
  let modified = metadata.modified().ok().and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok()).map(|value| value.as_secs()).unwrap_or(0);
  let start_ms = request.start_ms.unwrap_or(0);
  let duration_ms = request.duration_ms.unwrap_or(5_000).clamp(3_000, 8_000);
  let key = preview_cache_key(&input_sha, metadata.len(), modified, &preset, start_ms, duration_ms)?;
  let cache_dir = root.path().join("video-previews");
  std::fs::create_dir_all(&cache_dir).map_err(|e| format!("PREVIEW_DIRECTORY_FAILED: {e}"))?;
  let output = cache_dir.join(format!("{key}.mp4"));
  if !output.is_file() {
    let ffmpeg = app_lib::services::get_ffmpeg_path(&app).await.ok_or_else(|| "RENDER_START_FAILED: packaged FFmpeg is unavailable".to_string())?;
    let input_for_worker = input.clone();
    let output_for_worker = output.clone();
    let cache_for_worker = cache_dir.clone();
    let key_for_worker = key.clone();
    let subtitle_path = preset.localization.subtitle_path.clone().map(std::path::PathBuf::from);
    let start_arg = format!("{}", start_ms as f64 / 1000.0);
    let duration_arg = format!("{}", duration_ms as f64 / 1000.0);
    tokio::task::spawn_blocking(move || {
      let hook_path = if preset.hook.enabled {
        let path = cache_for_worker.join(format!("{key_for_worker}.hook.ass"));
        crate::services::pipeline::capcut_automation::write_hook_ass(&path, &preset)?;
        Some(path)
      } else {
        None
      };
      let manual_subtitle = if preset.localization.manual_cues.iter().any(|cue| cue.enabled) {
        let path = cache_for_worker.join(format!("{key_for_worker}.subs.ass"));
        crate::services::pipeline::capcut_automation::write_manual_subtitle_ass(&path, &preset)?;
        Some(path)
      } else {
        None
      };
      let subtitle = manual_subtitle.as_deref().or(subtitle_path.as_deref());
      if let Some(path) = subtitle {
        if !path.is_file() {
          return Err("SUBTITLE_FILE_NOT_FOUND".to_string());
        }
      }
      let graph = crate::services::pipeline::capcut_automation::build_filter_graph(&preset, subtitle, hook_path.as_deref())?;
      let partial = output_for_worker.with_extension("mp4.partial");
      let mut command = std::process::Command::new(ffmpeg);
      command.args(["-hide_banner", "-loglevel", "error", "-y", "-ss", &start_arg, "-t", &duration_arg, "-i"]).arg(&input_for_worker);
      let filter_script_path = if graph.starts_with("[0:v]") {
        let script = crate::services::pipeline::capcut_automation::configure_filter_complex(&mut command, &output_for_worker, &graph)?;
        command.args(["-map", "[capcut_out]"]);
        script
      } else {
        command.args(["-vf", &graph]);
        None
      };
      let _filter_script_guard = filter_script_path.map(crate::services::pipeline::capcut_automation::FilterScriptGuard);
      let status = command.args(["-an", "-c:v", "libx264", "-preset", "veryfast", "-crf", "26", "-f", "mp4"]).arg(&partial).status().map_err(|e| format!("PREVIEW_RENDER_FAILED: {e}"))?;
      if !status.success() {
        let _ = std::fs::remove_file(&partial);
        return Err("PREVIEW_RENDER_FAILED".into());
      }
      std::fs::rename(partial, output_for_worker).map_err(|e| format!("PREVIEW_PUBLISH_FAILED: {e}"))
    })
    .await
    .map_err(|e| format!("PREVIEW_WORKER_FAILED: {e}"))??;
  }
  Ok(PreviewResponse { path: output.to_string_lossy().to_string(), cache_key: key })
}

fn preview_cache_key(input_sha: &str, input_len: u64, modified_secs: u64, preset: &crate::services::pipeline::capcut_automation::CapcutAutomationPresetV1, start_ms: u64, duration_ms: u64) -> Result<String, String> {
  let preset_bytes = serde_json::to_vec(preset).map_err(|e| format!("PREVIEW_CACHE_KEY_FAILED: {e}"))?;
  let mut digest = Sha256::new();
  digest.update(&preset_bytes);
  let preset_hash = digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
  Ok(format!("{}-{}-{}-{}-{}x{}-{}-{}-v2", input_sha, input_len, modified_secs, preset_hash, preset.output.width, preset.output.height, start_ms, duration_ms))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn preview_cache_key_includes_content_preset_and_time_window() {
    let preset = crate::services::pipeline::capcut_automation::CapcutAutomationPresetV1::default();
    let first = preview_cache_key("input-sha", 10, 20, &preset, 0, 5_000).unwrap();
    let second = preview_cache_key("input-sha", 10, 20, &preset, 1_000, 5_000).unwrap();
    let mut changed = preset.clone();
    changed.output.width += 2;
    let third = preview_cache_key("input-sha", 10, 20, &changed, 0, 5_000).unwrap();
    assert_ne!(first, second);
    assert_ne!(first, third);
    assert!(first.ends_with("-0-5000-v2"));
  }
}
