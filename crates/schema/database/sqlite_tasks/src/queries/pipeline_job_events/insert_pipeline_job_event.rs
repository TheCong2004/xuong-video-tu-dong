use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;

pub struct InsertPipelineJobEventArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub id: Option<&'a str>,
  pub job_id: &'a str,
  pub sequence: i64,
  pub stage_id: Option<&'a str>,
  pub business_status: Option<&'a str>,
  pub event_type: &'a str,
  pub level: &'a str,
  pub message: &'a str,
  pub error_code: Option<&'a str>,
  pub metadata_json: Option<&'a str>,
}

pub async fn insert_pipeline_job_event(args: InsertPipelineJobEventArgs<'_>) -> Result<String, SqliteTasksError> {
  let id = args.id.map(|s| s.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

  sqlx::query(
    r#"
    INSERT INTO pipeline_job_events (
      id,
      job_id,
      sequence,
      stage_id,
      business_status,
      event_type,
      level,
      message,
      error_code,
      metadata_json
    )
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
    "#,
  )
  .bind(&id)
  .bind(args.job_id)
  .bind(args.sequence)
  .bind(args.stage_id)
  .bind(args.business_status)
  .bind(args.event_type)
  .bind(args.level)
  .bind(args.message)
  .bind(args.error_code)
  .bind(args.metadata_json)
  .execute(args.db.get_pool())
  .await?;

  Ok(id)
}
