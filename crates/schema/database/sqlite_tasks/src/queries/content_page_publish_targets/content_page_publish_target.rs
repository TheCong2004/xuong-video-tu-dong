use crate::error::SqliteTasksError;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContentPagePublishTarget {
  pub id: String,
  pub page_id: String,
  pub platform: String,
  pub enabled: bool,
  pub account_label: Option<String>,
  pub destination_id: String,
  pub destination_handle: Option<String>,
  pub browser_profile_id: String,
  pub post_mode: String,
  pub default_slots_json: String,
  pub created_at: i64,
  pub updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct RawContentPagePublishTarget {
  pub id: String,
  pub page_id: String,
  pub platform: String,
  pub enabled: i64,
  pub account_label: Option<String>,
  pub destination_id: String,
  pub destination_handle: Option<String>,
  pub browser_profile_id: String,
  pub post_mode: String,
  pub default_slots_json: String,
  pub created_at: i64,
  pub updated_at: i64,
}

pub(crate) fn raw_into_publish_target(raw: RawContentPagePublishTarget) -> Result<ContentPagePublishTarget, SqliteTasksError> {
  Ok(ContentPagePublishTarget {
    id: raw.id,
    page_id: raw.page_id,
    platform: raw.platform,
    enabled: raw.enabled != 0,
    account_label: raw.account_label,
    destination_id: raw.destination_id,
    destination_handle: raw.destination_handle,
    browser_profile_id: raw.browser_profile_id,
    post_mode: raw.post_mode,
    default_slots_json: raw.default_slots_json,
    created_at: raw.created_at,
    updated_at: raw.updated_at,
  })
}
