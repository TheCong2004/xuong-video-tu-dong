use super::publisher_adapter::{PublicationExecutionContext, PublicationResult, PublisherAdapter, PublisherError, PublisherErrorCode};
use crate::services::pipeline::clients::browser_runtime_client::{acquire_worker, release_lease, AcquireWorkerRequest};
use async_trait::async_trait;
use log::{error, info, warn};
use reqwest::Client;
use std::path::Path;
use std::time::Duration;

pub struct TikTokPublisherAdapter;

impl TikTokPublisherAdapter {
  pub fn new() -> Self {
    Self
  }
}

#[async_trait]
impl PublisherAdapter for TikTokPublisherAdapter {
  async fn prepare(&self, ctx: &PublicationExecutionContext) -> Result<(), PublisherError> {
    info!("[TikTokPublisher] Preparing execution for pub_id={} destination={}", ctx.publication_id, ctx.target_destination_id);
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
    info!("[TikTokPublisher] Validating session for profile={}", ctx.browser_profile_id);
    let req = AcquireWorkerRequest {
      job_id: ctx.job_id.clone(),
      step_id: format!("publish_tt_{}", ctx.publication_id),
      attempt_id: "val_session".to_string(),
      capability: "social.tiktok.publish".to_string(),
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
          Err(PublisherError::auth_required(format!("TikTok session expired or not authenticated in profile '{}'", ctx.browser_profile_id)))
        } else {
          Err(PublisherError::profile_offline(format!("Browser profile '{}' offline or unavailable: {}", ctx.browser_profile_id, err)))
        }
      }
    }
  }

  async fn publish(&self, ctx: &PublicationExecutionContext) -> Result<PublicationResult, PublisherError> {
    self.prepare(ctx).await?;

    info!(
      "[TikTokPublisher] Initiating TikTok publishing: pub_id={} profile={} handle={:?}",
      ctx.publication_id, ctx.browser_profile_id, ctx.target_destination_handle
    );

    // 1. Acquire exclusive lock on target browser profile
    let acquire_req = AcquireWorkerRequest {
      job_id: ctx.job_id.clone(),
      step_id: format!("publish_tt_{}", ctx.publication_id),
      attempt_id: format!("attempt_{}", ctx.publication_id),
      capability: "social.tiktok.publish".to_string(),
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
      "requestId": format!("req_tt_{}", ctx.publication_id),
      "jobId": ctx.job_id,
      "publicationId": ctx.publication_id,
      "leaseId": lease_id,
      "profileId": ctx.browser_profile_id,
      "platform": "tiktok",
      "method": "social.tiktok.video.publish",
      "params": {
        "targetHandle": ctx.target_destination_handle,
        "videoPath": ctx.video_path,
        "title": ctx.title,
        "caption": ctx.caption,
        "hashtags": ctx.hashtags,
        "idempotencyKey": ctx.idempotency_key
      }
    });

    info!("[TikTokPublisher] Dispatching payload to worker: {}", lease.worker_id);
    let resp = client.post(&dispatch_url).json(&payload).send().await;

    // Ensure lease is released
    let _ = release_lease(&lease_id).await;

    match resp {
      Ok(res) if res.status().is_success() => {
        let body: serde_json::Value = res.json().await.map_err(|e| {
          PublisherError::new(PublisherErrorCode::UploadFailed, format!("Failed to parse response: {e}"), false)
        })?;

        // Fail-closed check: HTTP 200 with body.ok == false or error property MUST be treated as failure
        if body.get("ok").and_then(|v| v.as_bool()) == Some(false) {
          let err_msg = body.get("error").or_else(|| body.get("message")).and_then(|v| v.as_str()).unwrap_or("TikTok extension reported ok=false");
          let err_code = body.get("code").and_then(|v| v.as_str()).unwrap_or("UPLOAD_FAILED");
          if err_code.contains("AUTH") || err_msg.contains("login") || err_msg.contains("AUTH_REQUIRED") {
            return Err(PublisherError::auth_required(err_msg));
          } else if err_code.contains("CAPABILITY") || err_msg.contains("not implemented") || err_msg.contains("unsupported") {
            return Err(PublisherError::new(PublisherErrorCode::CapabilityUnavailable, err_msg, false));
          } else {
            return Err(PublisherError::upload_failed(err_msg, false));
          }
        }

        let post_id = body.get("postId").and_then(|v| v.as_str()).map(|s| s.to_string())
          .or_else(|| body.get("itemId").and_then(|v| v.as_str()).map(|s| s.to_string()))
          .or_else(|| body.get("platform_post_id").and_then(|v| v.as_str()).map(|s| s.to_string()));
        let post_url = body.get("postUrl").and_then(|v| v.as_str()).map(|s| s.to_string())
          .or_else(|| body.get("shareUrl").and_then(|v| v.as_str()).map(|s| s.to_string()))
          .or_else(|| body.get("post_url").and_then(|v| v.as_str()).map(|s| s.to_string()));

        // Evidence validation: Require post_id or post_url. If missing, fail with VerificationRequired, NEVER fake POSTED
        if post_id.is_none() && post_url.is_none() {
          return Err(PublisherError::new(
            PublisherErrorCode::VerificationRequired,
            "TikTok posting completed without authoritative post_id or post_url evidence from runtime extension",
            false,
          ));
        }

        info!("[TikTokPublisher] Published successfully with evidence! post_id={:?} url={:?}", post_id, post_url);
        Ok(PublicationResult {
          platform_post_id: post_id.clone(),
          post_url: post_url.or_else(|| post_id.as_ref().map(|id| format!("https://www.tiktok.com/@video/{id}"))),
          posted_at: chrono::Utc::now().timestamp(),
          raw_metadata: Some(body),
        })
      }
      Ok(res) => {
        let status = res.status();
        let body_text = res.text().await.unwrap_or_default();
        error!("[TikTokPublisher] TikTok publish returned {status}: {body_text}");
        if body_text.contains("AUTH_REQUIRED") || body_text.contains("login") {
          Err(PublisherError::auth_required(body_text))
        } else {
          Err(PublisherError::upload_failed(format!("TikTok publish failed ({status}): {body_text}"), true))
        }
      }
      Err(err) => {
        error!("[TikTokPublisher] Network error during dispatch: {err}");
        Err(PublisherError::new(
          PublisherErrorCode::VerifyFailed,
          format!("Network disconnected during TikTok publish, verification required: {err}"),
          false,
        ))
      }
    }
  }

  async fn verify(&self, ctx: &PublicationExecutionContext) -> Result<Option<PublicationResult>, PublisherError> {
    info!("[TikTokPublisher] Verifying publication status for pub_id={} key={}", ctx.publication_id, ctx.idempotency_key);
    let base_url = crate::services::pipeline::clients::browser_runtime_client::get_donut_browser_api_base_url();
    let client = Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| {
      PublisherError::new(PublisherErrorCode::NetworkError, e.to_string(), true)
    })?;

    let verify_url = format!("{base_url}/v1/publications/verify");
    let payload = serde_json::json!({
      "publicationId": ctx.publication_id,
      "platform": "tiktok",
      "profileId": ctx.browser_profile_id,
      "targetHandle": ctx.target_destination_handle,
      "idempotencyKey": ctx.idempotency_key
    });

    if let Ok(res) = client.post(&verify_url).json(&payload).send().await {
      if res.status().is_success() {
        if let Ok(body) = res.json::<serde_json::Value>().await {
          if body.get("verified").and_then(|v| v.as_bool()).unwrap_or(false) {
            let post_id = body.get("postId").and_then(|v| v.as_str()).map(|s| s.to_string());
            let post_url = body.get("postUrl").and_then(|v| v.as_str()).map(|s| s.to_string());
            return Ok(Some(PublicationResult {
              platform_post_id: post_id,
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
    info!("[TikTokPublisher] Cancelling TikTok publication pub_id={}", ctx.publication_id);
    Ok(())
  }
}
