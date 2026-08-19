use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use crate::queries::pipeline_job_events::pipeline_job_event::{raw_into_pipeline_job_event, PipelineJobEvent, RawPipelineJobEvent};

pub struct ListPipelineJobEventsArgs<'a> {
  pub db: &'a TaskDbConnection,
  pub job_id: &'a str,
}

pub async fn list_pipeline_job_events(args: ListPipelineJobEventsArgs<'_>) -> Result<Vec<PipelineJobEvent>, SqliteTasksError> {
  let results = sqlx::query_as::<_, RawPipelineJobEvent>(
    r#"
    SELECT
      id,
      job_id,
      sequence,
      stage_id,
      business_status,
      event_type,
      level,
      message,
      error_code,
      metadata_json,
      created_at
    FROM pipeline_job_events
    WHERE job_id = ?1
    ORDER BY sequence ASC, created_at ASC
    "#,
  )
  .bind(args.job_id)
  .fetch_all(args.db.get_pool())
  .await?;

  let events = results.into_iter().map(raw_into_pipeline_job_event).collect();
  Ok(events)
}
