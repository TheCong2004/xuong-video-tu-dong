use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SpeechRequest {
  pub model: String,
  pub input: String,
  pub voice: String,
  pub response_format: String,
  pub speed: f32,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub language: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub instruct: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duration: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct SpeechResponse {
  pub bytes: Vec<u8>,
  pub content_type: String,
}

#[derive(Clone, Debug, Default)]
pub struct TranscriptionRequest {
  pub model: Option<String>,
  pub language: Option<String>,
  pub response_format: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TranscriptionResponse {
  #[serde(default)]
  pub text: String,
  #[serde(default)]
  pub language: Option<String>,
  #[serde(default)]
  pub duration: Option<f64>,
  #[serde(default)]
  pub segments: Vec<Value>,
}

/// A timed source segment accepted by VoiceStudio's dubbing generator.
/// Keeping this contract typed prevents accidental omission of timing or
/// voice-profile fields when ArtCraft hands a transcript to the service.
#[derive(Clone, Debug, Serialize)]
pub struct DubSegment {
  pub start: f64,
  pub end: f64,
  pub text: String,
  #[serde(skip_serializing_if = "String::is_empty")]
  pub instruct: String,
  #[serde(skip_serializing_if = "String::is_empty")]
  pub profile_id: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub speed: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub gain: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub target_lang: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub direction: Option<String>,
  #[serde(skip_serializing_if = "String::is_empty")]
  pub effect_preset: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DubRequest {
  pub segments: Vec<DubSegment>,
  pub language: String,
  pub language_code: String,
  #[serde(skip_serializing_if = "String::is_empty")]
  pub instruct: String,
  pub num_step: u32,
  pub guidance_scale: f32,
  pub speed: f32,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub segment_ids: Option<Vec<String>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub regen_only: Option<Vec<String>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub preview: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub slot_fit: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub timing_strategy: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub overflow_budget_s: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub fit_options: Option<Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub voice_match: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TranslateSegment {
  pub id: String,
  pub text: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub target_lang: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub direction: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub slot_seconds: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub start: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub end: Option<f32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TranslateRequest {
  pub segments: Vec<TranslateSegment>,
  pub target_lang: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub provider: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub source_lang: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub job_id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub quality: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub glossary: Option<Vec<Value>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dialect: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub auto_glossary: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub reflect: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub condense: Option<bool>,
}
