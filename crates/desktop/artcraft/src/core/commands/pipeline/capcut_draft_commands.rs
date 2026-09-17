use crate::core::state::task_database::TaskDatabase;
use crate::services::pipeline::artifact_store::ArtifactStore;
use crate::services::pipeline::capcut::prepare_capcut;
use crate::services::pipeline::capcut_draft_engine::{build_draft, EditingPreset};
use crate::services::pipeline::contracts::{ArtifactKind, PipelineContext, StageId};
use enums::tauri::pipeline::pipeline_stage::PipelineStage;
use enums::tauri::tasks::task_status::TaskStatus;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlite_tasks::queries::pipeline::get_pipeline_job_by_id::{get_pipeline_job_by_id, GetPipelineJobByIdArgs};
use sqlite_tasks::queries::pipeline::update_pipeline_job_stage::{update_pipeline_job_stage, UpdatePipelineJobStageArgs};
use sqlite_tasks::queries::pipeline::update_pipeline_job_status::{update_pipeline_job_status, UpdatePipelineJobStatusArgs};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tokens::tokens::sqlite::pipeline_jobs::PipelineJobId;

#[derive(Debug, Deserialize)]
pub struct RebuildCapcutDraftRequest {
  pub job_id: String,
  pub editing_preset: Option<EditingPreset>,
}

#[derive(Debug, Serialize)]
pub struct CapcutDraftActionResponse {
  pub draft_id: String,
  pub draft_path: String,
  pub engine: String,
}

#[tauri::command]
pub async fn rebuild_floword_capcut_draft_command(task_database: State<'_, TaskDatabase>, request: RebuildCapcutDraftRequest) -> Result<CapcutDraftActionResponse, String> {
  let pipeline_job_id = PipelineJobId::new_from_str(&request.job_id);
  let job = get_pipeline_job_by_id(GetPipelineJobByIdArgs { db: task_database.get_connection(), pipeline_job_id: &pipeline_job_id }).await.map_err(|error| error.to_string())?.ok_or_else(|| "Không tìm thấy job để dựng lại CapCut".to_string())?;
  let mut outputs: Value = job.maybe_stage_outputs.as_deref().and_then(|raw| serde_json::from_str(raw).ok()).ok_or_else(|| "Job chưa có artifact dựng phim".to_string())?;
  let mut context: PipelineContext = serde_json::from_value(outputs.get("pipeline_context").cloned().ok_or_else(|| "Job thiếu pipeline context".to_string())?).map_err(|error| error.to_string())?;
  let capcut_input = prepare_capcut(&context).map_err(|error| error.to_string())?;
  let payload: Value = job.maybe_input_payload.as_deref().and_then(|raw| serde_json::from_str(raw).ok()).unwrap_or_else(|| json!({}));
  let preset = request.editing_preset.unwrap_or_else(|| EditingPreset::from_payload(&payload));
  let build_input = capcut_input.clone();
  let build_preset = preset.clone();
  let build_job_id = request.job_id.clone();
  let result = tokio::task::spawn_blocking(move || build_draft(&build_input, &build_preset, &build_job_id)).await.map_err(|error| error.to_string())?.map_err(|error| error.to_string())?;

  let old_manifest = outputs.get("capcut_artifact").and_then(|value| value.get("path")).and_then(Value::as_str).ok_or_else(|| "Job chưa có CapCut artifact để cập nhật".to_string())?;
  let work_dir = Path::new(old_manifest).parent().and_then(Path::parent).ok_or_else(|| "Đường dẫn artifact CapCut không hợp lệ".to_string())?;
  let manifest_path = work_dir.join("capcut").join(format!("draft_manifest_rebuild_{}.json", chrono::Utc::now().timestamp_millis()));
  let draft_path = result.draft_path.to_string_lossy().to_string();
  let manifest = json!({
    "draftId": result.draft_id,
    "draftPath": draft_path,
    "desktopRoot": result.desktop_root,
    "visualTrackCount": result.visual_track_count,
    "audioTrackCount": result.audio_track_count,
    "captionTrackCount": result.caption_track_count,
    "timelineDurationUs": result.timeline.duration.max(0),
    "inputArtifactIds": capcut_input.input_artifact_ids,
    "editingPreset": preset,
    "lintWarnings": result.lint_warnings,
    "source": "capcut_cli_rust_rebuild"
  });
  std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
  let stored = ArtifactStore::register_typed_artifact(work_dir, &request.job_id, StageId::Capcut, "capcut_cli_rust", ArtifactKind::CapcutDraft, &manifest_path, json!({"rebuild": true, "editing_preset": preset})).map_err(|error| error.to_string())?;
  let artifact_ref = stored.to_artifact_ref(StageId::Capcut).map_err(|error| error.to_string())?;
  context.artifact_refs.push(artifact_ref);
  if let Some(stage) = context.stage_states.iter_mut().find(|stage| stage.stage_id == StageId::Capcut) {
    stage.output_artifact_ids = vec![stored.id.clone()];
  }
  outputs["pipeline_context"] = serde_json::to_value(&context).map_err(|error| error.to_string())?;
  outputs["capcut_artifact"] = serde_json::to_value(&stored).map_err(|error| error.to_string())?;
  outputs["draft_manifest"] = manifest;
  outputs["draft_url"] = json!(draft_path);
  let serialized = serde_json::to_string(&outputs).map_err(|error| error.to_string())?;
  update_pipeline_job_stage(UpdatePipelineJobStageArgs { db: task_database.get_connection(), pipeline_job_id: &pipeline_job_id, current_stage: PipelineStage::DraftReady, maybe_stage_outputs: Some(&serialized), maybe_business_status: Some("READY_TO_POST") }).await.map_err(|error| error.to_string())?;
  update_pipeline_job_status(UpdatePipelineJobStatusArgs { db: task_database.get_connection(), pipeline_job_id: &pipeline_job_id, status: TaskStatus::CompleteSuccess, maybe_business_status: Some("READY_TO_POST") }).await.map_err(|error| error.to_string())?;

  Ok(CapcutDraftActionResponse { draft_id: result.draft_id, draft_path, engine: "capcut_cli_rust".into() })
}

#[derive(Debug, Deserialize)]
pub struct OpenCapcutDraftRequest {
  pub draft_path: String,
}

#[tauri::command]
pub fn open_floword_capcut_draft_command(app: AppHandle, request: OpenCapcutDraftRequest) -> Result<bool, String> {
  let path = PathBuf::from(request.draft_path.trim()).canonicalize().map_err(|error| format!("Không tìm thấy project CapCut: {error}"))?;
  if !path.is_dir() || !path.join("draft_content.json").is_file() {
    return Err("Đường dẫn không phải project CapCut hợp lệ".into());
  }

  if let Some(executable) = find_capcut_executable() {
    std::process::Command::new(executable).spawn().map_err(|error| format!("Không thể mở CapCut: {error}"))?;
  }
  app.opener().reveal_item_in_dir(path.clone()).map_err(|error| format!("Không thể mở thư mục project: {error}"))?;
  Ok(true)
}

fn find_capcut_executable() -> Option<PathBuf> {
  if let Ok(configured) = std::env::var("CAPCUT_DESKTOP_EXE") {
    let path = PathBuf::from(configured.trim());
    if path.is_file() {
      return Some(path);
    }
  }
  let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
  let direct = [local.join("Programs/CapCut/CapCut.exe"), local.join("CapCut/Apps/CapCut.exe"), local.join("JianyingPro/Apps/JianyingPro.exe")];
  if let Some(path) = direct.into_iter().find(|path| path.is_file()) {
    return Some(path);
  }
  for (root, exe_name) in [(local.join("CapCut/Apps"), "CapCut.exe"), (local.join("JianyingPro/Apps"), "JianyingPro.exe")] {
    let Ok(entries) = std::fs::read_dir(root) else {
      continue;
    };
    let mut versions = entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect::<Vec<_>>();
    versions.sort();
    if let Some(path) = versions.into_iter().rev().map(|dir| dir.join(exe_name)).find(|path| path.is_file()) {
      return Some(path);
    }
  }
  None
}
