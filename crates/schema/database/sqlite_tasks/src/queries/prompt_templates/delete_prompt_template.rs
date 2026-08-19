use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub async fn delete_prompt_template(
  db: &TaskDbConnection,
  id: &str,
) -> Result<bool, SqliteTasksError> {
  let res = sqlx::query("DELETE FROM floword_prompt_templates WHERE id = $1")
    .bind(id)
    .execute(db.get_pool())
    .await?;

  Ok(res.rows_affected() > 0)
}
