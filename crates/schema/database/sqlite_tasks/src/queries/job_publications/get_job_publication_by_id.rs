use super::job_publication::{raw_into_job_publication, JobPublication, RawJobPublication};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub async fn get_job_publication_by_id(
  db: &TaskDbConnection,
  id: &str,
) -> Result<Option<JobPublication>, SqliteTasksError> {
  let maybe_raw: Option<RawJobPublication> = sqlx::query_as(
    r#"
    SELECT id, job_id, page_id, platform, target_config_id, browser_profile_id, status,
           scheduled_at, approved_at, started_at, posted_at, attempt_count, idempotency_key,
           platform_post_id, post_url, title, caption, hashtags_json, description, video_path,
           last_error_code, last_error_message, created_at, updated_at
    FROM job_publications
    WHERE id = $1
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

pub async fn get_job_publication_by_idempotency_key(
  db: &TaskDbConnection,
  key: &str,
) -> Result<Option<JobPublication>, SqliteTasksError> {
  let maybe_raw: Option<RawJobPublication> = sqlx::query_as(
    r#"
    SELECT id, job_id, page_id, platform, target_config_id, browser_profile_id, status,
           scheduled_at, approved_at, started_at, posted_at, attempt_count, idempotency_key,
           platform_post_id, post_url, title, caption, hashtags_json, description, video_path,
           last_error_code, last_error_message, created_at, updated_at
    FROM job_publications
    WHERE idempotency_key = $1
    "#,
  )
  .bind(key)
  .fetch_optional(db.get_pool())
  .await?;

  match maybe_raw {
    Some(raw) => Ok(Some(raw_into_job_publication(raw)?)),
    None => Ok(None),
  }
}
