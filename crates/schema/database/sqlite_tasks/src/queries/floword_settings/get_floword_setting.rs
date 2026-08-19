use super::floword_setting::FlowordSetting;
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub async fn get_floword_setting(
  db: &TaskDbConnection,
  key: &str,
) -> Result<Option<FlowordSetting>, SqliteTasksError> {
  let setting: Option<FlowordSetting> = sqlx::query_as(
    "SELECT key, value_json, updated_at FROM floword_system_settings WHERE key = $1",
  )
  .bind(key)
  .fetch_optional(db.get_pool())
  .await?;

  Ok(setting)
}
