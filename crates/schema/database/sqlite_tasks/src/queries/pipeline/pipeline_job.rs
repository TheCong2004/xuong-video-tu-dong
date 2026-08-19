use crate::error::SqliteTasksError;
use enums::tauri::pipeline::pipeline_stage::PipelineStage;
use enums::tauri::tasks::task_status::TaskStatus;
use serde::{Deserialize, Serialize};
use tokens::tokens::sqlite::pipeline_jobs::PipelineJobId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineJob {
  pub id: PipelineJobId,
  pub status: TaskStatus,
  pub current_stage: PipelineStage,
  pub maybe_page_id: Option<String>,
  pub maybe_input_payload: Option<String>,
  pub maybe_stage_outputs: Option<String>,
  pub maybe_on_failure_message: Option<String>,
  pub maybe_page_snapshot: Option<String>,
  pub maybe_business_status: Option<String>,
  pub maybe_started_at: Option<i64>,
  pub maybe_failure_code: Option<String>,
  pub maybe_failure_stage: Option<String>,
  pub created_at: i64,
  pub updated_at: i64,
  pub maybe_completed_at: Option<i64>,
}

#[derive(Debug)]
#[derive(sqlx::FromRow)]
pub struct RawPipelineJob {
  pub id: String,
  pub status: String,
  pub current_stage: String,
  pub page_id: Option<String>,
  pub input_payload: Option<String>,
  pub stage_outputs: Option<String>,
  pub on_failure_message: Option<String>,
  pub page_snapshot: Option<String>,
  pub business_status: Option<String>,
  pub started_at: Option<i64>,
  pub failure_code: Option<String>,
  pub failure_stage: Option<String>,
  pub created_at: i64,
  pub updated_at: i64,
  pub completed_at: Option<i64>,
}

pub(crate) fn raw_into_pipeline_job(raw: RawPipelineJob) -> Result<PipelineJob, SqliteTasksError> {
  Ok(PipelineJob {
    id: PipelineJobId::new_from_str(&raw.id),
    status: TaskStatus::from_str(&raw.status)?,
    current_stage: PipelineStage::from_str(&raw.current_stage)?,
    maybe_page_id: raw.page_id,
    maybe_input_payload: raw.input_payload,
    maybe_stage_outputs: raw.stage_outputs,
    maybe_on_failure_message: raw.on_failure_message,
    maybe_page_snapshot: raw.page_snapshot,
    maybe_business_status: raw.business_status,
    maybe_started_at: raw.started_at,
    maybe_failure_code: raw.failure_code,
    maybe_failure_stage: raw.failure_stage,
    created_at: raw.created_at,
    updated_at: raw.updated_at,
    maybe_completed_at: raw.completed_at,
  })
}
