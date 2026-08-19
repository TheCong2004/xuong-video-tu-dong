use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub async fn delete_publish_target(
  db: &TaskDbConnection,
  id: &str,
) -> Result<(), SqliteTasksError> {
  sqlx::query("DELETE FROM content_page_publish_targets WHERE id = $1")
    .bind(id)
    .execute(db.get_pool())
    .await?;

  Ok(())
}
