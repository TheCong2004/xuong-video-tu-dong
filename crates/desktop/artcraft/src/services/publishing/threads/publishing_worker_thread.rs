use super::super::adapters::facebook_adapter::FacebookPublisherAdapter;
use super::super::adapters::publisher_adapter::{PublicationExecutionContext, PublisherAdapter, PublisherErrorCode};
use super::super::adapters::tiktok_adapter::TikTokPublisherAdapter;
use super::super::adapters::youtube_adapter::YouTubePublisherAdapter;
use log::{error, info, warn};
use sqlite_tasks::connection::TaskDbConnection;
use sqlite_tasks::queries::job_publications::claim_job_publication::claim_job_publication;
use sqlite_tasks::queries::job_publications::list_pending_publications::list_pending_publications;
use sqlite_tasks::queries::job_publications::update_job_publication::{update_job_publication, UpdateJobPublicationArgs};
use sqlite_tasks::queries::pipeline_job_events::insert_pipeline_job_event::{insert_pipeline_job_event, InsertPipelineJobEventArgs};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;

pub struct PublishingWorkerThread;

impl PublishingWorkerThread {
  pub fn start(app: AppHandle, db: TaskDbConnection) {
    info!("[PublishingWorker] Starting dedicated Publishing Worker Thread...");
    let db = Arc::new(db);

    tauri::async_runtime::spawn(async move {
      let fb_adapter = Arc::new(FacebookPublisherAdapter::new());
      let tt_adapter = Arc::new(TikTokPublisherAdapter::new());
      let yt_adapter = Arc::new(YouTubePublisherAdapter::new());

      let mut poll_interval = tokio::time::interval(Duration::from_secs(2));

      loop {
        poll_interval.tick().await;

        let pending_items = match list_pending_publications(&db, 10).await {
          Ok(items) => items,
          Err(err) => {
            warn!("[PublishingWorker] Failed to query pending publications: {err}");
            continue;
          }
        };

        if pending_items.is_empty() {
          continue;
        }

        info!("[PublishingWorker] Discovered {} pending publication(s)", pending_items.len());

        for pub_item in pending_items {
          let db = db.clone();
          let fb_adapter = fb_adapter.clone();
          let tt_adapter = tt_adapter.clone();
          let yt_adapter = yt_adapter.clone();

          // 1. Atomic claim
          let claimed = match claim_job_publication(&db, &pub_item.id).await {
            Ok(Some(item)) => item,
            Ok(None) => {
              // Another worker claimed or not ready
              continue;
            }
            Err(err) => {
              error!("[PublishingWorker] Failed to claim publication {}: {err}", pub_item.id);
              continue;
            }
          };

          info!(
            "[PublishingWorker] Claimed publication id={} job_id={} platform={} profile={}",
            claimed.id, claimed.job_id, claimed.platform, claimed.browser_profile_id
          );

          let stage_str = format!("PUBLISH_{}", claimed.platform.to_uppercase());
          let msg_started = format!("Bắt đầu đăng tải lên {} qua profile '{}'", claimed.platform, claimed.browser_profile_id);
          let meta_started = serde_json::json!({
            "publication_id": claimed.id,
            "platform": claimed.platform,
            "attempt": claimed.attempt_count
          }).to_string();

          // Record POST_STARTED event
          let _ = insert_pipeline_job_event(
            InsertPipelineJobEventArgs {
              db: &db,
              id: None,
              job_id: &claimed.job_id,
              sequence: 20,
              stage_id: Some(&stage_str),
              business_status: Some("POSTING"),
              event_type: "POST_STARTED",
              level: "INFO",
              message: &msg_started,
              error_code: None,
              metadata_json: Some(&meta_started),
            },
          ).await;

          // Spawn async publication task
          tokio::spawn(async move {
            let hashtags: Vec<String> = claimed
              .hashtags_json
              .as_deref()
              .and_then(|j| serde_json::from_str(j).ok())
              .unwrap_or_default();

            let ctx = PublicationExecutionContext {
              publication_id: claimed.id.clone(),
              job_id: claimed.job_id.clone(),
              page_id: claimed.page_id.clone(),
              platform: claimed.platform.clone(),
              browser_profile_id: claimed.browser_profile_id.clone(),
              video_path: claimed.video_path.clone().unwrap_or_default(),
              title: claimed.title.clone(),
              caption: claimed.caption.clone(),
              hashtags,
              description: claimed.description.clone(),
              target_destination_id: claimed.target_config_id.clone().unwrap_or_default(),
              target_destination_handle: None,
              idempotency_key: claimed.idempotency_key.clone(),
            };

            let adapter: Arc<dyn PublisherAdapter> = match claimed.platform.to_lowercase().as_str() {
              "facebook" => fb_adapter,
              "tiktok" => tt_adapter,
              "youtube" => yt_adapter,
              unknown => {
                error!("[PublishingWorker] Unknown platform: {unknown}");
                let _ = update_job_publication(
                  &db,
                  UpdateJobPublicationArgs {
                    id: claimed.id.clone(),
                    status: Some("POST_ERROR".to_string()),
                    scheduled_at: None,
                    approved_at: None,
                    posted_at: None,
                    platform_post_id: None,
                    post_url: None,
                    title: None,
                    caption: None,
                    last_error_code: Some("UNKNOWN_PLATFORM".to_string()),
                    last_error_message: Some(format!("Unsupported publishing platform '{unknown}'")),
                  },
                ).await;
                return;
              }
            };

            match adapter.publish(&ctx).await {
              Ok(res) => {
                info!(
                  "[PublishingWorker] Publication SUCCESS: id={} platform={} post_id={:?}",
                  claimed.id, claimed.platform, res.platform_post_id
                );

                let _ = update_job_publication(
                  &db,
                  UpdateJobPublicationArgs {
                    id: claimed.id.clone(),
                    status: Some("POSTED".to_string()),
                    scheduled_at: None,
                    approved_at: None,
                    posted_at: Some(res.posted_at),
                    platform_post_id: res.platform_post_id.clone(),
                    post_url: res.post_url.clone(),
                    title: None,
                    caption: None,
                    last_error_code: None,
                    last_error_message: None,
                  },
                ).await;

                let msg_ok = format!("Đăng tải thành công lên {}! Post URL: {:?}", claimed.platform, res.post_url);
                let meta_ok = serde_json::json!({
                  "publication_id": claimed.id,
                  "platform_post_id": res.platform_post_id,
                  "post_url": res.post_url
                }).to_string();

                let _ = insert_pipeline_job_event(
                  InsertPipelineJobEventArgs {
                    db: &db,
                    id: None,
                    job_id: &claimed.job_id,
                    sequence: 21,
                    stage_id: Some(&stage_str),
                    business_status: Some("POSTED"),
                    event_type: "POST_CONFIRMED",
                    level: "INFO",
                    message: &msg_ok,
                    error_code: None,
                    metadata_json: Some(&meta_ok),
                  },
                ).await;
              }
              Err(err) => {
                error!(
                  "[PublishingWorker] Publication ERROR: id={} platform={} err={:?}",
                  claimed.id, claimed.platform, err
                );

                let (new_status, is_retryable) = match err.code {
                  PublisherErrorCode::AuthRequired => ("AUTH_REQUIRED", false),
                  PublisherErrorCode::VerifyFailed => ("VERIFY_REQUIRED", false),
                  _ => {
                    if err.retryable && claimed.attempt_count < 3 {
                      ("READY_TO_POST", true)
                    } else {
                      ("POST_ERROR", false)
                    }
                  }
                };

                let _ = update_job_publication(
                  &db,
                  UpdateJobPublicationArgs {
                    id: claimed.id.clone(),
                    status: Some(new_status.to_string()),
                    scheduled_at: None,
                    approved_at: None,
                    posted_at: None,
                    platform_post_id: None,
                    post_url: None,
                    title: None,
                    caption: None,
                    last_error_code: Some(err.code.as_str().to_string()),
                    last_error_message: Some(err.message.clone()),
                  },
                ).await;

                let msg_err = format!("Lỗi khi đăng lên {}: {} (Thử lại: {})", claimed.platform, err.message, is_retryable);
                let meta_err = serde_json::json!({
                  "publication_id": claimed.id,
                  "retryable": is_retryable,
                  "attempt": claimed.attempt_count
                }).to_string();

                let _ = insert_pipeline_job_event(
                  InsertPipelineJobEventArgs {
                    db: &db,
                    id: None,
                    job_id: &claimed.job_id,
                    sequence: 22,
                    stage_id: Some(&stage_str),
                    business_status: Some(new_status),
                    event_type: if err.code == PublisherErrorCode::AuthRequired {
                      "AUTH_REQUIRED"
                    } else if err.code == PublisherErrorCode::VerifyFailed {
                      "VERIFY_REQUIRED"
                    } else {
                      "POST_ERROR"
                    },
                    level: if err.code == PublisherErrorCode::AuthRequired { "WARN" } else { "ERROR" },
                    message: &msg_err,
                    error_code: Some(err.code.as_str()),
                    metadata_json: Some(&meta_err),
                  },
                ).await;
              }
            }
          });
        }
      }
    });
  }
}
