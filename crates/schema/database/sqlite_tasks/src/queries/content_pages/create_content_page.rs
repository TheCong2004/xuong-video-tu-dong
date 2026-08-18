use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use crate::queries::content_pages::content_page::{raw_into_content_page, ContentPage, RawContentPage};
use sqlx::{QueryBuilder, Sqlite};

pub struct CreateContentPageArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub id: Option<&'a str>,
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
  pub default_image_prompt: Option<&'a str>,
  pub default_expand_9_16_prompt: Option<&'a str>,
  pub default_video_prompt: Option<&'a str>,
}

pub async fn create_content_page(args: CreateContentPageArgs<'_>) -> Result<ContentPage, SqliteTasksError> {
  let id = args.id.map(|s| s.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
  let slug = args.slug.map(|s| s.to_string()).unwrap_or_else(|| {
    args.name.to_lowercase().replace(' ', "-").replace(|c: char| !c.is_alphanumeric() && c != '-', "")
  });

  let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
    r#"
    INSERT INTO content_pages (
      id,
      name,
      slug,
      output_root,
      target_platform,
      default_model_id,
      default_workflow_id,
      default_language,
      default_tone,
      default_aspect_ratio,
      browser_profile_id,
      default_image_prompt,
      default_expand_9_16_prompt,
      default_video_prompt,
      is_archived
    )
    VALUES (
  "#,
  );

  let mut separated = query_builder.separated(", ");
  separated.push_bind(&id);
  separated.push_bind(args.name);
  separated.push_bind(&slug);
  separated.push_bind(args.output_root);
  separated.push_bind(args.target_platform);
  separated.push_bind(args.default_model_id);
  separated.push_bind(args.default_workflow_id);
  separated.push_bind(args.default_language);
  separated.push_bind(args.default_tone);
  separated.push_bind(args.default_aspect_ratio);
  separated.push_bind(args.browser_profile_id);
  separated.push_bind(args.default_image_prompt);
  separated.push_bind(args.default_expand_9_16_prompt);
  separated.push_bind(args.default_video_prompt);
  separated.push_bind(0);
  separated.push_unseparated(")");

  let query = query_builder.build();
  query.execute(args.db.get_pool()).await?;

  let fetch_query = sqlx::query_as::<_, RawContentPage>(
    "SELECT id, name, slug, output_root, target_platform, default_model_id, default_workflow_id, default_language, default_tone, default_aspect_ratio, browser_profile_id, default_image_prompt, default_expand_9_16_prompt, default_video_prompt, is_archived, created_at, updated_at FROM content_pages WHERE id = ?1 LIMIT 1"
  ).bind(&id);

  let raw = fetch_query.fetch_one(args.db.get_pool()).await?;
  raw_into_content_page(raw)
}
