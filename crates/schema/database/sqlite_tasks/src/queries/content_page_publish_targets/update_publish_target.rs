use super::content_page_publish_target::{raw_into_publish_target, ContentPagePublishTarget, RawContentPagePublishTarget};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct UpdatePublishTargetArgs {
  pub id: String,
  pub enabled: Option<bool>,
  pub account_label: Option<String>,
  pub destination_id: Option<String>,
  pub destination_handle: Option<String>,
  pub browser_profile_id: Option<String>,
  pub post_mode: Option<String>,
  pub default_slots_json: Option<String>,
}

pub async fn update_publish_target(
  db: &TaskDbConnection,
  args: UpdatePublishTargetArgs,
) -> Result<ContentPagePublishTarget, SqliteTasksError> {
  let existing: RawContentPagePublishTarget = sqlx::query_as(
    "SELECT id, page_id, platform, enabled, account_label, destination_id, destination_handle,
            browser_profile_id, post_mode, default_slots_json, created_at, updated_at
     FROM content_page_publish_targets WHERE id = $1",
  )
  .bind(&args.id)
  .fetch_optional(db.get_pool())
  .await?
  .ok_or_else(|| SqliteTasksError::Custom(format!("Publish target {} not found", args.id)))?;

  let enabled = args.enabled.map(|b| if b { 1 } else { 0 }).unwrap_or(existing.enabled);
  let account_label = args.account_label.or(existing.account_label);
  let destination_id = args.destination_id.unwrap_or(existing.destination_id);
  let destination_handle = args.destination_handle.or(existing.destination_handle);
  let browser_profile_id = args.browser_profile_id.unwrap_or(existing.browser_profile_id);
  let post_mode = args.post_mode.unwrap_or(existing.post_mode);
  let default_slots_json = args.default_slots_json.unwrap_or(existing.default_slots_json);

  let raw: RawContentPagePublishTarget = sqlx::query_as(
    r#"
    UPDATE content_page_publish_targets SET
      enabled = $1,
      account_label = $2,
      destination_id = $3,
      destination_handle = $4,
      browser_profile_id = $5,
      post_mode = $6,
      default_slots_json = $7,
      updated_at = unixepoch('now')
    WHERE id = $8
    RETURNING id, page_id, platform, enabled, account_label, destination_id, destination_handle,
              browser_profile_id, post_mode, default_slots_json, created_at, updated_at
    "#,
  )
  .bind(enabled)
  .bind(&account_label)
  .bind(&destination_id)
  .bind(&destination_handle)
  .bind(&browser_profile_id)
  .bind(&post_mode)
  .bind(&default_slots_json)
  .bind(&args.id)
  .fetch_one(db.get_pool())
  .await?;

  raw_into_publish_target(raw)
}
