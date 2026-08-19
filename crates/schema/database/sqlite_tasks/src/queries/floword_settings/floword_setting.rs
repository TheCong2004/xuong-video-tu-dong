use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FlowordSetting {
  pub key: String,
  pub value_json: String,
  pub updated_at: i64,
}
