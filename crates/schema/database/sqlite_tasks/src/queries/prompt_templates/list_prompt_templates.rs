use super::prompt_template::PromptTemplate;
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub async fn list_prompt_templates(
  db: &TaskDbConnection,
) -> Result<Vec<PromptTemplate>, SqliteTasksError> {
  let templates: Vec<PromptTemplate> = sqlx::query_as(
    r#"
    SELECT id, name, image_prompt, expand_prompt, video_prompt, created_at, updated_at
    FROM floword_prompt_templates
    ORDER BY name ASC
    "#,
  )
  .fetch_all(db.get_pool())
  .await?;

  Ok(templates)
}
