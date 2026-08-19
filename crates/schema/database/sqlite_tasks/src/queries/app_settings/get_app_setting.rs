use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct GetAppSettingArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub key: &'a str,
}

pub async fn get_app_setting(args: GetAppSettingArgs<'_>) -> Result<Option<String>, SqliteTasksError> {
  let row = sqlx::query_scalar::<_, String>(
    "SELECT value FROM app_settings WHERE key = ?1 LIMIT 1",
  )
  .bind(args.key)
  .fetch_optional(args.db.get_pool())
  .await?;

  Ok(row)
}
