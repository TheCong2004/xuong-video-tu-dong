use crate::error::SqliteTasksError;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPage {
  pub id: String,
  pub name: String,
  pub slug: String,
  pub output_root: String,
  pub target_platform: Option<String>,
  pub default_model_id: Option<String>,
  pub default_workflow_id: Option<String>,
  pub default_language: Option<String>,
  pub default_tone: Option<String>,
  pub default_aspect_ratio: Option<String>,
  pub browser_profile_id: Option<String>,
  pub is_archived: bool,
  pub created_at: i64,
  pub updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct RawContentPage {
  pub id: String,
  pub name: String,
  pub slug: String,
  pub output_root: String,
  pub target_platform: Option<String>,
  pub default_model_id: Option<String>,
  pub default_workflow_id: Option<String>,
  pub default_language: Option<String>,
  pub default_tone: Option<String>,
  pub default_aspect_ratio: Option<String>,
  pub browser_profile_id: Option<String>,
  pub is_archived: i64,
  pub created_at: i64,
  pub updated_at: i64,
}

pub(crate) fn raw_into_content_page(raw: RawContentPage) -> Result<ContentPage, SqliteTasksError> {
  Ok(ContentPage {
    id: raw.id,
    name: raw.name,
    slug: raw.slug,
    output_root: raw.output_root,
    target_platform: raw.target_platform,
    default_model_id: raw.default_model_id,
    default_workflow_id: raw.default_workflow_id,
    default_language: raw.default_language,
    default_tone: raw.default_tone,
    default_aspect_ratio: raw.default_aspect_ratio,
    browser_profile_id: raw.browser_profile_id,
    is_archived: raw.is_archived != 0,
    created_at: raw.created_at,
    updated_at: raw.updated_at,
  })
}
