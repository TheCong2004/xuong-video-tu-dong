use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct ArchiveContentPageArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub id: &'a str,
  pub is_archived: bool,
}

pub async fn archive_content_page(args: ArchiveContentPageArgs<'_>) -> Result<bool, SqliteTasksError> {
  let archived_val: i64 = if args.is_archived { 1 } else { 0 };

  let result = sqlx::query(
    "UPDATE content_pages SET is_archived = ?1, updated_at = unixepoch('now') WHERE id = ?2"
  )
  .bind(archived_val)
  .bind(args.id)
  .execute(args.db.get_pool())
  .await?;

  Ok(result.rows_affected() > 0)
}
