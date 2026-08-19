use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineJobEvent {
  pub id: String,
  pub job_id: String,
  pub sequence: i64,
  pub stage_id: Option<String>,
  pub business_status: Option<String>,
  pub event_type: String,
  pub level: String,
  pub message: String,
  pub error_code: Option<String>,
  pub metadata_json: Option<String>,
  pub created_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct RawPipelineJobEvent {
  pub id: String,
  pub job_id: String,
  pub sequence: i64,
  pub stage_id: Option<String>,
  pub business_status: Option<String>,
  pub event_type: String,
  pub level: String,
  pub message: String,
  pub error_code: Option<String>,
  pub metadata_json: Option<String>,
  pub created_at: i64,
}

pub(crate) fn raw_into_pipeline_job_event(raw: RawPipelineJobEvent) -> PipelineJobEvent {
  PipelineJobEvent {
    id: raw.id,
    job_id: raw.job_id,
    sequence: raw.sequence,
    stage_id: raw.stage_id,
    business_status: raw.business_status,
    event_type: raw.event_type,
    level: raw.level,
    message: raw.message,
    error_code: raw.error_code,
    metadata_json: raw.metadata_json,
    created_at: raw.created_at,
  }
}
