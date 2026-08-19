use super::job_publication::{raw_into_job_publication, JobPublication, RawJobPublication};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use uuid::Uuid;

pub struct CreateJobPublicationArgs {
  pub job_id: String,
  pub page_id: String,
  pub platform: String,
  pub target_config_id: Option<String>,
  pub browser_profile_id: String,
  pub status: String,
  pub scheduled_at: Option<i64>,
  pub approved_at: Option<i64>,
  pub idempotency_key: String,
  pub title: Option<String>,
  pub caption: Option<String>,
  pub hashtags_json: Option<String>,
  pub description: Option<String>,
  pub video_path: Option<String>,
}

pub async fn create_job_publication(
  db: &TaskDbConnection,
  args: CreateJobPublicationArgs,
) -> Result<JobPublication, SqliteTasksError> {
  let id = format!("pub_{}", Uuid::new_v4());

  let raw: RawJobPublication = sqlx::query_as(
    r#"
    INSERT INTO job_publications (
      id, job_id, page_id, platform, target_config_id, browser_profile_id, status,
      scheduled_at, approved_at, idempotency_key, title, caption, hashtags_json,
      description, video_path
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
    ON CONFLICT(idempotency_key) DO UPDATE SET
      video_path = COALESCE(excluded.video_path, job_publications.video_path),
      title = COALESCE(excluded.title, job_publications.title),
      caption = COALESCE(excluded.caption, job_publications.caption),
      updated_at = unixepoch('now')
    RETURNING id, job_id, page_id, platform, target_config_id, browser_profile_id, status,
              scheduled_at, approved_at, started_at, posted_at, attempt_count, idempotency_key,
              platform_post_id, post_url, title, caption, hashtags_json, description, video_path,
              last_error_code, last_error_message, created_at, updated_at
    "#,
  )
  .bind(&id)
  .bind(&args.job_id)
  .bind(&args.page_id)
  .bind(&args.platform)
  .bind(&args.target_config_id)
  .bind(&args.browser_profile_id)
  .bind(&args.status)
  .bind(args.scheduled_at)
  .bind(args.approved_at)
  .bind(&args.idempotency_key)
  .bind(&args.title)
  .bind(&args.caption)
  .bind(&args.hashtags_json)
  .bind(&args.description)
  .bind(&args.video_path)
  .fetch_one(db.get_pool())
  .await?;

  raw_into_job_publication(raw)
}
