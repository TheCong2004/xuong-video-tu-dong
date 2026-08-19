use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use crate::queries::content_pages::content_page::{raw_into_content_page, ContentPage, RawContentPage};

pub struct UpdateContentPageArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub id: &'a str,
  pub name: &'a str,
  pub slug: Option<&'a str>,
  pub output_root: &'a str,
  pub target_platform: Option<&'a str>,
  pub default_model_id: Option<&'a str>,
  pub default_workflow_id: Option<&'a str>,
  pub default_language: Option<&'a str>,
  pub default_tone: Option<&'a str>,
  pub default_aspect_ratio: Option<&'a str>,
  pub browser_profile_id: Option<&'a str>,
  pub worker_pool_id: Option<&'a str>,
  pub default_image_prompt: Option<&'a str>,
  pub default_expand_9_16_prompt: Option<&'a str>,
  pub default_video_prompt: Option<&'a str>,
}

pub async fn update_content_page(args: UpdateContentPageArgs<'_>) -> Result<ContentPage, SqliteTasksError> {
  let slug = args.slug.map(|s| s.to_string()).unwrap_or_else(|| {
    args.name.to_lowercase().replace(' ', "-").replace(|c: char| !c.is_alphanumeric() && c != '-', "")
  });

  sqlx::query(
    r#"
    UPDATE content_pages SET
      name = ?1,
      slug = ?2,
      output_root = ?3,
      target_platform = ?4,
      default_model_id = ?5,
      default_workflow_id = ?6,
      default_language = ?7,
      default_tone = ?8,
      default_aspect_ratio = ?9,
      browser_profile_id = ?10,
      worker_pool_id = ?11,
      default_image_prompt = ?12,
      default_expand_9_16_prompt = ?13,
      default_video_prompt = ?14,
      updated_at = unixepoch('now')
    WHERE id = ?15
    "#,
  )
  .bind(args.name)
  .bind(&slug)
  .bind(args.output_root)
  .bind(args.target_platform)
  .bind(args.default_model_id)
  .bind(args.default_workflow_id)
  .bind(args.default_language)
  .bind(args.default_tone)
  .bind(args.default_aspect_ratio)
  .bind(args.browser_profile_id)
  .bind(args.worker_pool_id)
  .bind(args.default_image_prompt)
  .bind(args.default_expand_9_16_prompt)
  .bind(args.default_video_prompt)
  .bind(args.id)
  .execute(args.db.get_pool())
  .await?;

  let raw = sqlx::query_as::<_, RawContentPage>(
    "SELECT id, name, slug, output_root, target_platform, default_model_id, default_workflow_id, default_language, default_tone, default_aspect_ratio, browser_profile_id, worker_pool_id, default_image_prompt, default_expand_9_16_prompt, default_video_prompt, is_archived, created_at, updated_at FROM content_pages WHERE id = ?1 LIMIT 1"
  )
  .bind(args.id)
  .fetch_one(args.db.get_pool())
  .await?;

  raw_into_content_page(raw)
}
