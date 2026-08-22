use log::{error, info};
use sha2::{Digest, Sha256};
use sqlite_tasks::connection::TaskDbConnection;
use sqlite_tasks::queries::content_page_publish_targets::list_publish_targets_for_page::list_publish_targets_for_page;
use sqlite_tasks::queries::job_publications::create_job_publication::{create_job_publication, CreateJobPublicationArgs};
use sqlite_tasks::queries::job_publications::job_publication::JobPublication;
use sqlite_tasks::queries::pipeline_job_events::insert_pipeline_job_event::{insert_pipeline_job_event, InsertPipelineJobEventArgs};

/// Creates publication records for a completed video job based on enabled page publish targets.
pub async fn create_publications_for_completed_job(db: &TaskDbConnection, job_id: &str, page_id: &str, video_path: &str, title: Option<&str>, caption: Option<&str>, hashtags: &[String], description: Option<&str>) -> Result<Vec<JobPublication>, String> {
  info!("[PublicationManager] Creating publication records for job_id={} page_id={}", job_id, page_id);

  let targets = list_publish_targets_for_page(db, page_id).await.map_err(|e| format!("Failed to list publish targets: {e}"))?;

  let enabled_targets: Vec<_> = targets.into_iter().filter(|t| t.enabled).collect();

  if enabled_targets.is_empty() {
    info!("[PublicationManager] No enabled publish targets configured for page_id={}", page_id);
    return Ok(Vec::new());
  }

  let hashtags_json = serde_json::to_string(hashtags).ok();
  let mut created_pubs = Vec::new();

  for target in enabled_targets {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{}:{}:{}", job_id, page_id, target.platform, target.destination_id));
    let hash_bytes = hasher.finalize();
    let idempotency_key = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();

    let (status, scheduled_at, approved_at) = if target.post_mode.to_lowercase() == "auto" { ("READY_TO_POST".to_string(), None, Some(chrono::Utc::now().timestamp())) } else { ("WAITING_APPROVAL".to_string(), None, None) };

    let args = CreateJobPublicationArgs { job_id: job_id.to_string(), page_id: page_id.to_string(), platform: target.platform.clone(), target_config_id: Some(target.id.clone()), browser_profile_id: target.browser_profile_id.clone(), status: status.clone(), scheduled_at, approved_at, idempotency_key: idempotency_key.clone(), title: title.map(|s| s.to_string()), caption: caption.map(|s| s.to_string()), hashtags_json: hashtags_json.clone(), description: description.map(|s| s.to_string()), video_path: Some(video_path.to_string()) };

    match create_job_publication(db, args).await {
      Ok(pub_record) => {
        info!("[PublicationManager] Created publication record: id={} platform={} status={} key={}", pub_record.id, pub_record.platform, pub_record.status, pub_record.idempotency_key);

        let stage_str = format!("PUBLISH_{}", pub_record.platform.to_uppercase());
        let msg_str = format!("Khởi tạo bản ghi xuất bản {} [{}]: chế độ {}", pub_record.platform, pub_record.id, target.post_mode);
        let meta_str = serde_json::json!({
          "publication_id": pub_record.id,
          "platform": pub_record.platform,
          "mode": target.post_mode,
          "browser_profile_id": pub_record.browser_profile_id
        })
        .to_string();

        let _ = insert_pipeline_job_event(InsertPipelineJobEventArgs { db, id: None, job_id, sequence: 15, stage_id: Some(&stage_str), business_status: Some(&pub_record.status), event_type: "PUBLICATION_CREATED", level: "INFO", message: &msg_str, error_code: None, metadata_json: Some(&meta_str) }).await;

        created_pubs.push(pub_record);
      },
      Err(err) => {
        error!("[PublicationManager] Failed to create publication for platform {}: {err}", target.platform);
      },
    }
  }

  Ok(created_pubs)
}
