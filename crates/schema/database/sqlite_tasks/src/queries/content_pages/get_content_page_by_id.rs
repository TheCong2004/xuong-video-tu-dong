use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use crate::queries::content_pages::content_page::{raw_into_content_page, ContentPage, RawContentPage};

pub struct GetContentPageByIdArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub id: &'a str,
}

pub async fn get_content_page_by_id(args: GetContentPageByIdArgs<'_>) -> Result<Option<ContentPage>, SqliteTasksError> {
  let query = sqlx::query_as::<_, RawContentPage>(
    "SELECT id, name, slug, output_root, target_platform, default_model_id, default_workflow_id, default_language, default_tone, default_aspect_ratio, browser_profile_id, is_archived, created_at, updated_at FROM content_pages WHERE id = ?1 LIMIT 1"
  ).bind(args.id);

  let maybe_raw = query.fetch_optional(args.db.get_pool()).await?;
  match maybe_raw {
    Some(raw) => Ok(Some(raw_into_content_page(raw)?)),
    None => Ok(None),
  }
}
