use crate::{DubRequest, SpeechRequest, SpeechResponse, TranscriptionRequest, TranscriptionResponse, TranslateRequest, VoiceStudioClient, VoiceStudioClientError};
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Capability facade used by ArtCraft's orchestration layer.
///
/// Implementations own transport and engine-specific details; callers only
/// depend on speech capabilities. This keeps the UI and pipeline independent
/// from VoiceStudio's Python/Bun runtime and leaves room for another provider.
#[async_trait]
pub trait VoiceProvider: Send + Sync {
  async fn health(&self) -> Result<Value, VoiceStudioClientError>;
  async fn capabilities(&self) -> Result<Value, VoiceStudioClientError>;
  async fn accept_model_terms(&self) -> Result<Value, VoiceStudioClientError>;
  async fn voices(&self) -> Result<Value, VoiceStudioClientError>;
  async fn voice_profiles(&self) -> Result<Value, VoiceStudioClientError>;
  async fn upload_voice_clip(&self, path: &Path, name: &str, reference_text: Option<&str>) -> Result<Value, VoiceStudioClientError>;
  async fn save_voice_as_profile(&self, voice_id: &str, profile_name: &str) -> Result<Value, VoiceStudioClientError>;
  async fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechResponse, VoiceStudioClientError>;
  async fn synthesize_batch(&self, requests: Vec<SpeechRequest>, max_concurrency: usize) -> Result<Vec<SpeechResponse>, VoiceStudioClientError>;
  async fn transcribe(&self, path: &Path, request: &TranscriptionRequest) -> Result<TranscriptionResponse, VoiceStudioClientError>;
  async fn dub_upload(&self, path: &Path, job_id: Option<&str>, input_type: &str, source_lang: Option<&str>) -> Result<Value, VoiceStudioClientError>;
  async fn dub_transcribe(&self, job_id: &str, num_speakers: Option<u16>) -> Result<Value, VoiceStudioClientError>;
  async fn wait_for_task(&self, task_id: &str, timeout: Duration) -> Result<Value, VoiceStudioClientError>;
  async fn dub_translate(&self, request: &TranslateRequest) -> Result<Value, VoiceStudioClientError>;
  async fn dub_generate(&self, job_id: &str, request: &DubRequest) -> Result<Value, VoiceStudioClientError>;
}

#[async_trait]
impl VoiceProvider for VoiceStudioClient {
  async fn health(&self) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::health(self).await
  }
  async fn capabilities(&self) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::capabilities(self).await
  }
  async fn accept_model_terms(&self) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::accept_model_terms(self).await
  }
  async fn voices(&self) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::voices(self).await
  }
  async fn voice_profiles(&self) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::voice_profiles(self).await
  }
  async fn upload_voice_clip(&self, path: &Path, name: &str, reference_text: Option<&str>) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::upload_voice_clip(self, path, name, reference_text).await
  }
  async fn save_voice_as_profile(&self, voice_id: &str, profile_name: &str) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::save_voice_as_profile(self, voice_id, profile_name).await
  }
  async fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechResponse, VoiceStudioClientError> {
    VoiceStudioClient::synthesize(self, request).await
  }
  async fn synthesize_batch(&self, requests: Vec<SpeechRequest>, max_concurrency: usize) -> Result<Vec<SpeechResponse>, VoiceStudioClientError> {
    VoiceStudioClient::synthesize_batch(self, requests, max_concurrency).await
  }
  async fn transcribe(&self, path: &Path, request: &TranscriptionRequest) -> Result<TranscriptionResponse, VoiceStudioClientError> {
    VoiceStudioClient::transcribe(self, path, request).await
  }
  async fn dub_upload(&self, path: &Path, job_id: Option<&str>, input_type: &str, source_lang: Option<&str>) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::dub_upload(self, path, job_id, input_type, source_lang).await
  }
  async fn dub_transcribe(&self, job_id: &str, num_speakers: Option<u16>) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::dub_transcribe(self, job_id, num_speakers).await
  }
  async fn wait_for_task(&self, task_id: &str, timeout: Duration) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::wait_for_task(self, task_id, timeout).await
  }
  async fn dub_translate(&self, request: &TranslateRequest) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::dub_translate(self, request).await
  }
  async fn dub_generate(&self, job_id: &str, request: &DubRequest) -> Result<Value, VoiceStudioClientError> {
    VoiceStudioClient::dub_generate(self, job_id, request).await
  }
}
