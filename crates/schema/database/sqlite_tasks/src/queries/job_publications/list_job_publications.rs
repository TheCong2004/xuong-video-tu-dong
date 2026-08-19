use super::job_publication::{raw_into_job_publication, JobPublication, RawJobPublication};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct ListJobPublicationsArgs {
  pub job_id: Option<String>,
  pub page_id: Option<String>,
  pub platform: Option<String>,
  pub status: Option<String>,
  pub limit: Option<i64>,
}

pub async fn list_job_publications(
  db: &TaskDbConnection,
  args: ListJobPublicationsArgs,
) -> Result<Vec<JobPublication>, SqliteTasksError> {
  let limit = args.limit.unwrap_or(100);

  let rows: Vec<RawJobPublication> = sqlx::query_as(
    r#"
    SELECT id, job_id, page_id, platform, target_config_id, browser_profile_id, status,
           scheduled_at, approved_at, started_at, posted_at, attempt_count, idempotency_key,
           platform_post_id, post_url, title, caption, hashtags_json, description, video_path,
           last_error_code, last_error_message, created_at, updated_at
    FROM job_publications
    WHERE ($1 IS NULL OR job_id = $1)
      AND ($2 IS NULL OR page_id = $2)
      AND ($3 IS NULL OR platform = $3)
      AND ($4 IS NULL OR status = $4)
    ORDER BY created_at DESC
    LIMIT $5
    "#,
  )
  .bind(&args.job_id)
  .bind(&args.page_id)
  .bind(&args.platform)
  .bind(&args.status)
  .bind(limit)
  .fetch_all(db.get_pool())
  .await?;

  rows.into_iter().map(raw_into_job_publication).collect()
}
