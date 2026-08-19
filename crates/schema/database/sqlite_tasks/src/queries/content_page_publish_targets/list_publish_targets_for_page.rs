use super::content_page_publish_target::{raw_into_publish_target, ContentPagePublishTarget, RawContentPagePublishTarget};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub async fn list_publish_targets_for_page(
  db: &TaskDbConnection,
  page_id: &str,
) -> Result<Vec<ContentPagePublishTarget>, SqliteTasksError> {
  let rows: Vec<RawContentPagePublishTarget> = sqlx::query_as(
    r#"
    SELECT id, page_id, platform, enabled, account_label, destination_id, destination_handle,
           browser_profile_id, post_mode, default_slots_json, created_at, updated_at
    FROM content_page_publish_targets
    WHERE page_id = $1
    ORDER BY created_at ASC
    "#,
  )
  .bind(page_id)
  .fetch_all(db.get_pool())
  .await?;

  rows.into_iter().map(raw_into_publish_target).collect()
}
