use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::services::pipeline::capcut_automation::{file_hashes, render_video_with_overlays, write_hook_ass, write_receipt, CapcutAutomationPresetV1, CapcutAutomationReceipt};
use crate::services::pipeline::output_policy::OutputPathResolver;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCapcutAutomationRequest {
  pub input_path: String,
  pub output_root: Option<String>,
  pub page_name: Option<String>,
  pub job_id: Option<String>,
  pub request_id: Option<String>,
  pub preset: Option<CapcutAutomationPresetV1>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCapcutAutomationResponse {
  pub receipt: CapcutAutomationReceipt,
}

#[tauri::command]
pub async fn run_capcut_automation_command(app: AppHandle, app_data_root: State<'_, AppDataRoot>, request: RunCapcutAutomationRequest) -> Result<RunCapcutAutomationResponse, String> {
  let preset = request.preset.unwrap_or_default();
  preset.validate()?;
  let input = PathBuf::from(request.input_path.trim());
  if input.as_os_str().is_empty() {
    return Err("INPUT_NOT_FOUND".to_string());
  }

  let job_id = non_empty_id(request.job_id, "local-render")?;
  let request_id = non_empty_id(request.request_id, &job_id)?;
  let page_name = request.page_name.unwrap_or_else(|| "Untitled Page".to_string());
  let output_root = request.output_root.unwrap_or_else(|| app_data_root.path().join("outputs").to_string_lossy().to_string());
  let output_dir = OutputPathResolver::prepare_output_directory(&output_root, &page_name)?;
  let job_dir = app_data_root.path().join("video-jobs").join(&job_id);
  std::fs::create_dir_all(&job_dir).map_err(|e| format!("TEMP_DIRECTORY_CREATE_FAILED: {e}"))?;
  let ffmpeg = app_lib::services::get_ffmpeg_path(&app).await.ok_or_else(|| "RENDER_START_FAILED: packaged FFmpeg is unavailable".to_string())?;
  let ffprobe = ffmpeg.parent().map(|p| p.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" })).filter(|p| p.is_file());
  let rendered = job_dir.join("rendered.mp4");
  let receipt_path = job_dir.join("receipt.json");
  if let Ok(bytes) = std::fs::read(&receipt_path) {
    if let Ok(receipt) = serde_json::from_slice::<CapcutAutomationReceipt>(&bytes) {
      if receipt.terminal && receipt.state == "COMPLETED" && Path::new(&receipt.output_path).is_file() {
        return Ok(RunCapcutAutomationResponse { receipt });
      }
    }
  }
  let output_dir_for_worker = output_dir.clone();
  let page_name_for_worker = page_name.clone();
  let result = tokio::task::spawn_blocking(move || render_local_video(&ffmpeg, ffprobe.as_deref(), &input, &rendered, &output_dir_for_worker, &page_name_for_worker, &job_id, &request_id, &preset, &receipt_path)).await.map_err(|e| format!("INTERNAL_ERROR: {e}"))??;
  Ok(RunCapcutAutomationResponse { receipt: result })
}

fn non_empty_id(value: Option<String>, fallback: &str) -> Result<String, String> {
  let value = value.unwrap_or_else(|| fallback.to_string()).trim().to_string();
  if value.is_empty() || value.len() > 128 || value.contains(['\\', '/', ':']) {
    return Err("JOB_ID_INVALID".to_string());
  }
  Ok(value)
}

fn render_local_video(ffmpeg: &Path, ffprobe: Option<&Path>, input: &Path, rendered: &Path, output_dir: &Path, page_name: &str, job_id: &str, request_id: &str, preset: &CapcutAutomationPresetV1, receipt_path: &Path) -> Result<CapcutAutomationReceipt, String> {
  let input = input.canonicalize().map_err(|e| format!("INPUT_NOT_READABLE: {e}"))?;
  if !input.is_file() {
    return Err("INPUT_NOT_FOUND".to_string());
  }
  let input_sha = file_hashes(&input)?.0;
  let hook_path = if preset.hook.enabled {
    let path = rendered.with_extension("hook.ass");
    write_hook_ass(&path, preset)?;
    Some(path)
  } else {
    None
  };
  let (rendered_sha, _) = render_video_with_overlays(ffmpeg, &input, rendered, preset, None, hook_path.as_deref())?;
  if let Some(path) = &hook_path {
    let _ = std::fs::remove_file(path);
  }
  if rendered_sha == input_sha {
    return Err("ARTIFACT_VERIFY_FAILED: output is identical to input".to_string());
  }
  let (duration_ms, width, height, video_codec, audio_codec) = ffprobe_output(ffprobe, rendered).ok_or_else(|| "MEDIA_PROBE_FAILED".to_string())?;
  if duration_ms == 0 || width == 0 || height == 0 {
    return Err("ARTIFACT_VERIFY_FAILED: invalid output metadata".to_string());
  }
  let filename = OutputPathResolver::generate_final_filename(job_id, "rendered_video", "mp4");
  let final_path = OutputPathResolver::publish_final_file(rendered, output_dir, &filename)?;
  let (output_sha, output_md5) = file_hashes(&final_path)?;
  if output_sha == input_sha {
    return Err("ARTIFACT_VERIFY_FAILED: published output is identical to input".to_string());
  }
  let receipt = CapcutAutomationReceipt { schema_version: 1, job_id: job_id.to_string(), request_id: request_id.to_string(), input_sha256: input_sha, output_sha256: output_sha, output_md5, output_path: final_path.to_string_lossy().to_string(), duration_ms, width, height, video_codec, audio_codec, playback_rate: preset.playback_rate, subtitle_burned: false, subtitle_mode: "NONE".into(), subtitle_cue_count: 0, hook_applied: hook_path.is_some(), foreign_text_regions_applied: preset.foreign_text.manual_regions.len(), terminal: true, state: "COMPLETED".to_string(), ..Default::default() };
  write_receipt(receipt_path, &receipt)?;
  let _ = page_name;
  Ok(receipt)
}

fn ffprobe_output(ffprobe: Option<&Path>, path: &Path) -> Option<(u64, u32, u32, String, String)> {
  let ffprobe = ffprobe?;
  let output = std::process::Command::new(ffprobe).args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"]).arg(path).output().ok()?;
  if !output.status.success() {
    return None;
  }
  let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
  let streams = value.get("streams")?.as_array()?;
  let video = streams.iter().find(|stream| stream.get("codec_type").and_then(serde_json::Value::as_str) == Some("video"))?;
  let audio = streams.iter().find(|stream| stream.get("codec_type").and_then(serde_json::Value::as_str) == Some("audio"));
  let duration = value.get("format").and_then(|f| f.get("duration")).and_then(|d| d.as_str()).and_then(|d| d.parse::<f64>().ok()).unwrap_or(0.0);
  Some((duration.mul_add(1_000.0, 0.0).round() as u64, video.get("width")?.as_u64()? as u32, video.get("height")?.as_u64()? as u32, video.get("codec_name").and_then(serde_json::Value::as_str).unwrap_or("unknown").to_string(), audio.and_then(|stream| stream.get("codec_name")).and_then(serde_json::Value::as_str).unwrap_or("none").to_string()))
}
