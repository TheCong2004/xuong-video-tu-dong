use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationExecutionContext {
  pub publication_id: String,
  pub job_id: String,
  pub page_id: String,
  pub platform: String,
  pub browser_profile_id: String,
  pub video_path: String,
  pub title: Option<String>,
  pub caption: Option<String>,
  pub hashtags: Vec<String>,
  pub description: Option<String>,
  pub target_destination_id: String,
  pub target_destination_handle: Option<String>,
  pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationResult {
  pub platform_post_id: Option<String>,
  pub post_url: Option<String>,
  pub posted_at: i64,
  pub raw_metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublisherErrorCode {
  AuthRequired,
  ProfileOffline,
  VideoNotFound,
  UploadFailed,
  CaptionRejected,
  PlatformRejected,
  NetworkError,
  Timeout,
  TargetNotFound,
  TargetAmbiguous,
  VerifyFailed,
  VerificationRequired,
  CapabilityUnavailable,
  Unknown,
}

impl PublisherErrorCode {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::AuthRequired => "AUTH_REQUIRED",
      Self::ProfileOffline => "PROFILE_OFFLINE",
      Self::VideoNotFound => "VIDEO_NOT_FOUND",
      Self::UploadFailed => "UPLOAD_FAILED",
      Self::CaptionRejected => "CAPTION_REJECTED",
      Self::PlatformRejected => "PLATFORM_REJECTED",
      Self::NetworkError => "NETWORK_ERROR",
      Self::Timeout => "TIMEOUT",
      Self::TargetNotFound => "TARGET_NOT_FOUND",
      Self::TargetAmbiguous => "TARGET_AMBIGUOUS",
      Self::VerifyFailed => "VERIFY_FAILED",
      Self::VerificationRequired => "VERIFY_REQUIRED",
      Self::CapabilityUnavailable => "CAPABILITY_UNAVAILABLE",
      Self::Unknown => "UNKNOWN",
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherError {
  pub code: PublisherErrorCode,
  pub message: String,
  pub retryable: bool,
}

impl PublisherError {
  pub fn new(code: PublisherErrorCode, message: impl Into<String>, retryable: bool) -> Self {
    Self {
      code,
      message: message.into(),
      retryable,
    }
  }

  pub fn auth_required(message: impl Into<String>) -> Self {
    Self::new(PublisherErrorCode::AuthRequired, message, false)
  }

  pub fn profile_offline(message: impl Into<String>) -> Self {
    Self::new(PublisherErrorCode::ProfileOffline, message, true)
  }

  pub fn target_ambiguous(message: impl Into<String>) -> Self {
    Self::new(PublisherErrorCode::TargetAmbiguous, message, false)
  }

  pub fn upload_failed(message: impl Into<String>, retryable: bool) -> Self {
    Self::new(PublisherErrorCode::UploadFailed, message, retryable)
  }

  pub fn verify_failed(message: impl Into<String>) -> Self {
    Self::new(PublisherErrorCode::VerifyFailed, message, false)
  }
}

impl std::fmt::Display for PublisherError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[{:?}] {} (retryable={})", self.code, self.message, self.retryable)
  }
}

impl std::error::Error for PublisherError {}

#[async_trait]
pub trait PublisherAdapter: Send + Sync {
  async fn prepare(&self, ctx: &PublicationExecutionContext) -> Result<(), PublisherError>;
  async fn validate_session(&self, ctx: &PublicationExecutionContext) -> Result<bool, PublisherError>;
  async fn publish(&self, ctx: &PublicationExecutionContext) -> Result<PublicationResult, PublisherError>;
  async fn verify(&self, ctx: &PublicationExecutionContext) -> Result<Option<PublicationResult>, PublisherError>;
  async fn cancel_if_supported(&self, ctx: &PublicationExecutionContext) -> Result<(), PublisherError>;
}
