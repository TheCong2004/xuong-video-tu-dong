use super::prompt_template::PromptTemplate;
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct CreatePromptTemplateArgs {
  pub id: Option<String>,
  pub name: String,
  pub image_prompt: String,
  pub expand_prompt: Option<String>,
  pub video_prompt: String,
}

pub async fn create_prompt_template(
  db: &TaskDbConnection,
  args: CreatePromptTemplateArgs,
) -> Result<PromptTemplate, SqliteTasksError> {
  let id = args.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

  let template: PromptTemplate = sqlx::query_as(
    r#"
    INSERT INTO floword_prompt_templates (id, name, image_prompt, expand_prompt, video_prompt)
    VALUES ($1, $2, $3, $4, $5)
    RETURNING id, name, image_prompt, expand_prompt, video_prompt, created_at, updated_at
    "#,
  )
  .bind(&id)
  .bind(&args.name)
  .bind(&args.image_prompt)
  .bind(&args.expand_prompt)
  .bind(&args.video_prompt)
  .fetch_one(db.get_pool())
  .await?;

  Ok(template)
}
