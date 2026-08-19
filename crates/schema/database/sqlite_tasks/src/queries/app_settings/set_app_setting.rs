use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct SetAppSettingArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub key: &'a str,
  pub value: &'a str,
}

pub async fn set_app_setting(args: SetAppSettingArgs<'_>) -> Result<(), SqliteTasksError> {
  sqlx::query(
    r#"
    INSERT INTO app_settings (key, value, updated_at)
    VALUES (?1, ?2, unixepoch('now'))
    ON CONFLICT(key) DO UPDATE SET
      value = excluded.value,
      updated_at = unixepoch('now')
    "#,
  )
  .bind(args.key)
  .bind(args.value)
  .execute(args.db.get_pool())
  .await?;

  Ok(())
}
