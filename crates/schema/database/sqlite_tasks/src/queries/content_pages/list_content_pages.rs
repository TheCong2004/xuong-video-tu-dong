use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use crate::queries::content_pages::content_page::{raw_into_content_page, ContentPage, RawContentPage};

pub struct ListContentPagesArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub include_archived: bool,
}

pub struct ContentPageList {
  pub pages: Vec<ContentPage>,
}

pub async fn list_content_pages(args: ListContentPagesArgs<'_>) -> Result<ContentPageList, SqliteTasksError> {
  let sql = if args.include_archived {
    "SELECT id, name, slug, output_root, target_platform, default_model_id, default_workflow_id, default_language, default_tone, default_aspect_ratio, browser_profile_id, default_image_prompt, default_expand_9_16_prompt, default_video_prompt, is_archived, created_at, updated_at FROM content_pages ORDER BY updated_at DESC"
  } else {
    "SELECT id, name, slug, output_root, target_platform, default_model_id, default_workflow_id, default_language, default_tone, default_aspect_ratio, browser_profile_id, default_image_prompt, default_expand_9_16_prompt, default_video_prompt, is_archived, created_at, updated_at FROM content_pages WHERE is_archived = 0 ORDER BY updated_at DESC"
  };

  let results = sqlx::query_as::<_, RawContentPage>(sql)
    .fetch_all(args.db.get_pool())
    .await?;

  let mut pages = Vec::new();
  for raw in results {
    pages.push(raw_into_content_page(raw)?);
  }

  Ok(ContentPageList { pages })
}
