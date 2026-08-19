use super::prompt_template::PromptTemplate;
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct UpdatePromptTemplateArgs {
  pub id: String,
  pub name: Option<String>,
  pub image_prompt: Option<String>,
  pub expand_prompt: Option<String>,
  pub video_prompt: Option<String>,
}

pub async fn update_prompt_template(
  db: &TaskDbConnection,
  args: UpdatePromptTemplateArgs,
) -> Result<PromptTemplate, SqliteTasksError> {
  let existing: PromptTemplate = sqlx::query_as(
    "SELECT id, name, image_prompt, expand_prompt, video_prompt, created_at, updated_at FROM floword_prompt_templates WHERE id = $1",
  )
  .bind(&args.id)
  .fetch_optional(db.get_pool())
  .await?
  .ok_or_else(|| SqliteTasksError::Custom(format!("Prompt template {} not found", args.id)))?;

  let name = args.name.unwrap_or(existing.name);
  let image_prompt = args.image_prompt.unwrap_or(existing.image_prompt);
  let expand_prompt = args.expand_prompt.or(existing.expand_prompt);
  let video_prompt = args.video_prompt.unwrap_or(existing.video_prompt);

  let template: PromptTemplate = sqlx::query_as(
    r#"
    UPDATE floword_prompt_templates SET
      name = $1,
      image_prompt = $2,
      expand_prompt = $3,
      video_prompt = $4,
      updated_at = unixepoch('now')
    WHERE id = $5
    RETURNING id, name, image_prompt, expand_prompt, video_prompt, created_at, updated_at
    "#,
  )
  .bind(name)
  .bind(image_prompt)
  .bind(expand_prompt)
  .bind(video_prompt)
  .bind(&args.id)
  .fetch_one(db.get_pool())
  .await?;

  Ok(template)
}
