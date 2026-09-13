use thiserror::Error;

#[derive(Debug, Error)]
pub enum VoiceStudioClientError {
  #[error("VoiceStudio is unavailable: {0}")]
  Unavailable(String),
  #[error("VoiceStudio request failed with HTTP {status}: {body}")]
  Http { status: u16, body: String },
  #[error("VoiceStudio returned an invalid response: {0}")]
  InvalidResponse(String),
  #[error("VoiceStudio request was cancelled")]
  Cancelled,
  #[error(transparent)]
  Transport(#[from] reqwest::Error),
  #[error(transparent)]
  Io(#[from] std::io::Error),
}
