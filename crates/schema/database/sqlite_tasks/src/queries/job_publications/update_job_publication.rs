use super::job_publication::{raw_into_job_publication, JobPublication, RawJobPublication};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct UpdateJobPublicationArgs {
  pub id: String,
  pub status: Option<String>,
  pub scheduled_at: Option<i64>,
  pub approved_at: Option<i64>,
  pub posted_at: Option<i64>,
  pub platform_post_id: Option<String>,
  pub post_url: Option<String>,
  pub title: Option<String>,
  pub caption: Option<String>,
  pub last_error_code: Option<String>,
  pub last_error_message: Option<String>,
}

pub async fn update_job_publication(
  db: &TaskDbConnection,
  args: UpdateJobPublicationArgs,
) -> Result<JobPublication, SqliteTasksError> {
  let existing: RawJobPublication = sqlx::query_as(
    "SELECT id, job_id, page_id, platform, target_config_id, browser_profile_id, status,
            scheduled_at, approved_at, started_at, posted_at, attempt_count, idempotency_key,
            platform_post_id, post_url, title, caption, hashtags_json, description, video_path,
            last_error_code, last_error_message, created_at, updated_at
     FROM job_publications WHERE id = $1",
  )
  .bind(&args.id)
  .fetch_optional(db.get_pool())
  .await?
  .ok_or_else(|| SqliteTasksError::Custom(format!("Job publication {} not found", args.id)))?;

  let status = args.status.unwrap_or(existing.status);
  let scheduled_at = args.scheduled_at.or(existing.scheduled_at);
  let approved_at = args.approved_at.or(existing.approved_at);
  let posted_at = args.posted_at.or(existing.posted_at);
  let platform_post_id = args.platform_post_id.or(existing.platform_post_id);
  let post_url = args.post_url.or(existing.post_url);
  let title = args.title.or(existing.title);
  let caption = args.caption.or(existing.caption);
  let last_error_code = args.last_error_code.or(existing.last_error_code);
  let last_error_message = args.last_error_message.or(existing.last_error_message);

  let raw: RawJobPublication = sqlx::query_as(
    r#"
    UPDATE job_publications SET
      status = $1,
      scheduled_at = $2,
      approved_at = $3,
      posted_at = $4,
      platform_post_id = $5,
      post_url = $6,
      title = $7,
      caption = $8,
      last_error_code = $9,
      last_error_message = $10,
      updated_at = unixepoch('now')
    WHERE id = $11
    RETURNING id, job_id, page_id, platform, target_config_id, browser_profile_id, status,
              scheduled_at, approved_at, started_at, posted_at, attempt_count, idempotency_key,
              platform_post_id, post_url, title, caption, hashtags_json, description, video_path,
              last_error_code, last_error_message, created_at, updated_at
    "#,
  )
  .bind(&status)
  .bind(scheduled_at)
  .bind(approved_at)
  .bind(posted_at)
  .bind(&platform_post_id)
  .bind(&post_url)
  .bind(&title)
  .bind(&caption)
  .bind(&last_error_code)
  .bind(&last_error_message)
  .bind(&args.id)
  .fetch_one(db.get_pool())
  .await?;

  raw_into_job_publication(raw)
}
