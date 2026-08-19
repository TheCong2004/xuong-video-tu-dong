use super::dispatch_protocol::{parse_dispatch_body, validate_dispatch_response};
use super::publisher_adapter::{PublicationExecutionContext, PublicationResult, PublisherAdapter, PublisherError, PublisherErrorCode};
use crate::services::pipeline::clients::browser_runtime_client::{acquire_worker, release_lease, AcquireWorkerRequest};
use async_trait::async_trait;
use log::{error, info};
use reqwest::Client;
use std::path::Path;
use std::time::Duration;

pub struct FacebookPublisherAdapter;

impl FacebookPublisherAdapter {
  pub fn new() -> Self {
    Self
  }
}

#[async_trait]
impl PublisherAdapter for FacebookPublisherAdapter {
  async fn prepare(&self, ctx: &PublicationExecutionContext) -> Result<(), PublisherError> {
    info!("[FacebookPublisher] Preparing execution for pub_id={} target={}", ctx.publication_id, ctx.target_destination_id);
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
    info!("[FacebookPublisher] Validating session for profile={}", ctx.browser_profile_id);
    let req = AcquireWorkerRequest {
      job_id: ctx.job_id.clone(),
      step_id: format!("publish_facebook_{}_session_check", ctx.publication_id),
      attempt_id: "val_session".to_string(),
      capability: "social.facebook.publish".to_string(),
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
          Err(PublisherError::auth_required(format!("Facebook session expired or not authenticated in profile '{}'", ctx.browser_profile_id)))
        } else {
          Err(PublisherError::profile_offline(format!("Browser profile '{}' offline or unavailable: {}", ctx.browser_profile_id, err)))
        }
      }
    }
  }

  async fn publish(&self, ctx: &PublicationExecutionContext) -> Result<PublicationResult, PublisherError> {
    self.prepare(ctx).await?;

    info!(
      "[FacebookPublisher] Initiating Facebook publishing: pub_id={} profile={} page_id={}",
      ctx.publication_id, ctx.browser_profile_id, ctx.target_destination_id
    );

    // 1. Acquire exclusive lock on target browser profile
    let step_id = format!("publish_facebook_{}", ctx.publication_id);
    let acquire_req = AcquireWorkerRequest {
      job_id: ctx.job_id.clone(),
      step_id: step_id.clone(),
      attempt_id: format!("attempt_{}", ctx.publication_id),
      capability: "social.facebook.publish".to_string(),
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

    // 2. Dispatch using canonical floword-production v1 request envelope
    let base_url = crate::services::pipeline::clients::browser_runtime_client::get_donut_browser_api_base_url();
    let client = Client::builder().timeout(Duration::from_secs(180)).build().map_err(|e| {
      PublisherError::new(PublisherErrorCode::NetworkError, e.to_string(), true)
    })?;

    let dispatch_url = format!("{base_url}/v1/workers/{}/dispatch", lease.worker_id);
    let payload = serde_json::json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": format!("req_fb_{}", ctx.publication_id),
      "jobId": ctx.job_id,
      "stepId": step_id,
      "attemptId": format!("attempt_{}", ctx.publication_id),
      "leaseId": lease_id,
      "profileId": ctx.browser_profile_id,
      "pageId": ctx.target_destination_id,
      "method": "social.facebook.reels.publish",
      "params": {
        "publicationId": ctx.publication_id,
        "targetPageId": ctx.target_destination_id,
        "targetPageHandle": ctx.target_destination_handle,
        "videoPath": ctx.video_path,
        "title": ctx.title,
        "caption": ctx.caption,
        "hashtags": ctx.hashtags,
        "idempotencyKey": ctx.idempotency_key
      }
    });

    info!("[FacebookPublisher] Dispatching to worker {} step={}", lease.worker_id, step_id);
    let resp = client.post(&dispatch_url).json(&payload).send().await;

    // Ensure lease is always released
    let _ = release_lease(&lease_id).await;

    match resp {
      Ok(res) if res.status().is_success() => {
        // Parse raw JSON — protocol violation if unparseable
        let raw: serde_json::Value = res.json().await.map_err(|e| {
          PublisherError::new(
            PublisherErrorCode::PlatformRejected,
            format!("[Facebook] Failed to parse dispatch response (protocol violation): {e}"),
            false,
          )
        })?;

        // Decode into typed struct and classify errors using canonical error taxonomy
        let parsed = parse_dispatch_body(raw.clone(), "Facebook")?;
        let result = validate_dispatch_response(parsed, "Facebook")?;

        // Extract authoritative evidence from body.result (preferred) then root fallback
        let result_obj = result.as_ref();
        let post_id = result_obj
          .and_then(|r| r.get("postId").or_else(|| r.get("platform_post_id")))
          .and_then(|v| v.as_str())
          .map(|s| s.to_string())
          .or_else(|| raw.get("postId").and_then(|v| v.as_str()).map(|s| s.to_string()));
        let post_url = result_obj
          .and_then(|r| r.get("postUrl").or_else(|| r.get("post_url")))
          .and_then(|v| v.as_str())
          .map(|s| s.to_string())
          .or_else(|| raw.get("postUrl").and_then(|v| v.as_str()).map(|s| s.to_string()));

        // Evidence validation: require post_id or post_url
        // NEVER construct a URL from post_id — Facebook post IDs cannot be reliably converted to URLs
        if post_id.is_none() && post_url.is_none() {
          return Err(PublisherError::new(
            PublisherErrorCode::VerificationRequired,
            "Facebook posting completed without authoritative post_id or post_url evidence from runtime extension",
            false,
          ));
        }

        info!("[FacebookPublisher] Published successfully. post_id={:?} post_url={:?}", post_id, post_url);
        Ok(PublicationResult {
          platform_post_id: post_id,
          post_url, // stored as-is; never fabricated from post_id
          posted_at: chrono::Utc::now().timestamp(),
          raw_metadata: Some(raw),
        })
      }
      Ok(res) => {
        let status = res.status();
        let body_text = res.text().await.unwrap_or_default();
        error!("[FacebookPublisher] HTTP {status}: {body_text}");
        if body_text.contains("AUTH_REQUIRED") || body_text.contains("login") {
          Err(PublisherError::auth_required(body_text))
        } else if body_text.contains("TARGET_AMBIGUOUS") {
          Err(PublisherError::target_ambiguous(body_text))
        } else {
          Err(PublisherError::upload_failed(format!("Facebook publish failed ({status}): {body_text}"), true))
        }
      }
      Err(err) => {
        error!("[FacebookPublisher] Network error during dispatch: {err}");
        // Network timeout during possible in-flight upload — outcome unknown
        Err(PublisherError::new(
          PublisherErrorCode::VerifyFailed,
          format!("Network disconnected during publish, verification required: {err}"),
          false,
        ))
      }
    }
  }

  async fn verify(&self, ctx: &PublicationExecutionContext) -> Result<Option<PublicationResult>, PublisherError> {
    info!("[FacebookPublisher] Verifying publication status for pub_id={} key={}", ctx.publication_id, ctx.idempotency_key);
    let base_url = crate::services::pipeline::clients::browser_runtime_client::get_donut_browser_api_base_url();
    let client = Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| {
      PublisherError::new(PublisherErrorCode::NetworkError, e.to_string(), true)
    })?;

    let verify_url = format!("{base_url}/v1/publications/verify");
    let payload = serde_json::json!({
      "publicationId": ctx.publication_id,
      "platform": "facebook",
      "profileId": ctx.browser_profile_id,
      "targetPageId": ctx.target_destination_id,
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
    info!("[FacebookPublisher] Cancelling Facebook publication pub_id={}", ctx.publication_id);
    Ok(())
  }
}
