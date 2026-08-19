use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use enums::tauri::tasks::task_status::TaskStatus;
use sqlx::{QueryBuilder, Sqlite};
use tokens::tokens::sqlite::pipeline_jobs::PipelineJobId;

pub struct UpdatePipelineJobStatusArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub pipeline_job_id: &'a PipelineJobId,
  pub status: TaskStatus,
  pub maybe_business_status: Option<&'a str>,
}

/// Set a job's status. Terminal statuses also stamp `completed_at`.
/// If starting for the first time, stamps `started_at`.
/// Returns true if a row was updated.
pub async fn update_pipeline_job_status(args: UpdatePipelineJobStatusArgs<'_>) -> Result<bool, SqliteTasksError> {
  let is_terminal = matches!(args.status, TaskStatus::CompleteSuccess | TaskStatus::CompleteFailure | TaskStatus::Dead | TaskStatus::CancelledByUser | TaskStatus::CancelledByProvider | TaskStatus::CancelledByUs);
  let is_started = matches!(args.status, TaskStatus::Started);

  let status = args.status.to_str().to_string();
  let id = args.pipeline_job_id.as_str().to_string();
  let maybe_biz = args.maybe_business_status.map(|s| s.to_string());

  let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
    r#"
    UPDATE pipeline_jobs
    SET status =
  "#,
  );
  query_builder.push_bind(status);
  query_builder.push(", updated_at = unixepoch('now')");
  if let Some(biz) = maybe_biz {
    query_builder.push(", business_status = ");
    query_builder.push_bind(biz);
  }
  if is_started {
    query_builder.push(", started_at = COALESCE(started_at, unixepoch('now'))");
  }
  if is_terminal {
    query_builder.push(", completed_at = unixepoch('now')");
  }
  query_builder.push(" WHERE id = ");
  query_builder.push_bind(id);

  let query = query_builder.build();
  let res = query.execute(args.db.get_pool()).await?;

  Ok(res.rows_affected() > 0)
}
