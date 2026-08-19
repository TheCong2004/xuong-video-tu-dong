use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
  pub total_jobs: i64,
  pub queued: i64,
  pub waiting_worker: i64,
  pub generating_image: i64,
  pub converting_9_16: i64,
  pub generating_video: i64,
  pub downloading: i64,
  pub saving_local: i64,
  pub ready_to_post: i64,
  pub scheduled: i64,
  pub posting: i64,
  pub done: i64,
  pub error: i64,
  pub auth_required: i64,

  // Publications breakdown
  pub publications_facebook: i64,
  pub publications_tiktok: i64,
  pub publications_youtube: i64,
  pub publications_posted: i64,
  pub publications_scheduled: i64,
  pub publications_waiting_approval: i64,
  pub publications_error: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardSummaryQueryArgs {
  pub page_id: Option<String>,
  pub date_from: Option<i64>,
  pub date_to: Option<i64>,
  pub status: Option<String>,
  pub business_status: Option<String>,
  pub task_status: Option<String>,
  pub platform: Option<String>,
}

#[derive(sqlx::FromRow)]
struct JobAggregateRow {
  total: i64,
  queued: i64,
  waiting_worker: i64,
  generating_image: i64,
  converting_9_16: i64,
  generating_video: i64,
  downloading: i64,
  saving_local: i64,
  ready_to_post: i64,
  done: i64,
  error: i64,
  auth_required: i64,
}

#[derive(sqlx::FromRow)]
struct PubAggregateRow {
  facebook_count: i64,
  tiktok_count: i64,
  youtube_count: i64,
  posted_count: i64,
  scheduled_count: i64,
  waiting_approval_count: i64,
  error_count: i64,
  posting_count: i64,
  ready_count: i64,
}

pub async fn get_dashboard_summary(
  db: &TaskDbConnection,
  args: DashboardSummaryQueryArgs,
) -> Result<DashboardSummary, SqliteTasksError> {
  let pool = db.get_pool();

  // 1. Authoritative Job aggregation query based on business_status funnel and task terminal states
  let job_row: JobAggregateRow = sqlx::query_as(
    r#"
    SELECT
      COUNT(*) AS total,
      COUNT(CASE WHEN business_status = 'QUEUED' OR (business_status IS NULL AND (status = 'QUEUED' OR status = 'pending')) THEN 1 END) AS queued,
      COUNT(CASE WHEN business_status = 'WAITING_WORKER' THEN 1 END) AS waiting_worker,
      COUNT(CASE WHEN business_status = 'GENERATING_IMAGE' OR business_status = 'IMAGE_DONE' THEN 1 END) AS generating_image,
      COUNT(CASE WHEN business_status = 'CONVERTING_9_16' OR business_status = 'IMAGE_9_16_DONE' THEN 1 END) AS converting_9_16,
      COUNT(CASE WHEN business_status = 'GENERATING_VIDEO' OR business_status = 'VIDEO_DONE' THEN 1 END) AS generating_video,
      COUNT(CASE WHEN business_status = 'DOWNLOADING' OR business_status = 'DOWNLOADED' THEN 1 END) AS downloading,
      COUNT(CASE WHEN business_status = 'SAVING_LOCAL' OR business_status = 'LOCAL_SAVED' THEN 1 END) AS saving_local,
      COUNT(CASE WHEN business_status = 'READY_TO_POST' THEN 1 END) AS ready_to_post,
      COUNT(CASE WHEN business_status = 'DONE' OR status = 'complete_success' OR status = 'DONE' OR status = 'COMPLETED' THEN 1 END) AS done,
      COUNT(CASE WHEN business_status = 'ERROR' OR status = 'complete_failure' OR status = 'ERROR' OR status = 'FAILED' OR status = 'dead' THEN 1 END) AS error,
      COUNT(CASE WHEN business_status = 'AUTH_REQUIRED' OR status = 'waiting_input' THEN 1 END) AS auth_required
    FROM pipeline_jobs
    WHERE ($1 IS NULL OR page_id = $1)
      AND ($2 IS NULL OR created_at >= $2)
      AND ($3 IS NULL OR created_at <= $3)
      AND ($4 IS NULL OR business_status = $4 OR status = $4)
      AND ($5 IS NULL OR business_status = $5)
      AND ($6 IS NULL OR status = $6)
    "#,
  )
  .bind(&args.page_id)
  .bind(args.date_from)
  .bind(args.date_to)
  .bind(&args.status)
  .bind(&args.business_status)
  .bind(&args.task_status)
  .fetch_one(pool)
  .await?;

  // 2. Authoritative Publication aggregation query
  let pub_row: PubAggregateRow = sqlx::query_as(
    r#"
    SELECT
      COUNT(CASE WHEN platform = 'facebook' THEN 1 END) AS facebook_count,
      COUNT(CASE WHEN platform = 'tiktok' THEN 1 END) AS tiktok_count,
      COUNT(CASE WHEN platform = 'youtube' THEN 1 END) AS youtube_count,
      COUNT(CASE WHEN status = 'POSTED' THEN 1 END) AS posted_count,
      COUNT(CASE WHEN status = 'SCHEDULED' THEN 1 END) AS scheduled_count,
      COUNT(CASE WHEN status = 'WAITING_APPROVAL' THEN 1 END) AS waiting_approval_count,
      COUNT(CASE WHEN status = 'POST_ERROR' OR status = 'ERROR' THEN 1 END) AS error_count,
      COUNT(CASE WHEN status = 'POSTING' THEN 1 END) AS posting_count,
      COUNT(CASE WHEN status = 'READY_TO_POST' THEN 1 END) AS ready_count
    FROM job_publications
    WHERE ($1 IS NULL OR page_id = $1)
      AND ($2 IS NULL OR created_at >= $2)
      AND ($3 IS NULL OR created_at <= $3)
      AND ($4 IS NULL OR platform = $4)
    "#,
  )
  .bind(&args.page_id)
  .bind(args.date_from)
  .bind(args.date_to)
  .bind(&args.platform)
  .fetch_one(pool)
  .await?;

  Ok(DashboardSummary {
    total_jobs: job_row.total,
    queued: job_row.queued,
    waiting_worker: job_row.waiting_worker,
    generating_image: job_row.generating_image,
    converting_9_16: job_row.converting_9_16,
    generating_video: job_row.generating_video,
    downloading: job_row.downloading,
    saving_local: job_row.saving_local,
    ready_to_post: job_row.ready_to_post + pub_row.ready_count,
    scheduled: pub_row.scheduled_count,
    posting: pub_row.posting_count,
    done: job_row.done,
    error: job_row.error,
    auth_required: job_row.auth_required,

    publications_facebook: pub_row.facebook_count,
    publications_tiktok: pub_row.tiktok_count,
    publications_youtube: pub_row.youtube_count,
    publications_posted: pub_row.posted_count,
    publications_scheduled: pub_row.scheduled_count,
    publications_waiting_approval: pub_row.waiting_approval_count,
    publications_error: pub_row.error_count,
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
  fn test_dashboard_summary_default_zero() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
      let temp = tempfile::tempdir().unwrap();
      let db = TaskDbConnection::connect_and_migrate(temp.path().join("tasks.sqlite")).await.unwrap();
      let summary = get_dashboard_summary(&db, DashboardSummaryQueryArgs::default())
        .await
        .unwrap();

      assert_eq!(summary.total_jobs, 0);
      assert_eq!(summary.queued, 0);
      assert_eq!(summary.done, 0);
      assert_eq!(summary.publications_facebook, 0);
    });
  }

  #[test]
  fn test_dashboard_summary_business_status_combinations() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
      let temp = tempfile::tempdir().unwrap();
      let db = TaskDbConnection::connect_and_migrate(temp.path().join("tasks.sqlite")).await.unwrap();

      // 1. status=started, business_status=GENERATING_IMAGE
      create_pipeline_job(CreatePipelineJobArgs {
        db: &db,
        status: TaskStatus::Started,
        current_stage: PipelineStage::ScriptGenerating,
        maybe_page_id: Some("page_1"),
        maybe_input_payload: None,
        maybe_page_snapshot: None,
        maybe_business_status: Some("GENERATING_IMAGE"),
      }).await.unwrap();

      // 2. status=started, business_status=WAITING_WORKER
      create_pipeline_job(CreatePipelineJobArgs {
        db: &db,
        status: TaskStatus::Started,
        current_stage: PipelineStage::Queued,
        maybe_page_id: Some("page_1"),
        maybe_input_payload: None,
        maybe_page_snapshot: None,
        maybe_business_status: Some("WAITING_WORKER"),
      }).await.unwrap();

      // 3. status=started, business_status=READY_TO_POST
      create_pipeline_job(CreatePipelineJobArgs {
        db: &db,
        status: TaskStatus::Started,
        current_stage: PipelineStage::Completed,
        maybe_page_id: Some("page_1"),
        maybe_input_payload: None,
        maybe_page_snapshot: None,
        maybe_business_status: Some("READY_TO_POST"),
      }).await.unwrap();

      // 4. status=waiting_input, business_status=AUTH_REQUIRED
      create_pipeline_job(CreatePipelineJobArgs {
        db: &db,
        status: TaskStatus::WaitingInput,
        current_stage: PipelineStage::ScriptGenerating,
        maybe_page_id: Some("page_1"),
        maybe_input_payload: None,
        maybe_page_snapshot: None,
        maybe_business_status: Some("AUTH_REQUIRED"),
      }).await.unwrap();

      // 5. status=complete_failure, business_status=ERROR
      create_pipeline_job(CreatePipelineJobArgs {
        db: &db,
        status: TaskStatus::CompleteFailure,
        current_stage: PipelineStage::ScriptGenerating,
        maybe_page_id: Some("page_1"),
        maybe_input_payload: None,
        maybe_page_snapshot: None,
        maybe_business_status: Some("ERROR"),
      }).await.unwrap();

      let summary = get_dashboard_summary(&db, DashboardSummaryQueryArgs::default()).await.unwrap();

      assert_eq!(summary.total_jobs, 5);
      assert_eq!(summary.generating_image, 1);
      assert_eq!(summary.waiting_worker, 1);
      assert_eq!(summary.ready_to_post, 1);
      assert_eq!(summary.auth_required, 1);
      assert_eq!(summary.error, 1);
    });
  }
}


