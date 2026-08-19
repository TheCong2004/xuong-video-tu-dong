use super::content_page_publish_target::{raw_into_publish_target, ContentPagePublishTarget, RawContentPagePublishTarget};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use uuid::Uuid;

pub struct CreatePublishTargetArgs {
  pub page_id: String,
  pub platform: String,
  pub enabled: bool,
  pub account_label: Option<String>,
  pub destination_id: String,
  pub destination_handle: Option<String>,
  pub browser_profile_id: String,
  pub post_mode: Option<String>,
  pub default_slots_json: Option<String>,
}

pub async fn create_publish_target(
  db: &TaskDbConnection,
  args: CreatePublishTargetArgs,
) -> Result<ContentPagePublishTarget, SqliteTasksError> {
  let id = format!("pt_{}", Uuid::new_v4());
  let post_mode = args.post_mode.unwrap_or_else(|| "review".to_string());
  let default_slots = args.default_slots_json.unwrap_or_else(|| {
    "[\"08:30\", \"10:00\", \"17:00\", \"22:00\"]".to_string()
  });

  let raw: RawContentPagePublishTarget = sqlx::query_as(
    r#"
    INSERT INTO content_page_publish_targets (
      id, page_id, platform, enabled, account_label, destination_id, destination_handle,
      browser_profile_id, post_mode, default_slots_json
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
    ON CONFLICT(page_id, platform, destination_id) DO UPDATE SET
      enabled = excluded.enabled,
      account_label = excluded.account_label,
      destination_handle = excluded.destination_handle,
      browser_profile_id = excluded.browser_profile_id,
      post_mode = excluded.post_mode,
      default_slots_json = excluded.default_slots_json,
      updated_at = unixepoch('now')
    RETURNING id, page_id, platform, enabled, account_label, destination_id, destination_handle,
              browser_profile_id, post_mode, default_slots_json, created_at, updated_at
    "#,
  )
  .bind(&id)
  .bind(&args.page_id)
  .bind(&args.platform)
  .bind(args.enabled)
  .bind(&args.account_label)
  .bind(&args.destination_id)
  .bind(&args.destination_handle)
  .bind(&args.browser_profile_id)
  .bind(&post_mode)
  .bind(&default_slots)
  .fetch_one(db.get_pool())
  .await?;

  raw_into_publish_target(raw)
}
