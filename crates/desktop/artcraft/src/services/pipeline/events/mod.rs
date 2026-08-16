//! Pipeline lifecycle events emitted to the frontend.

use log::warn;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::contracts::StageState;

pub const STAGE_STATE_EVENT: &str = "pipeline://stage_state";
pub const STAGE_COMPLETE_EVENT: &str = "pipeline://stage_complete";
pub const JOB_COMPLETE_EVENT: &str = "pipeline://job_complete";
pub const JOB_FAILED_EVENT: &str = "pipeline://job_failed";

#[derive(Clone, Debug, Serialize)]
pub struct StageCompletePayload {
  pub job_id: String,
  pub completed_stage: String,
  pub next_stage: String,
  pub progress: u32,
  pub stage_message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StageStatePayload {
  pub job_id: String,
  pub stage: StageState,
  pub progress: Option<u8>,
  pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JobCompletePayload {
  pub job_id: String,
  pub result_type: String, // "draft" | "video"
  pub stage: String,
  pub progress: u32,
  pub draft_url: String,
  pub video_url: Option<String>,
  pub rendering_supported: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct JobFailedPayload {
  pub job_id: String,
  pub failed_stage: String,
  pub error_code: String,
  pub error_message: String,
}

pub fn emit_stage_complete(app: &AppHandle, payload: StageCompletePayload) {
  emit(app, STAGE_COMPLETE_EVENT, payload);
}

pub fn emit_stage_state(app: &AppHandle, payload: StageStatePayload) {
  emit(app, STAGE_STATE_EVENT, payload);
}

pub fn emit_job_complete(app: &AppHandle, payload: JobCompletePayload) {
  emit(app, JOB_COMPLETE_EVENT, payload);
}

pub fn emit_job_failed(app: &AppHandle, payload: JobFailedPayload) {
  emit(app, JOB_FAILED_EVENT, payload);
}

fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
  if let Err(err) = app.emit(event, payload) {
    warn!("Failed to emit {event}: {err}");
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::contracts::{StageId, StageStatus};

  #[test]
  fn stage_state_payload_serializes_business_stage_contract() {
    let mut stage = StageState::pending(StageId::Voice);
    stage.start_stage(Some("tts".to_string()), "2026-01-01T00:00:00Z").unwrap();
    let payload = StageStatePayload { job_id: "job-1".to_string(), stage, progress: Some(60), message: Some("Generating narration".to_string()) };

    let json = serde_json::to_value(payload).unwrap();
    assert_eq!(json["stage"]["stage_id"], "voice");
    assert_eq!(json["stage"]["status"], "running");
    assert_eq!(json["progress"], 60);
    assert_eq!(StageStatus::Running, serde_json::from_value(json["stage"]["status"].clone()).unwrap());
  }
}
