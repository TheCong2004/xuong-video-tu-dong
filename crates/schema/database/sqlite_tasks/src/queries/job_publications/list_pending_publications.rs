use super::job_publication::{raw_into_job_publication, JobPublication, RawJobPublication};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub async fn list_pending_publications(
  db: &TaskDbConnection,
  limit: i64,
) -> Result<Vec<JobPublication>, SqliteTasksError> {
  let rows: Vec<RawJobPublication> = sqlx::query_as(
    r#"
    SELECT id, job_id, page_id, platform, target_config_id, browser_profile_id, status,
           scheduled_at, approved_at, started_at, posted_at, attempt_count, idempotency_key,
           platform_post_id, post_url, title, caption, hashtags_json, description, video_path,
           last_error_code, last_error_message, created_at, updated_at
    FROM job_publications
    WHERE status = 'READY_TO_POST'
       OR (status = 'SCHEDULED' AND scheduled_at IS NOT NULL AND scheduled_at <= unixepoch('now'))
    ORDER BY created_at ASC
    LIMIT $1
    "#,
  )
  .bind(limit)
  .fetch_all(db.get_pool())
  .await?;

  rows.into_iter().map(raw_into_job_publication).collect()
}
