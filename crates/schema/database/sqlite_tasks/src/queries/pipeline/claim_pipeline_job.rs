use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use tokens::tokens::sqlite::pipeline_jobs::PipelineJobId;

pub struct ClaimPipelineJobArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub pipeline_job_id: &'a PipelineJobId,
}

/// Atomically claim a pending or restarted job:
/// Sets status = Started, started_at = COALESCE(started_at, now),
/// business_status = 'WAITING_WORKER' (or running state).
/// Returns true only if the claim succeeded.
pub async fn claim_pipeline_job(args: ClaimPipelineJobArgs<'_>) -> Result<bool, SqliteTasksError> {
  let id = args.pipeline_job_id.as_str();

  let res = sqlx::query(
    r#"
    UPDATE pipeline_jobs
    SET
      status = 'started',
      started_at = COALESCE(started_at, unixepoch('now')),
      business_status = COALESCE(business_status, 'WAITING_WORKER'),
      updated_at = unixepoch('now')
    WHERE id = ?1 AND status IN ('pending', 'started')
    "#,
  )
  .bind(id)
  .execute(args.db.get_pool())
  .await?;

  Ok(res.rows_affected() > 0)
}
