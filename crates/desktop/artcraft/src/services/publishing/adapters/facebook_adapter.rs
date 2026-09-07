use super::dispatch_protocol::{parse_dispatch_body, validate_dispatch_response, ExpectedDispatchIdentity};
use super::publisher_adapter::{PublicationExecutionContext, PublicationResult, PublisherAdapter, PublisherError, PublisherErrorCode};
use crate::services::pipeline::clients::browser_runtime_backend::{BrowserIdentity, BrowserRuntimeBackend, BrowserSession};
use crate::services::publishing::facebook_binding::FacebookRuntimeIdentity;
use async_trait::async_trait;
use log::info;
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct FacebookPublisherAdapter;
impl FacebookPublisherAdapter { pub fn new() -> Self { Self } }

fn facebook_url(url: &str) -> bool {
  url::Url::parse(url).ok().and_then(|u| {
    if u.scheme() != "https" { return None; }
    let host = u.host_str()?.to_ascii_lowercase();
    Some(host == "facebook.com" || host.ends_with(".facebook.com"))
  }).unwrap_or(false)
}

fn identity_from_page(session: &BrowserSession, target_id: &str, page_url: &str) -> BrowserIdentity {
  BrowserIdentity { profile_id: session.profile_id.clone(), browser_pid: session.browser_pid, remote_debugging_port: session.remote_debugging_port, cdp_endpoint: session.cdp_endpoint.clone(), launch_generation: session.launch_generation, browser_engine: session.browser_engine.clone(), grok_target_id: String::new(), grok_page_url: String::new(), target_kind: "FACEBOOK".into(), managed_target_id: Some(target_id.into()), managed_page_url: Some(page_url.into()), reused: session.reused }
}

fn runtime_identity_from_page(session: &BrowserSession, target_id: &str, page_url: &str) -> FacebookRuntimeIdentity {
  FacebookRuntimeIdentity { browser_pid: session.browser_pid, launch_generation: session.launch_generation, cdp_endpoint: session.cdp_endpoint.clone(), managed_target_id: target_id.to_string(), managed_page_url: page_url.to_string(), claimed_at_utc: 0, last_validated_at_utc: 0 }
}

#[async_trait]
impl PublisherAdapter for FacebookPublisherAdapter {
  async fn prepare(&self, ctx: &PublicationExecutionContext) -> Result<(), PublisherError> {
    if !Path::new(&ctx.video_path).is_file() { return Err(PublisherError::new(PublisherErrorCode::VideoNotFound, "Facebook video path does not exist", false)); }
    Ok(())
  }

  async fn validate_session(&self, ctx: &PublicationExecutionContext) -> Result<bool, PublisherError> {
    self.prepare(ctx).await?;
    let expected_target = ctx.managed_facebook_target_id.as_deref().ok_or_else(|| PublisherError::new(PublisherErrorCode::TargetNotFound, "Select an exact managed Facebook target before publishing", false))?;
    let backend = BrowserRuntimeBackend::local();
    let session = backend.run_browser(&ctx.browser_profile_id, serde_json::json!({"targetKind":"FACEBOOK","browserEngine":"CHROME_FOR_TESTING","coldStartOnly":true,"headless":false})).await.map_err(PublisherError::profile_offline)?;
    let pages = backend.list_pages(&ctx.browser_profile_id).await.map_err(PublisherError::profile_offline)?;
    let candidate = pages.pages.iter().find(|p| p.target_id == expected_target).ok_or_else(|| PublisherError::new(PublisherErrorCode::TargetNotFound, "Configured Facebook target is no longer present", false))?;
    if candidate.page_type.as_deref().unwrap_or("page") != "page" || !facebook_url(&candidate.url) { return Err(PublisherError::new(PublisherErrorCode::TargetNotFound, "Configured target is not a Facebook page", false)); }
    let identity = identity_from_page(&session, &candidate.target_id, &candidate.url);
    if let Some(binding) = ctx.facebook_runtime_binding.as_ref() {
      let mut runtime_identity = runtime_identity_from_page(&session, &candidate.target_id, &candidate.url);
      if let Some(stored) = binding.runtime.as_ref() {
        runtime_identity.claimed_at_utc = stored.claimed_at_utc;
        runtime_identity.last_validated_at_utc = stored.last_validated_at_utc;
      }
      if !binding.runtime_is_current(&ctx.browser_profile_id, &runtime_identity, true) {
        return Err(PublisherError::new(PublisherErrorCode::TargetNotFound, "Facebook runtime binding is stale; reconcile the live PID and generation", false));
      }
    }
    validate_page_snapshot(ctx, &candidate.url)?;
    let status = backend.start_worker(&ctx.browser_profile_id, &identity).await.map_err(PublisherError::profile_offline)?;
    if !status.extension_ready || !matches!(status.state.as_str(), "READY" | "IDLE") { return Err(PublisherError::auth_required("Facebook session is not ready")); }
    Ok(true)
  }

  async fn publish(&self, ctx: &PublicationExecutionContext) -> Result<PublicationResult, PublisherError> {
    self.prepare(ctx).await?;
    let caption = [ctx.caption.clone(), ctx.description.clone(), (!ctx.hashtags.is_empty()).then(|| ctx.hashtags.join(" "))].into_iter().flatten().collect::<Vec<_>>().join(" ");
    validate_live_confirmation(ctx, &caption)?;
    let expected_target = ctx.managed_facebook_target_id.as_deref().ok_or_else(|| PublisherError::new(PublisherErrorCode::TargetNotFound, "Select an exact managed Facebook target before publishing", false))?;
    let backend = BrowserRuntimeBackend::local();
    let request_id = format!("req_fb_{}_{}_{}", ctx.publication_id, ctx.attempt_number, uuid::Uuid::new_v4());
    let step_id = format!("publish_facebook_{}", ctx.publication_id);
    let attempt_id = format!("pub_{}_attempt_{}", ctx.publication_id, ctx.attempt_number);
    let session = backend.run_browser(&ctx.browser_profile_id, serde_json::json!({"targetKind":"FACEBOOK","browserEngine":"CHROME_FOR_TESTING","coldStartOnly":true,"headless":false})).await.map_err(PublisherError::profile_offline)?;
    let pages = backend.list_pages(&ctx.browser_profile_id).await.map_err(PublisherError::profile_offline)?;
    let candidate = pages.pages.iter().find(|p| p.target_id == expected_target).ok_or_else(|| PublisherError::new(PublisherErrorCode::TargetNotFound, "Configured Facebook target is stale", false))?;
    if candidate.page_type.as_deref().unwrap_or("page") != "page" || !facebook_url(&candidate.url) { return Err(PublisherError::new(PublisherErrorCode::TargetNotFound, "Configured target is not a Facebook page", false)); }
    if let Some(binding) = ctx.facebook_runtime_binding.as_ref() {
      let mut live_identity = runtime_identity_from_page(&session, &candidate.target_id, &candidate.url);
      if let Some(stored) = binding.runtime.as_ref() {
        live_identity.claimed_at_utc = stored.claimed_at_utc;
        live_identity.last_validated_at_utc = stored.last_validated_at_utc;
      }
      if !binding.runtime_is_current(&ctx.browser_profile_id, &live_identity, true) {
        return Err(PublisherError::new(PublisherErrorCode::TargetNotFound, "Facebook runtime binding is stale; reconcile the live PID and generation", false));
      }
    }
    validate_page_snapshot(ctx, &candidate.url)?;
    let page_lease = backend.claim_page_exact(&ctx.browser_profile_id, &ctx.job_id, &request_id, "FACEBOOK_AUTOMATION", "FACEBOOK", &candidate.target_id).await.map_err(PublisherError::target_ambiguous)?;
    let identity = identity_from_page(&session, &page_lease.target_id, &candidate.url);
    backend.start_worker(&ctx.browser_profile_id, &identity).await.map_err(PublisherError::profile_offline)?;
    let lease_id = format!("local-fb-{}", uuid::Uuid::new_v4());
    let expected = ExpectedDispatchIdentity { request_id: request_id.clone(), job_id: ctx.job_id.clone(), step_id: step_id.clone(), attempt_id: attempt_id.clone(), lease_id: lease_id.clone(), profile_id: ctx.browser_profile_id.clone() };
    let payload = serde_json::json!({"protocol":"floword-production","protocolVersion":1,"requestId":request_id,"jobId":ctx.job_id,"stepId":step_id,"attemptId":attempt_id,"leaseId":lease_id,"profileId":ctx.browser_profile_id,"targetKind":"FACEBOOK","targetId":page_lease.target_id,"pageId":ctx.page_id,"method":"social.facebook.reels.publish","params":{"publicationId":ctx.publication_id,"targetPageId":ctx.facebook_page_id.as_deref().unwrap_or(&ctx.target_destination_id),"targetPageHandle":ctx.target_destination_handle,"facebookPageCanonicalUrl":ctx.facebook_page_canonical_url,"facebookPageDisplayName":ctx.facebook_page_display_name,"videoPath":ctx.video_path,"title":ctx.title,"caption":caption,"visibleLink":ctx.visible_link,"idempotencyKey":ctx.idempotency_key}});
    info!("[FacebookPublisher] dispatching exact managed target for publication {}", ctx.publication_id);
    let raw = backend.dispatch_sidecar(&ctx.browser_profile_id, payload).await.map_err(|e| PublisherError::new(PublisherErrorCode::VerifyFailed, format!("Facebook dispatch outcome unknown: {e}"), false))?;
    let _ = backend.release_page(&ctx.browser_profile_id, &page_lease.target_id, &page_lease.page_lease_id, &ctx.job_id, &request_id).await;
    let parsed = parse_dispatch_body(raw.clone(), "Facebook")?;
    let result = validate_dispatch_response(parsed, &expected, "Facebook")?;
    let post_id = result.as_ref().and_then(|r| r.get("postId")).and_then(|v| v.as_str()).map(str::to_string);
    let post_url = result.as_ref().and_then(|r| r.get("postUrl")).and_then(|v| v.as_str()).map(str::to_string);
    if post_id.is_none() && post_url.is_none() { return Err(PublisherError::new(PublisherErrorCode::VerificationRequired, "Facebook post has no authoritative verification evidence", false)); }
    Ok(PublicationResult { platform_post_id: post_id, post_url, posted_at: chrono::Utc::now().timestamp(), raw_metadata: Some(raw) })
  }

  async fn verify(&self, _ctx: &PublicationExecutionContext) -> Result<Option<PublicationResult>, PublisherError> { Ok(None) }
  async fn cancel_if_supported(&self, _ctx: &PublicationExecutionContext) -> Result<(), PublisherError> { Ok(()) }
}

fn validate_page_snapshot(ctx: &PublicationExecutionContext, current_url: &str) -> Result<(), PublisherError> {
  let Some(snapshot) = ctx.facebook_page_snapshot.as_ref() else {
    return Err(PublisherError::new(PublisherErrorCode::TargetNotFound, "Facebook Page identity snapshot is missing", false));
  };
  let expected_url = snapshot.facebook_page_canonical_url.as_deref();
  let display_name = ctx.facebook_page_display_name.as_deref().unwrap_or_default();
  let page_id = ctx.facebook_page_id.as_deref();
  if !snapshot.matches_current(page_id, Some(current_url), display_name, true) {
    return Err(PublisherError::new(PublisherErrorCode::TargetNotFound, "Facebook Page identity does not match the persisted snapshot", false));
  }
  if let Some(url) = expected_url {
    if url != current_url { return Err(PublisherError::new(PublisherErrorCode::TargetNotFound, "Facebook Page canonical URL changed", false)); }
  }
  Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_live_confirmation(ctx: &PublicationExecutionContext, caption: &str) -> Result<(), PublisherError> {
  let Some(occurrence_id) = ctx.scheduled_occurrence_id.as_deref() else {
    return Err(PublisherError::new(PublisherErrorCode::VerificationRequired, "Live publish requires an exact scheduled occurrence confirmation", false));
  };
  let Some(confirmation) = ctx.live_publish_confirmation.as_ref() else {
    return Err(PublisherError::new(PublisherErrorCode::VerificationRequired, "LIVE_PUBLISH_CONFIRMED is not present for this publication", false));
  };
  let video_bytes = std::fs::read(&ctx.video_path).map_err(|_| PublisherError::new(PublisherErrorCode::VideoNotFound, "Facebook video path does not exist", false))?;
  let caption_sha = sha256_hex(caption.as_bytes());
  let video_sha = sha256_hex(&video_bytes);
  let page_id = ctx.facebook_page_id.as_deref();
  if !confirmation.is_valid_for(&ctx.publication_id, occurrence_id, &ctx.browser_profile_id, page_id, &video_sha, &caption_sha, ctx.visible_link.as_deref(), chrono::Utc::now().timestamp()) {
    return Err(PublisherError::new(PublisherErrorCode::VerificationRequired, "Live publish confirmation does not match the current publication snapshot", false));
  }
  Ok(())
}
