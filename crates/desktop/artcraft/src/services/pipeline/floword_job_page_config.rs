use log::{info, warn};
use serde::{Deserialize, Serialize};
use sqlite_tasks::connection::TaskDbConnection;
use sqlite_tasks::queries::content_pages::get_content_page_by_id::{get_content_page_by_id, GetContentPageByIdArgs};
use sqlite_tasks::queries::content_pages::content_page::ContentPage;
use sqlite_tasks::queries::pipeline::pipeline_job::PipelineJob;
use sqlite_tasks::queries::pipeline_job_events::insert_pipeline_job_event::{insert_pipeline_job_event, InsertPipelineJobEventArgs};

#[derive(Debug)]
pub enum PageConfigError {
  PageIdMissing { job_id: String },
  PageNotFound { page_id: String, job_id: String },
  DatabaseError { page_id: String, source: anyhow::Error },
  OutputRootMissing { page_id: String, page_name: String },
}

impl std::fmt::Display for PageConfigError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::PageIdMissing { job_id } => write!(f, "Job '{job_id}' is missing required page_id"),
      Self::PageNotFound { page_id, job_id } => write!(f, "ContentPage '{page_id}' not found in database for legacy job '{job_id}'"),
      Self::DatabaseError { page_id, source } => write!(f, "Failed to query database for page '{page_id}': {source}"),
      Self::OutputRootMissing { page_id, page_name } => write!(f, "Page '{page_name}' ({page_id}) has an empty output_root directory"),
    }
  }
}

impl std::error::Error for PageConfigError {}


/// Canonical immutable Page Snapshot attached to a Job upon creation.
/// Once created, this snapshot NEVER changes even if the user edits the mutable ContentPage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowordJobPageSnapshot {
  pub page_id: String,
  pub page_name: String,
  pub slug: String,
  pub output_root: String,
  pub browser_profile_id: Option<String>,
  pub worker_pool_id: Option<String>,
  pub target_platform: Option<String>,
  pub default_image_prompt: Option<String>,
  pub default_expand_9_16_prompt: Option<String>,
  pub default_video_prompt: Option<String>,
  pub default_language: Option<String>,
  pub default_tone: Option<String>,
  pub default_aspect_ratio: Option<String>,
}

impl FlowordJobPageSnapshot {
  /// Construct snapshot from a mutable ContentPage record.
  pub fn from_content_page(page: &ContentPage) -> Self {
    Self {
      page_id: page.id.clone(),
      page_name: page.name.clone(),
      slug: page.slug.clone(),
      output_root: page.output_root.clone(),
      browser_profile_id: page.browser_profile_id.clone(),
      worker_pool_id: page.worker_pool_id.clone(),
      target_platform: page.target_platform.clone(),
      default_image_prompt: page.default_image_prompt.clone(),
      default_expand_9_16_prompt: page.default_expand_9_16_prompt.clone(),
      default_video_prompt: page.default_video_prompt.clone(),
      default_language: page.default_language.clone(),
      default_tone: page.default_tone.clone(),
      default_aspect_ratio: page.default_aspect_ratio.clone(),
    }
  }
}

/// Canonical resolver for Job Page Configuration.
/// Rules:
/// 1. If job.page_snapshot exists and parses correctly: USE SNAPSHOT exclusively.
/// 2. Do NOT re-read mutable ContentPage fields for values already captured in snapshot.
/// 3. For legacy Jobs created before page_snapshot existed: fallback to ContentPage and emit LEGACY_PAGE_SNAPSHOT_MISSING event.
/// 4. Output root must not be empty.
pub async fn resolve_job_page_config(
  job: &PipelineJob,
  db: &TaskDbConnection,
) -> Result<FlowordJobPageSnapshot, PageConfigError> {
  let job_id_str = job.id.as_str().to_string();

  // 1. Try parsing immutable page_snapshot
  if let Some(snapshot_raw) = job.maybe_page_snapshot.as_deref() {
    if let Ok(snapshot) = serde_json::from_str::<FlowordJobPageSnapshot>(snapshot_raw) {
      if snapshot.output_root.trim().is_empty() {
        return Err(PageConfigError::OutputRootMissing {
          page_id: snapshot.page_id.clone(),
          page_name: snapshot.page_name.clone(),
        });
      }
      info!(
        "[PageConfigResolver] Using immutable page snapshot for job '{}' (page_id: '{}', profile: '{:?}', output_root: '{}')",
        job_id_str, snapshot.page_id, snapshot.browser_profile_id, snapshot.output_root
      );
      return Ok(snapshot);
    }
  }

  // 2. Legacy fallback: determine page_id from job or input_payload
  let page_id = if let Some(pid) = job.maybe_page_id.as_deref().filter(|s| !s.trim().is_empty()) {
    pid.to_string()
  } else if let Some(input_raw) = job.maybe_input_payload.as_deref() {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(input_raw) {
      val.get("page_id")
        .or_else(|| val.get("page"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
    } else {
      String::new()
    }
  } else {
    String::new()
  };

  if page_id.trim().is_empty() {
    return Err(PageConfigError::PageIdMissing { job_id: job_id_str });
  }

  warn!(
    "[PageConfigResolver] Job '{}' missing immutable page_snapshot. Performing legacy fallback to ContentPage '{}'",
    job_id_str, page_id
  );

  let maybe_page = get_content_page_by_id(GetContentPageByIdArgs {
    db,
    id: &page_id,
  })
  .await
  .map_err(|e| PageConfigError::DatabaseError {
    page_id: page_id.clone(),
    source: anyhow::anyhow!("{e}"),
  })?;

  let page = maybe_page.ok_or_else(|| PageConfigError::PageNotFound {
    page_id: page_id.clone(),
    job_id: job_id_str.clone(),
  })?;

  if page.output_root.trim().is_empty() {
    return Err(PageConfigError::OutputRootMissing {
      page_id: page.id.clone(),
      page_name: page.name.clone(),
    });
  }

  // Emit durable warning event that legacy fallback occurred
  let _ = insert_pipeline_job_event(InsertPipelineJobEventArgs {
    db,
    id: None,
    job_id: &job_id_str,
    sequence: 1,
    stage_id: Some("PAGE_CONFIG_RESOLVER"),
    business_status: None,
    event_type: "LEGACY_PAGE_SNAPSHOT_MISSING",
    level: "WARN",
    message: "Job lacked immutable page snapshot; fell back to current ContentPage configuration",
    error_code: None,
    metadata_json: Some(&serde_json::json!({ "fallback_page_id": page_id }).to_string()),
  })
  .await;

  Ok(FlowordJobPageSnapshot::from_content_page(&page))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::connection::TaskDbConnection;
  use sqlite_tasks::queries::content_pages::create_content_page::{create_content_page, CreateContentPageArgs};
  use sqlite_tasks::queries::content_pages::update_content_page::{update_content_page, UpdateContentPageArgs};
  use sqlite_tasks::queries::pipeline::create_pipeline_job::{create_pipeline_job, CreatePipelineJobArgs};
  use sqlite_tasks::queries::pipeline::get_pipeline_job_by_id::{get_pipeline_job_by_id, GetPipelineJobByIdArgs};
  use enums::tauri::pipeline::pipeline_stage::PipelineStage;
  use enums::tauri::tasks::task_status::TaskStatus;

  #[test]
  fn test_immutable_page_snapshot_isolation() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
      let temp = tempfile::tempdir().unwrap();
      let db = TaskDbConnection::connect_and_migrate(temp.path().join("tasks.sqlite")).await.unwrap();

      // 1. Create Page A
      let page_id = create_content_page(CreateContentPageArgs {
        db: &db,
        name: "Page A",
        slug: "page-a",
        output_root: "D:\\Stage A",
        browser_profile_id: Some("PROFILE_A"),
        worker_pool_id: None,
        target_platform: None,
        default_image_prompt: Some("PROMPT_A"),
        default_expand_9_16_prompt: None,
        default_video_prompt: None,
        default_language: None,
        default_tone: None,
        default_aspect_ratio: None,
        description: None,
      }).await.unwrap();

      // 2. Enqueue Job with snapshot of Page A
      let page_a = get_content_page_by_id(GetContentPageByIdArgs { db: &db, id: &page_id }).await.unwrap().unwrap();
      let snapshot = FlowordJobPageSnapshot::from_content_page(&page_a);
      let snapshot_str = serde_json::to_string(&snapshot).unwrap();

      let job_id = create_pipeline_job(CreatePipelineJobArgs {
        db: &db,
        status: TaskStatus::Pending,
        current_stage: PipelineStage::Queued,
        maybe_page_id: Some(&page_id),
        maybe_input_payload: Some(r#"{"workflow_mode":"grok_content_pipeline"}"#),
        maybe_page_snapshot: Some(&snapshot_str),
        maybe_business_status: Some("QUEUED"),
      }).await.unwrap();

      // 3. User edits Page A to Page B (changes profile, output root, prompt)
      update_content_page(UpdateContentPageArgs {
        db: &db,
        id: &page_id,
        name: "Page B",
        slug: "page-b",
        output_root: "D:\\Stage B",
        browser_profile_id: Some("PROFILE_B"),
        worker_pool_id: None,
        target_platform: None,
        default_image_prompt: Some("PROMPT_B"),
        default_expand_9_16_prompt: None,
        default_video_prompt: None,
        default_language: None,
        default_tone: None,
        default_aspect_ratio: None,
        description: None,
        is_archived: false,
      }).await.unwrap();

      // 4. Resolve Job config -> MUST still have Page A snapshot values
      let job = get_pipeline_job_by_id(GetPipelineJobByIdArgs { db: &db, pipeline_job_id: &job_id }).await.unwrap().unwrap();
      let resolved = resolve_job_page_config(&job, &db).await.unwrap();

      assert_eq!(resolved.page_name, "Page A");
      assert_eq!(resolved.output_root, "D:\\Stage A");
      assert_eq!(resolved.browser_profile_id.as_deref(), Some("PROFILE_A"));
      assert_eq!(resolved.default_image_prompt.as_deref(), Some("PROMPT_A"));
    });
  }

  #[test]
  fn test_legacy_fallback_when_snapshot_missing() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
      let temp = tempfile::tempdir().unwrap();
      let db = TaskDbConnection::connect_and_migrate(temp.path().join("tasks.sqlite")).await.unwrap();

      // 1. Create Page
      let page_id = create_content_page(CreateContentPageArgs {
        db: &db,
        name: "Legacy Page",
        slug: "legacy-page",
        output_root: "D:\\LegacyOutput",
        browser_profile_id: Some("PROFILE_LEGACY"),
        worker_pool_id: None,
        target_platform: None,
        default_image_prompt: Some("LEGACY_PROMPT"),
        default_expand_9_16_prompt: None,
        default_video_prompt: None,
        default_language: None,
        default_tone: None,
        default_aspect_ratio: None,
        description: None,
      }).await.unwrap();

      // 2. Enqueue Job WITHOUT snapshot
      let job_id = create_pipeline_job(CreatePipelineJobArgs {
        db: &db,
        status: TaskStatus::Pending,
        current_stage: PipelineStage::Queued,
        maybe_page_id: Some(&page_id),
        maybe_input_payload: Some(r#"{"workflow_mode":"grok_content_pipeline"}"#),
        maybe_page_snapshot: None,
        maybe_business_status: Some("QUEUED"),
      }).await.unwrap();

      let job = get_pipeline_job_by_id(GetPipelineJobByIdArgs { db: &db, pipeline_job_id: &job_id }).await.unwrap().unwrap();
      let resolved = resolve_job_page_config(&job, &db).await.unwrap();

      assert_eq!(resolved.page_name, "Legacy Page");
      assert_eq!(resolved.output_root, "D:\\LegacyOutput");
      assert_eq!(resolved.browser_profile_id.as_deref(), Some("PROFILE_LEGACY"));
    });
  }
}
