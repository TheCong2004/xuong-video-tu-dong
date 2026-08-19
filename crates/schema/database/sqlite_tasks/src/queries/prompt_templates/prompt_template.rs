use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PromptTemplate {
  pub id: String,
  pub name: String,
  pub image_prompt: String,
  pub expand_prompt: Option<String>,
  pub video_prompt: String,
  pub created_at: i64,
  pub updated_at: i64,
}
