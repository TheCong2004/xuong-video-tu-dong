use super::job_publication::{raw_into_job_publication, JobPublication, RawJobPublication};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub async fn claim_job_publication(
  db: &TaskDbConnection,
  id: &str,
) -> Result<Option<JobPublication>, SqliteTasksError> {
  let maybe_raw: Option<RawJobPublication> = sqlx::query_as(
    r#"
    UPDATE job_publications
    SET status = 'POSTING',
        started_at = unixepoch('now'),
        attempt_count = attempt_count + 1,
        updated_at = unixepoch('now')
    WHERE id = $1
      AND (status = 'READY_TO_POST' OR (status = 'SCHEDULED' AND scheduled_at <= unixepoch('now')))
    RETURNING id, job_id, page_id, platform, target_config_id, browser_profile_id, status,
              scheduled_at, approved_at, started_at, posted_at, attempt_count, idempotency_key,
              platform_post_id, post_url, title, caption, hashtags_json, description, video_path,
              last_error_code, last_error_message, created_at, updated_at
    "#,
  )
  .bind(id)
  .fetch_optional(db.get_pool())
  .await?;

  match maybe_raw {
    Some(raw) => Ok(Some(raw_into_job_publication(raw)?)),
    None => Ok(None),
  }
}
