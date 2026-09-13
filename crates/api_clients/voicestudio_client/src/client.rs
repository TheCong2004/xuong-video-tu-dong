use crate::error::VoiceStudioClientError;
use crate::types::{DubRequest, SpeechRequest, SpeechResponse, TranscriptionRequest, TranscriptionResponse, TranslateRequest};
use futures::stream::{self, StreamExt, TryStreamExt};
use reqwest::{header, multipart, Client, StatusCode};
use serde::{Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:3900";

#[derive(Clone)]
pub struct VoiceStudioClient {
  client: Client,
  base_url: String,
  api_key: Option<String>,
}

impl VoiceStudioClient {
  pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Result<Self, VoiceStudioClientError> {
    Self::new_with_timeout(base_url, api_key, Duration::from_secs(120))
  }

  fn new_with_timeout(base_url: impl Into<String>, api_key: Option<String>, timeout: Duration) -> Result<Self, VoiceStudioClientError> {
    let client = Client::builder().connect_timeout(Duration::from_secs(3)).timeout(timeout).build()?;
    Ok(Self { client, base_url: normalize_base_url(base_url.into()), api_key })
  }

  pub fn from_env() -> Result<Self, VoiceStudioClientError> {
    let base_url = std::env::var("VOICESTUDIO_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    let api_key = std::env::var("VOICESTUDIO_API_KEY").ok().filter(|value| !value.trim().is_empty());
    let timeout_seconds = std::env::var("VOICESTUDIO_REQUEST_TIMEOUT_SECONDS").ok().and_then(|value| value.trim().parse::<u64>().ok()).map(|value| value.clamp(30, 3_600)).unwrap_or(120);
    Self::new_with_timeout(base_url, api_key, Duration::from_secs(timeout_seconds))
  }

  pub fn base_url(&self) -> &str {
    &self.base_url
  }

  pub async fn health(&self) -> Result<Value, VoiceStudioClientError> {
    self.get_json("/health").await
  }

  pub async fn capabilities(&self) -> Result<Value, VoiceStudioClientError> {
    self.get_json("/.well-known/voicestudio-speech").await
  }

  /// Records explicit acceptance of the local model's separately supplied
  /// terms. This is intentionally an opt-in action, never an implicit start.
  pub async fn accept_model_terms(&self) -> Result<Value, VoiceStudioClientError> {
    self.post_json("/settings/model-terms", &json!({ "accepted": true })).await
  }

  pub async fn voices(&self) -> Result<Value, VoiceStudioClientError> {
    self.get_json("/v1/audio/voices").await
  }

  /// Lists persisted profiles usable directly for cloned-voice synthesis.
  ///
  /// Gallery uploads are not profiles.  Returning gallery rows here used to
  /// make ArtCraft save an ID that the TTS endpoint could not resolve.
  pub async fn voice_profiles(&self) -> Result<Value, VoiceStudioClientError> {
    self.get_json("/profiles").await
  }

  /// Creates a persistent local voice profile from a consented reference clip.
  /// `reference_text` is optional: ArtCraft Speech/OmniVoice transcribes the
  /// reference on first use when it was not supplied by the user.
  pub async fn upload_voice_clip(&self, path: &Path, name: &str, reference_text: Option<&str>) -> Result<Value, VoiceStudioClientError> {
    let bytes = tokio::fs::read(path).await?;
    let filename = path.file_name().and_then(|value| value.to_str()).unwrap_or("voice.wav").to_string();
    let mut form = multipart::Form::new().text("name", name.to_string()).text("consent", "true").part("audio", multipart::Part::bytes(bytes).file_name(filename));
    if let Some(value) = reference_text.filter(|value| !value.trim().is_empty()) {
      form = form.text("reference_text", value.to_string());
    }
    let response = self.authorize(self.client.post(self.url("/profiles"))).multipart(form).send().await?;
    let response = self.ensure_success(response).await?;
    Ok(response.json().await?)
  }

  /// Promotes a gallery clip into a persistent VoiceStudio voice profile.
  pub async fn save_voice_as_profile(&self, voice_id: &str, profile_name: &str) -> Result<Value, VoiceStudioClientError> {
    let path = format!("/gallery/voices/{}/save-as-profile", path_segment(voice_id));
    let response = self.authorize(self.client.post(self.url(&path))).query(&[("profile_name", profile_name)]).send().await?;
    let response = self.ensure_success(response).await?;
    Ok(response.json().await?)
  }

  /// Uploads a video/audio source for the full dubbing workflow. VoiceStudio
  /// returns a job id and an asynchronous preparation task id.
  pub async fn dub_upload(&self, path: &Path, job_id: Option<&str>, input_type: &str, source_lang: Option<&str>) -> Result<Value, VoiceStudioClientError> {
    let bytes = tokio::fs::read(path).await?;
    let filename = path.file_name().and_then(|value| value.to_str()).unwrap_or("input.mp4").to_string();
    let mut form = multipart::Form::new().part("video", multipart::Part::bytes(bytes).file_name(filename)).text("input_type", input_type.to_string());
    if let Some(value) = job_id.filter(|value| !value.trim().is_empty()) {
      form = form.text("job_id", value.to_string());
    }
    if let Some(value) = source_lang.filter(|value| !value.trim().is_empty()) {
      form = form.text("source_lang", value.to_string());
    }
    let response = self.authorize(self.client.post(self.url("/dub/upload"))).multipart(form).send().await?;
    let response = self.ensure_success(response).await?;
    Ok(response.json().await?)
  }

  /// Runs VoiceStudio's persisted ASR/diarization pass for a dubbing job.
  pub async fn dub_transcribe(&self, job_id: &str, num_speakers: Option<u16>) -> Result<Value, VoiceStudioClientError> {
    let mut request = self.authorize(self.client.post(self.url(&format!("/dub/transcribe/{}", path_segment(job_id)))));
    if let Some(value) = num_speakers {
      request = request.query(&[("num_speakers", value)]);
    }
    let response = request.send().await?;
    let response = self.ensure_success(response).await?;
    Ok(response.json().await?)
  }

  /// Reads a background task status. VoiceStudio persists these rows so a
  /// caller can safely reconnect after a UI refresh or backend event gap.
  pub async fn task_status(&self, task_id: &str) -> Result<Value, VoiceStudioClientError> {
    self.get_json(&format!("/jobs/{}", path_segment(task_id))).await
  }

  /// Waits for a preparation task without busy-spinning. The timeout is
  /// caller-owned so a long video can opt into a larger budget than a short
  /// preview while still having a deterministic upper bound.
  pub async fn wait_for_task(&self, task_id: &str, timeout: Duration) -> Result<Value, VoiceStudioClientError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
      let status = self.task_status(task_id).await?;
      let state = status.get("status").and_then(Value::as_str).unwrap_or_default();
      if matches!(state, "done" | "failed" | "cancelled") {
        return if state == "done" { Ok(status) } else { Err(VoiceStudioClientError::InvalidResponse(format!("VoiceStudio task {task_id} ended with status {state}"))) };
      }
      if tokio::time::Instant::now() >= deadline {
        return Err(VoiceStudioClientError::Unavailable(format!("VoiceStudio task {task_id} timed out")));
      }
      tokio::time::sleep(Duration::from_millis(500)).await;
    }
  }

  /// Translates a VoiceStudio dubbing transcript. `payload` must match the
  /// server's `TranslateRequest` contract and is kept generic for forward
  /// compatibility with new quality/timing fields.
  pub async fn dub_translate(&self, payload: &TranslateRequest) -> Result<Value, VoiceStudioClientError> {
    self.post_json("/dub/translate", payload).await
  }

  /// Starts VoiceStudio's timing-aware dubbing/TTS generation for a job.
  pub async fn dub_generate(&self, job_id: &str, payload: &DubRequest) -> Result<Value, VoiceStudioClientError> {
    self.post_json(&format!("/dub/generate/{}", path_segment(job_id)), payload).await
  }

  pub async fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechResponse, VoiceStudioClientError> {
    let response = self.authorize(self.client.post(self.url("/v1/audio/speech"))).json(request).send().await?;
    let response = self.ensure_success(response).await?;
    let content_type = response.headers().get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).unwrap_or("application/octet-stream").to_string();
    let bytes = response.bytes().await?.to_vec();
    if bytes.is_empty() {
      return Err(VoiceStudioClientError::InvalidResponse("TTS response body is empty".to_string()));
    }
    Ok(SpeechResponse { bytes, content_type })
  }

  /// Synthesize several segments concurrently while preserving input order.
  ///
  /// The concurrency limit is deliberately caller-controlled because local
  /// TTS engines have very different GPU/CPU memory requirements. A zero
  /// value is treated as one to avoid creating an unbounded request fan-out.
  pub async fn synthesize_batch(&self, requests: Vec<SpeechRequest>, max_concurrency: usize) -> Result<Vec<SpeechResponse>, VoiceStudioClientError> {
    let limit = max_concurrency.max(1);
    let results = stream::iter(requests.into_iter().enumerate().map(|(index, request)| async move { self.synthesize(&request).await.map(|response| (index, response)) })).buffer_unordered(limit).try_collect::<Vec<_>>().await?;
    Ok(order_batch_results(results))
  }

  pub async fn transcribe(&self, path: &Path, request: &TranscriptionRequest) -> Result<TranscriptionResponse, VoiceStudioClientError> {
    let bytes = tokio::fs::read(path).await?;
    let filename = path.file_name().and_then(|value| value.to_str()).unwrap_or("audio.bin").to_string();
    let part = multipart::Part::bytes(bytes).file_name(filename);
    let mut form = multipart::Form::new().part("file", part);
    if let Some(model) = request.model.as_deref() {
      form = form.text("model", model.to_string());
    }
    if let Some(language) = request.language.as_deref() {
      form = form.text("language", language.to_string());
    }
    let response_format = if request.response_format.trim().is_empty() { "verbose_json" } else { request.response_format.as_str() };
    form = form.text("response_format", response_format.to_string());
    let response = self.authorize(self.client.post(self.url("/v1/audio/transcriptions"))).multipart(form).send().await?;
    let response = self.ensure_success(response).await?;
    let body = response.text().await?;
    serde_json::from_str(&body).map_err(|error| VoiceStudioClientError::InvalidResponse(error.to_string()))
  }

  async fn get_json(&self, path: &str) -> Result<Value, VoiceStudioClientError> {
    let response = self.authorize(self.client.get(self.url(path))).send().await?;
    let response = self.ensure_success(response).await?;
    Ok(response.json().await?)
  }

  async fn post_json<T: Serialize + ?Sized>(&self, path: &str, payload: &T) -> Result<Value, VoiceStudioClientError> {
    let response = self.authorize(self.client.post(self.url(path))).json(payload).send().await?;
    let response = self.ensure_success(response).await?;
    Ok(response.json().await?)
  }

  fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match self.api_key.as_deref() {
      Some(key) => request.bearer_auth(key),
      None => request,
    }
  }

  async fn ensure_success(&self, response: reqwest::Response) -> Result<reqwest::Response, VoiceStudioClientError> {
    let status = response.status();
    if status.is_success() {
      return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    let status_code = status.as_u16();
    if matches!(status, StatusCode::REQUEST_TIMEOUT | StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT) {
      return Err(VoiceStudioClientError::Unavailable(body));
    }
    Err(VoiceStudioClientError::Http { status: status_code, body })
  }

  fn url(&self, path: &str) -> String {
    format!("{}{}", self.base_url, path)
  }
}

fn normalize_base_url(base_url: String) -> String {
  base_url.trim_end_matches('/').to_string()
}

fn path_segment(value: &str) -> String {
  value.chars().filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')).collect()
}

fn order_batch_results(mut results: Vec<(usize, SpeechResponse)>) -> Vec<SpeechResponse> {
  results.sort_by_key(|(index, _)| *index);
  results.into_iter().map(|(_, response)| response).collect()
}

#[cfg(test)]
mod tests {
  use super::{normalize_base_url, order_batch_results, path_segment};
  use crate::types::{DubRequest, DubSegment, SpeechRequest, SpeechResponse, TranslateRequest, TranslateSegment};

  #[test]
  fn trims_trailing_slashes() {
    assert_eq!(normalize_base_url("http://127.0.0.1:3900///".to_string()), "http://127.0.0.1:3900");
  }

  #[test]
  fn speech_request_batch_preserves_input_order() {
    let results = vec![(2, SpeechResponse { bytes: b"third".to_vec(), content_type: "audio/mpeg".into() }), (0, SpeechResponse { bytes: b"first".to_vec(), content_type: "audio/mpeg".into() }), (1, SpeechResponse { bytes: b"second".to_vec(), content_type: "audio/mpeg".into() })];
    let ordered = order_batch_results(results);
    assert_eq!(ordered.iter().map(|response| response.bytes.as_slice()).collect::<Vec<_>>(), vec![b"first".as_slice(), b"second".as_slice(), b"third".as_slice()]);
  }

  #[test]
  fn path_segments_strip_route_separators() {
    assert_eq!(path_segment("job-01/../../other"), "job-01....other");
  }

  #[test]
  fn speech_request_can_carry_target_duration_without_raw_defaults() {
    let request = SpeechRequest { model: "omnivoice".into(), input: "hello".into(), voice: "profile-1".into(), response_format: "mp3".into(), speed: 1.0, language: Some("vi".into()), instruct: None, duration: Some(2.5) };
    let value = serde_json::to_value(request).expect("speech request should serialize");
    assert_eq!(value.get("duration").and_then(serde_json::Value::as_f64), Some(2.5));
    assert!(value.get("instruct").is_none());
  }

  #[test]
  fn dubbing_contract_preserves_timing_and_voice_match() {
    let request = DubRequest { segments: vec![DubSegment { start: 1.25, end: 3.5, text: "你好".into(), instruct: String::new(), profile_id: "profile-1".into(), speed: None, gain: None, target_lang: Some("vi".into()), direction: None, effect_preset: "broadcast".into() }], language: "Vietnamese".into(), language_code: "vi".into(), instruct: String::new(), num_step: 16, guidance_scale: 2.0, speed: 1.0, segment_ids: Some(vec!["seg-1".into()]), regen_only: None, preview: Some(false), slot_fit: Some("time_stretch".into()), timing_strategy: Some("concise".into()), overflow_budget_s: Some(0.0), fit_options: None, voice_match: Some("consistent".into()) };
    let value = serde_json::to_value(request).expect("dub request should serialize");
    assert_eq!(value["segments"][0]["start"], 1.25);
    assert_eq!(value["segments"][0]["end"], 3.5);
    assert_eq!(value["segments"][0]["profile_id"], "profile-1");
    assert_eq!(value["voice_match"], "consistent");
  }

  #[test]
  fn translation_contract_keeps_source_and_slot_metadata() {
    let request = TranslateRequest { segments: vec![TranslateSegment { id: "seg-1".into(), text: "繁體".into(), target_lang: None, direction: Some("calm".into()), slot_seconds: Some(2.5), start: Some(4.0), end: Some(6.5) }], target_lang: "vi".into(), provider: Some("argos".into()), source_lang: Some("zt".into()), job_id: Some("job-1".into()), quality: Some("fast".into()), glossary: None, dialect: None, auto_glossary: None, reflect: None, condense: Some(false) };
    let value = serde_json::to_value(request).expect("translation request should serialize");
    assert_eq!(value["source_lang"], "zt");
    assert_eq!(value["segments"][0]["slot_seconds"], 2.5);
    assert_eq!(value["segments"][0]["start"], 4.0);
  }
}
