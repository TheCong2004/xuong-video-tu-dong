use super::pipeline_job::{raw_into_pipeline_job, PipelineJob, RawPipelineJob};
use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListPipelineJobsPaginatedArgs {
  pub page_id: Option<String>,
  pub status: Option<String>,
  pub date_from: Option<i64>,
  pub date_to: Option<i64>,
  pub search_query: Option<String>,
  pub limit: Option<i64>,
  pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedPipelineJobsResult {
  pub jobs: Vec<PipelineJob>,
  pub total_count: i64,
  pub limit: i64,
  pub offset: i64,
}

pub async fn list_pipeline_jobs_paginated(
  db: &TaskDbConnection,
  args: ListPipelineJobsPaginatedArgs,
) -> Result<PaginatedPipelineJobsResult, SqliteTasksError> {
  let pool = db.get_pool();
  let limit = args.limit.unwrap_or(20).clamp(1, 100);
  let offset = args.offset.unwrap_or(0).max(0);

  let search_pattern = args.search_query.as_ref().map(|q| format!("%{}%", q.trim()));

  // 1. Get total count
  let count_row: (i64,) = sqlx::query_as(
    r#"
    SELECT COUNT(*)
    FROM pipeline_jobs
    WHERE ($1 IS NULL OR page_id = $1)
      AND ($2 IS NULL OR status = $2)
      AND ($3 IS NULL OR created_at >= $3)
      AND ($4 IS NULL OR created_at <= $4)
      AND ($5 IS NULL OR id LIKE $5 OR input_payload LIKE $5)
    "#,
  )
  .bind(&args.page_id)
  .bind(&args.status)
  .bind(args.date_from)
  .bind(args.date_to)
  .bind(&search_pattern)
  .fetch_one(pool)
  .await?;

  let total_count = count_row.0;

  // 2. Fetch paginated records
  let rows: Vec<RawPipelineJob> = sqlx::query_as(
    r#"
    SELECT
      id, status, current_stage, page_id, input_payload, stage_outputs,
      on_failure_message, page_snapshot, business_status, started_at, failure_code,
      failure_stage, created_at, updated_at, completed_at
    FROM pipeline_jobs
    WHERE ($1 IS NULL OR page_id = $1)
      AND ($2 IS NULL OR status = $2)
      AND ($3 IS NULL OR created_at >= $3)
      AND ($4 IS NULL OR created_at <= $4)
      AND ($5 IS NULL OR id LIKE $5 OR input_payload LIKE $5)
    ORDER BY created_at DESC
    LIMIT $6 OFFSET $7
    "#,
  )
  .bind(&args.page_id)
  .bind(&args.status)
  .bind(args.date_from)
  .bind(args.date_to)
  .bind(&search_pattern)
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await?;

  let mut jobs = Vec::with_capacity(rows.len());
  for r in rows {
    jobs.push(raw_into_pipeline_job(r)?);
  }

  Ok(PaginatedPipelineJobsResult {
    jobs,
    total_count,
    limit,
    offset,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::connection::TaskDbConnection;
  use crate::queries::pipeline::create_pipeline_job::{create_pipeline_job, CreatePipelineJobArgs};
  use enums::tauri::pipeline::pipeline_stage::PipelineStage;
  use enums::tauri::tasks::task_status::TaskStatus;

  #[test]
  fn test_list_pipeline_jobs_paginated() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
      let temp = tempfile::tempdir().unwrap();
      let db = TaskDbConnection::connect_and_migrate(temp.path().join("tasks.sqlite")).await.unwrap();

      // Create 5 dummy jobs
      for i in 0..5 {
        let payload = format!(r#"{{"prompt":"cyberpunk_test_item_{i}"}}"#);
        create_pipeline_job(CreatePipelineJobArgs {
          db: &db,
          status: TaskStatus::Pending,
          current_stage: PipelineStage::Queued,
          maybe_page_id: Some("page_demo"),
          maybe_input_payload: Some(&payload),
          maybe_page_snapshot: None,
          maybe_business_status: Some("QUEUED"),
        }).await.unwrap();
      }

      // Test page 1 with limit 2
      let res1 = list_pipeline_jobs_paginated(&db, ListPipelineJobsPaginatedArgs {
        limit: Some(2),
        offset: Some(0),
        ..Default::default()
      }).await.unwrap();

      assert_eq!(res1.total_count, 5);
      assert_eq!(res1.jobs.len(), 2);

      // Test search filter
      let res_search = list_pipeline_jobs_paginated(&db, ListPipelineJobsPaginatedArgs {
        search_query: Some("cyberpunk_test_item_3".to_string()),
        ..Default::default()
      }).await.unwrap();

      assert_eq!(res_search.total_count, 1);
      assert_eq!(res_search.jobs.len(), 1);
    });
  }
}

