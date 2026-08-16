use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use enums::tauri::pipeline::pipeline_stage::PipelineStage;
use sqlx::{QueryBuilder, Sqlite};
use tokens::tokens::sqlite::pipeline_jobs::PipelineJobId;

pub struct UpdatePipelineJobStageArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub pipeline_job_id: &'a PipelineJobId,
  pub current_stage: PipelineStage,
  pub maybe_stage_outputs: Option<&'a str>,
}

/// Advance a job to its next stage, storing the accumulated stage outputs.
/// Returns true if a row was updated.
pub async fn update_pipeline_job_stage(args: UpdatePipelineJobStageArgs<'_>) -> Result<bool, SqliteTasksError> {
  let current_stage = args.current_stage.to_str().to_string();
  let stage_outputs = args.maybe_stage_outputs.map(|s| s.to_string());
  let id = args.pipeline_job_id.as_str().to_string();

  let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
    r#"
    UPDATE pipeline_jobs
    SET current_stage =
  "#,
  );
  query_builder.push_bind(current_stage);
  query_builder.push(", stage_outputs = COALESCE(");
  query_builder.push_bind(stage_outputs);
  query_builder.push(", stage_outputs)");
  query_builder.push(", updated_at = unixepoch('now') WHERE id = ");
  query_builder.push_bind(id);

  let query = query_builder.build();
  let res = query.execute(args.db.get_pool()).await?;

  Ok(res.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::queries::pipeline::create_pipeline_job::{create_pipeline_job, CreatePipelineJobArgs};
  use crate::queries::pipeline::get_pipeline_job_by_id::{get_pipeline_job_by_id, GetPipelineJobByIdArgs};
  use enums::tauri::tasks::task_status::TaskStatus;

  #[test]
  fn stage_only_update_preserves_persisted_artifact_outputs() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
      let temp = tempfile::tempdir().unwrap();
      let db = TaskDbConnection::connect_and_migrate(temp.path().join("tasks.sqlite")).await.unwrap();
      let id = create_pipeline_job(CreatePipelineJobArgs { db: &db, status: TaskStatus::Pending, current_stage: PipelineStage::IngestAnalyze, maybe_input_payload: None }).await.unwrap();
      let outputs = r#"{"ingest_analyze":{"artifact_ids":["art_source_video"]}}"#;

      update_pipeline_job_stage(UpdatePipelineJobStageArgs { db: &db, pipeline_job_id: &id, current_stage: PipelineStage::IngestAnalyze, maybe_stage_outputs: Some(outputs) }).await.unwrap();
      update_pipeline_job_stage(UpdatePipelineJobStageArgs { db: &db, pipeline_job_id: &id, current_stage: PipelineStage::PreflightCheck, maybe_stage_outputs: None }).await.unwrap();

      let job = get_pipeline_job_by_id(GetPipelineJobByIdArgs { db: &db, pipeline_job_id: &id }).await.unwrap().unwrap();
      assert_eq!(job.maybe_stage_outputs.as_deref(), Some(outputs));
    });
  }
}
