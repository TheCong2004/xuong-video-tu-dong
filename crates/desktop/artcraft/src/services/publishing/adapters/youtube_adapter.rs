use super::publisher_adapter::{PublicationExecutionContext, PublicationResult, PublisherAdapter, PublisherError, PublisherErrorCode};
use crate::services::pipeline::clients::browser_runtime_client::{acquire_worker, release_lease, AcquireWorkerRequest};
use async_trait::async_trait;
use log::{error, info, warn};
use reqwest::Client;
use std::path::Path;
use std::time::Duration;

pub struct YouTubePublisherAdapter;

impl YouTubePublisherAdapter {
  pub fn new() -> Self {
    Self
  }
}

#[async_trait]
impl PublisherAdapter for YouTubePublisherAdapter {
  async fn prepare(&self, ctx: &PublicationExecutionContext) -> Result<(), PublisherError> {
    info!("[YouTubePublisher] Preparing execution for pub_id={} channel={}", ctx.publication_id, ctx.target_destination_id);
    if !Path::new(&ctx.video_path).exists() {
      return Err(PublisherError::new(
        PublisherErrorCode::VideoNotFound,
        format!("Master video file not found at path: {}", ctx.video_path),
        false,
      ));
    }
    Ok(())
  }

  async fn validate_session(&self, ctx: &PublicationExecutionContext) -> Result<bool, PublisherError> {
    info!("[YouTubePublisher] Validating session for profile={}", ctx.browser_profile_id);
    let req = AcquireWorkerRequest {
      job_id: ctx.job_id.clone(),
      step_id: format!("publish_yt_{}", ctx.publication_id),
      attempt_id: "val_session".to_string(),
      capability: "social.youtube.publish".to_string(),
      pool_id: None,
      profile_id: Some(ctx.browser_profile_id.clone()),
      ttl_seconds: Some(30),
    };

    match acquire_worker(req).await {
      Ok(lease) => {
        let _ = release_lease(&lease.lease_id).await;
        Ok(true)
      }
      Err(err) => {
        if err.contains("AUTH_REQUIRED") || err.contains("not logged in") {
          Err(PublisherError::auth_required(format!("YouTube session expired or not authenticated in profile '{}'", ctx.browser_profile_id)))
        } else {
          Err(PublisherError::profile_offline(format!("Browser profile '{}' offline or unavailable: {}", ctx.browser_profile_id, err)))
        }
      }
    }
  }

  async fn publish(&self, ctx: &PublicationExecutionContext) -> Result<PublicationResult, PublisherError> {
    self.prepare(ctx).await?;

    info!(
      "[YouTubePublisher] Initiating YouTube Shorts publishing: pub_id={} profile={} channel={}",
      ctx.publication_id, ctx.browser_profile_id, ctx.target_destination_id
    );

    // 1. Acquire exclusive lock on target browser profile
    let acquire_req = AcquireWorkerRequest {
      job_id: ctx.job_id.clone(),
      step_id: format!("publish_yt_{}", ctx.publication_id),
      attempt_id: format!("attempt_{}", ctx.publication_id),
      capability: "social.youtube.publish".to_string(),
      pool_id: None,
      profile_id: Some(ctx.browser_profile_id.clone()),
      ttl_seconds: Some(300),
    };

    let lease = acquire_worker(acquire_req).await.map_err(|err| {
      if err.contains("AUTH_REQUIRED") {
        PublisherError::auth_required(err)
      } else {
        PublisherError::profile_offline(err)
      }
    })?;

    let lease_id = lease.lease_id.clone();

    // 2. Dispatch publishing command to Donut Browser runtime
    let base_url = crate::services::pipeline::clients::browser_runtime_client::get_donut_browser_api_base_url();
    let client = Client::builder().timeout(Duration::from_secs(180)).build().map_err(|e| {
      PublisherError::new(PublisherErrorCode::NetworkError, e.to_string(), true)
    })?;

    let dispatch_url = format!("{base_url}/v1/workers/{}/dispatch", lease.worker_id);
    let payload = serde_json::json!({
      "requestId": format!("req_yt_{}", ctx.publication_id),
      "jobId": ctx.job_id,
      "publicationId": ctx.publication_id,
      "leaseId": lease_id,
      "profileId": ctx.browser_profile_id,
      "platform": "youtube",
      "method": "social.youtube.shorts.publish",
      "params": {
        "targetChannelId": ctx.target_destination_id,
        "videoPath": ctx.video_path,
        "title": ctx.title.as_deref().unwrap_or("Shorts Video"),
        "caption": ctx.caption,
        "description": ctx.description,
        "hashtags": ctx.hashtags,
        "idempotencyKey": ctx.idempotency_key
      }
    });

    info!("[YouTubePublisher] Dispatching payload to worker: {}", lease.worker_id);
    let resp = client.post(&dispatch_url).json(&payload).send().await;

    // Ensure lease is released
    let _ = release_lease(&lease_id).await;

    match resp {
      Ok(res) if res.status().is_success() => {
        let body: serde_json::Value = res.json().await.map_err(|e| {
          PublisherError::new(PublisherErrorCode::UploadFailed, format!("Failed to parse response: {e}"), false)
        })?;

        let video_id = body.get("videoId").and_then(|v| v.as_str()).map(|s| s.to_string())
          .or_else(|| body.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()));
        let post_url = body.get("videoUrl").and_then(|v| v.as_str()).map(|s| s.to_string())
          .or_else(|| body.get("postUrl").and_then(|v| v.as_str()).map(|s| s.to_string()))
          .or_else(|| video_id.as_ref().map(|id| format!("https://www.youtube.com/shorts/{id}")));

        info!("[YouTubePublisher] Published successfully! video_id={:?} url={:?}", video_id, post_url);
        Ok(PublicationResult {
          platform_post_id: video_id,
          post_url,
          posted_at: chrono::Utc::now().timestamp(),
          raw_metadata: Some(body),
        })
      }
      Ok(res) => {
        let status = res.status();
        let body_text = res.text().await.unwrap_or_default();
        error!("[YouTubePublisher] YouTube publish returned {status}: {body_text}");
        if body_text.contains("AUTH_REQUIRED") || body_text.contains("login") {
          Err(PublisherError::auth_required(body_text))
        } else {
          Err(PublisherError::upload_failed(format!("YouTube publish failed ({status}): {body_text}"), true))
        }
      }
      Err(err) => {
        error!("[YouTubePublisher] Network error during dispatch: {err}");
        Err(PublisherError::new(
          PublisherErrorCode::VerifyFailed,
          format!("Network disconnected during YouTube publish, verification required: {err}"),
          false,
        ))
      }
    }
  }

  async fn verify(&self, ctx: &PublicationExecutionContext) -> Result<Option<PublicationResult>, PublisherError> {
    info!("[YouTubePublisher] Verifying publication status for pub_id={} key={}", ctx.publication_id, ctx.idempotency_key);
    let base_url = crate::services::pipeline::clients::browser_runtime_client::get_donut_browser_api_base_url();
    let client = Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| {
      PublisherError::new(PublisherErrorCode::NetworkError, e.to_string(), true)
    })?;

    let verify_url = format!("{base_url}/v1/publications/verify");
    let payload = serde_json::json!({
      "publicationId": ctx.publication_id,
      "platform": "youtube",
      "profileId": ctx.browser_profile_id,
      "targetChannelId": ctx.target_destination_id,
      "idempotencyKey": ctx.idempotency_key
    });

    if let Ok(res) = client.post(&verify_url).json(&payload).send().await {
      if res.status().is_success() {
        if let Ok(body) = res.json::<serde_json::Value>().await {
          if body.get("verified").and_then(|v| v.as_bool()).unwrap_or(false) {
            let video_id = body.get("videoId").and_then(|v| v.as_str()).map(|s| s.to_string());
            let post_url = body.get("videoUrl").and_then(|v| v.as_str()).map(|s| s.to_string());
            return Ok(Some(PublicationResult {
              platform_post_id: video_id,
              post_url,
              posted_at: chrono::Utc::now().timestamp(),
              raw_metadata: Some(body),
            }));
          }
        }
      }
    }

    Ok(None)
  }

  async fn cancel_if_supported(&self, ctx: &PublicationExecutionContext) -> Result<(), PublisherError> {
    info!("[YouTubePublisher] Cancelling YouTube publication pub_id={}", ctx.publication_id);
    Ok(())
  }
}
