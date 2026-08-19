use super::floword_setting::FlowordSetting;
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct UpsertFlowordSettingArgs {
  pub key: String,
  pub value_json: String,
}

pub async fn upsert_floword_setting(
  db: &TaskDbConnection,
  args: UpsertFlowordSettingArgs,
) -> Result<FlowordSetting, SqliteTasksError> {
  let setting: FlowordSetting = sqlx::query_as(
    r#"
    INSERT INTO floword_system_settings (key, value_json, updated_at)
    VALUES ($1, $2, unixepoch('now'))
    ON CONFLICT (key) DO UPDATE SET
      value_json = excluded.value_json,
      updated_at = unixepoch('now')
    RETURNING key, value_json, updated_at
    "#,
  )
  .bind(&args.key)
  .bind(&args.value_json)
  .fetch_one(db.get_pool())
  .await?;

  Ok(setting)
}
