//! Local transcription adapter.
//!
//! The adapter executes an ArtCraft-owned Whisper.cpp binary against an
//! ArtCraft-owned model.  It intentionally has no Python/CapCap/cloud
//! fallback.  A missing resource is a hard, actionable error.

use super::capcut_automation_engine_manager::{CapcutAutomationEngineManager, CapcutEngineKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use voicestudio_client::{TranscriptionRequest as VoiceStudioTranscriptionRequest, VoiceStudioClient};

#[derive(Clone, Debug, Deserialize)]
pub struct LocalTranscriptionRequest {
  pub input_path: String,
  pub language: Option<String>,
  #[serde(default)]
  pub workspace_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TranscriptSegment {
  pub id: String,
  pub start_ms: u64,
  pub end_ms: u64,
  pub text: String,
  pub language: Option<String>,
  pub confidence: Option<f32>,
  pub speaker_id: Option<String>,
  #[serde(default)]
  pub quality_state: String,
  #[serde(default)]
  pub repetition_ratio: f32,
  #[serde(default)]
  pub token_count: usize,
  #[serde(default)]
  pub character_count: usize,
  #[serde(default)]
  pub avg_log_prob: Option<f32>,
  #[serde(default)]
  pub no_speech_probability: Option<f32>,
  #[serde(default)]
  pub compression_ratio: Option<f32>,
  #[serde(default)]
  pub duration_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalTranscriptionResponse {
  pub engine: String,
  pub model_sha256: String,
  pub executable_sha256: String,
  pub segments: Vec<TranscriptSegment>,
}

#[derive(Deserialize)]
struct WhisperJson {
  #[serde(default)]
  transcription: Vec<WhisperSegment>,
  #[serde(default)]
  language: Option<String>,
  /// whisper.cpp emits the detected language under `result.language` in its
  /// JSON output.  Keep the legacy top-level field for older builds, but
  /// prefer the canonical nested result when present so downstream
  /// translation receives an actual ISO language code instead of `auto`.
  #[serde(default)]
  result: Option<WhisperResult>,
}

#[derive(Deserialize)]
struct WhisperResult {
  #[serde(default)]
  language: Option<String>,
}

#[derive(Deserialize)]
struct WhisperSegment {
  timestamps: WhisperTimestamps,
  text: String,
  #[serde(default, alias = "avg_logprob")]
  avg_log_prob: Option<f32>,
  #[serde(default, alias = "no_speech_prob")]
  no_speech_probability: Option<f32>,
  #[serde(default)]
  compression_ratio: Option<f32>,
}

#[derive(Deserialize)]
struct WhisperTimestamps {
  from: String,
  to: String,
}

struct ParsedWhisperOutput {
  language: Option<String>,
  segments: Vec<TranscriptSegment>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WhisperRunDiagnostics {
  workspace_path: String,
  job_id: Option<String>,
  attempt_id: Option<String>,
  input_path: String,
  input_bytes: u64,
  executable_path: String,
  model_path: String,
  arguments: Vec<String>,
  requested_language: Option<String>,
  whisper_language: Option<String>,
  output_prefix: String,
  expected_json_path: String,
  exit_code: Option<i32>,
  stderr_category: Option<String>,
  stdout_bytes: usize,
  stderr_bytes: usize,
  stdout_sha256: String,
  stderr_sha256: String,
  actual_output_files: Vec<String>,
}

pub fn transcribe_local(manager: &CapcutAutomationEngineManager, request: LocalTranscriptionRequest) -> Result<LocalTranscriptionResponse, String> {
  let input = Path::new(&request.input_path);
  if !input.is_file() {
    return Err("CAPCUT_TRANSCRIPTION_INPUT_MISSING".to_string());
  }
  if voicestudio_asr_enabled() {
    return transcribe_with_voicestudio(request);
  }
  let engine = manager.resolve_first(CapcutEngineKind::Transcription)?;
  let executable = engine.executable_path.ok_or_else(|| "CAPCUT_TRANSCRIPTION_EXECUTABLE_MISSING".to_string())?;
  let model_sha256 = sha256_file(&engine.resource_path)?;
  let executable_sha256 = sha256_file(&executable)?;

  // Production jobs provide their attempt-scoped workspace. Direct command
  // callers still receive an isolated temporary workspace that is removed
  // when the request completes.
  let fallback_workspace = if request.workspace_path.is_none() { Some(tempfile::tempdir().map_err(|_| "CAPCUT_TRANSCRIPTION_TEMP_FAILED".to_string())?) } else { None };
  let workspace = request.workspace_path.as_deref().map(PathBuf::from).or_else(|| fallback_workspace.as_ref().map(|dir| dir.path().to_path_buf())).ok_or_else(|| "CAPCUT_TRANSCRIPTION_TEMP_FAILED".to_string())?;
  std::fs::create_dir_all(&workspace).map_err(|_| "CAPCUT_TRANSCRIPTION_TEMP_FAILED".to_string())?;
  let output_prefix = workspace.join("whisper-transcript");
  let json_path = whisper_json_path(&output_prefix);
  let stdout_path = output_prefix.with_extension("stdout.log");
  let stderr_path = output_prefix.with_extension("stderr.log");
  for stale_path in [&json_path, &stdout_path, &stderr_path] {
    if stale_path.is_file() {
      std::fs::remove_file(stale_path).map_err(|_| "CAPCUT_TRANSCRIPTION_TEMP_FAILED".to_string())?;
    }
  }

  let stdout_file = File::create(&stdout_path).map_err(|_| "CAPCUT_TRANSCRIPTION_TEMP_FAILED".to_string())?;
  let stderr_file = File::create(&stderr_path).map_err(|_| "CAPCUT_TRANSCRIPTION_TEMP_FAILED".to_string())?;
  let requested_language = request.language.clone().filter(|value| !value.trim().is_empty());
  let whisper_language = requested_language.as_deref().map(whisper_language_code).map(str::to_string).or_else(|| Some("auto".to_string()));
  // whisper.cpp defaults to English when -l is omitted. That silently turns
  // Chinese speech into romanized English hallucinations, so auto detection
  // must be explicit at the process boundary.
  let mut arguments = whisper_arguments(&engine.resource_path, input, &output_prefix, whisper_language.as_deref());
  let mut command = Command::new(&executable);
  command.args(&arguments);
  command.stdout(Stdio::from(stdout_file)).stderr(Stdio::from(stderr_file));
  let mut child = command.spawn().map_err(|_| "CAPCUT_TRANSCRIPTION_PROCESS_START_FAILED".to_string())?;
  let deadline = Instant::now() + Duration::from_secs(300);
  let status = loop {
    match child.try_wait().map_err(|_| "CAPCUT_TRANSCRIPTION_PROCESS_FAILED".to_string())? {
      Some(status) => break status,
      None if Instant::now() >= deadline => {
        let _ = child.kill();
        let _ = child.wait();
        return Err("CAPCUT_TRANSCRIPTION_TIMEOUT".to_string());
      },
      None => std::thread::sleep(Duration::from_millis(25)),
    }
  };
  let stdout = std::fs::read(&stdout_path).unwrap_or_default();
  let stderr = std::fs::read(&stderr_path).unwrap_or_default();
  let stderr_text = String::from_utf8_lossy(&stderr);
  let actual_output_files = matching_output_files(&workspace, &output_prefix);
  let attempt_id = workspace.file_name().and_then(|name| name.to_str()).map(str::to_string);
  let job_id = workspace.parent().and_then(Path::file_name).and_then(|name| name.to_str()).map(str::to_string);
  let diagnostics = WhisperRunDiagnostics { workspace_path: workspace.to_string_lossy().to_string(), job_id, attempt_id, input_path: input.to_string_lossy().to_string(), input_bytes: std::fs::metadata(input).map(|metadata| metadata.len()).unwrap_or(0), executable_path: executable.to_string_lossy().to_string(), model_path: engine.resource_path.to_string_lossy().to_string(), arguments, requested_language: requested_language.clone(), whisper_language: whisper_language.clone(), output_prefix: output_prefix.to_string_lossy().to_string(), expected_json_path: json_path.to_string_lossy().to_string(), exit_code: status.code(), stderr_category: stderr_category(&stderr_text).map(str::to_string), stdout_bytes: stdout.len(), stderr_bytes: stderr.len(), stdout_sha256: sha256_bytes(&stdout), stderr_sha256: sha256_bytes(&stderr), actual_output_files };
  let _ = std::fs::write(workspace.join("whisper-diagnostics.json"), serde_json::to_vec_pretty(&diagnostics).unwrap_or_default());
  let _ = std::fs::remove_file(&stdout_path);
  let _ = std::fs::remove_file(&stderr_path);
  classify_whisper_process_result(status.success(), &stderr_text)?;
  if !json_path.is_file() {
    return Err("CAPCUT_TRANSCRIPTION_OUTPUT_ARGUMENT_MISMATCH".to_string());
  }
  let json = std::fs::read_to_string(&json_path).map_err(|_| "CAPCUT_TRANSCRIPTION_OUTPUT_READ_FAILED".to_string())?;
  let parsed = parse_whisper_json(&json, requested_language)?;
  // Keep the attempt-scoped transcript artifact so a failed downstream stage
  // can be reproduced offline without rerunning the video/Whisper job.  Raw
  // transcript text is never emitted in diagnostics or logs.
  Ok(LocalTranscriptionResponse { engine: engine.spec.id, model_sha256, executable_sha256, segments: parsed.segments })
}

fn voicestudio_asr_enabled() -> bool {
  std::env::var("FLOWORD_ASR_PROVIDER").ok().is_some_and(|value| value.trim().eq_ignore_ascii_case("voicestudio"))
}

/// Execute VoiceStudio ASR from the synchronous native-command boundary.
/// The CapCut automation commands predate the async VoiceStudio client, so the
/// request is isolated on a small current-thread runtime instead of blocking
/// ArtCraft's main async executor. This path is opt-in via
/// `FLOWORD_ASR_PROVIDER=voicestudio`; the verified local Whisper path remains
/// the default.
fn transcribe_with_voicestudio(request: LocalTranscriptionRequest) -> Result<LocalTranscriptionResponse, String> {
  let input = PathBuf::from(&request.input_path);
  let workspace = request.workspace_path.as_deref().map(PathBuf::from);
  let requested_language = request.language.clone().filter(|value| !value.trim().is_empty() && !value.eq_ignore_ascii_case("auto"));
  let model = std::env::var("VOICESTUDIO_ASR_MODEL").ok().filter(|value| !value.trim().is_empty()).unwrap_or_else(|| "whisper-1".to_string());
  let input_bytes = std::fs::metadata(&input).map(|metadata| metadata.len()).unwrap_or(0);
  let input_for_request = input.clone();
  let language_for_request = requested_language.clone();
  let model_for_request = model.clone();
  let response = run_voice_studio_future(async move {
    let client = VoiceStudioClient::from_env().map_err(|error| error.to_string())?;
    client.transcribe(&input_for_request, &VoiceStudioTranscriptionRequest { model: Some(model_for_request), language: language_for_request, response_format: "verbose_json".to_string() }).await.map_err(|error| error.to_string())
  })?;
  let detected_language = response.language.clone().or(requested_language.clone());
  let mut segments = response.segments.iter().enumerate().filter_map(|(index, value)| parse_voicestudio_segment(value, index, detected_language.as_deref())).collect::<Vec<_>>();
  // Some VoiceStudio backends return a valid `text` field while omitting
  // segment timestamps (for example when word timestamps are unavailable).
  // Preserve that transcript as one timed segment instead of misclassifying a
  // successful ASR response as output-missing.
  if segments.is_empty() && !response.text.trim().is_empty() {
    let end_ms = response.duration.filter(|value| value.is_finite() && *value > 0.0).map(|value| (value * 1_000.0).round() as u64).unwrap_or(1).max(1);
    if let Some(segment) = parse_voicestudio_segment(&serde_json::json!({ "id": "segment-1", "text": response.text, "start": 0.0, "end": end_ms as f64 / 1_000.0 }), 0, detected_language.as_deref()) {
      segments.push(segment);
    }
  }
  if let Some(workspace) = workspace.as_deref() {
    let diagnostics = serde_json::json!({
      "provider": "voicestudio",
      "model": model,
      "inputBytes": input_bytes,
      "requestedLanguage": requested_language,
      "detectedLanguage": detected_language,
      "segmentCount": segments.len(),
      "durationSeconds": response.duration,
    });
    let _ = std::fs::create_dir_all(workspace);
    let _ = std::fs::write(workspace.join("voicestudio-transcription-diagnostics.json"), serde_json::to_vec_pretty(&diagnostics).unwrap_or_default());
  }
  Ok(LocalTranscriptionResponse { engine: "voicestudio-asr".to_string(), model_sha256: "external:voicestudio".to_string(), executable_sha256: "external:voicestudio".to_string(), segments })
}

fn parse_voicestudio_segment(value: &serde_json::Value, index: usize, language: Option<&str>) -> Option<TranscriptSegment> {
  let text = value.get("text").and_then(serde_json::Value::as_str).unwrap_or_default().trim().to_string();
  if text.is_empty() {
    return None;
  }
  let start_ms = value.get("start").and_then(value_to_millis).or_else(|| value.get("start_ms").and_then(serde_json::Value::as_u64)).unwrap_or(0);
  let end_ms = value.get("end").and_then(value_to_millis).or_else(|| value.get("end_ms").and_then(serde_json::Value::as_u64)).unwrap_or(start_ms);
  let duration_ms = end_ms.saturating_sub(start_ms);
  let quality = assess_transcript_quality(&text, duration_ms);
  let quality_state = quality_state_with_language_and_whisper_metrics(language, quality.state, None, None, None, &text, duration_ms);
  Some(TranscriptSegment { id: value.get("id").and_then(serde_json::Value::as_str).map(str::to_string).unwrap_or_else(|| format!("segment-{}", index + 1)), start_ms, end_ms, text: text.clone(), language: language.map(str::to_string), confidence: value.get("confidence").and_then(serde_json::Value::as_f64).map(|value| value as f32), speaker_id: None, quality_state, repetition_ratio: quality.repetition_ratio, token_count: quality.token_count, character_count: quality.character_count, avg_log_prob: None, no_speech_probability: None, compression_ratio: None, duration_ms })
}

fn value_to_millis(value: &serde_json::Value) -> Option<u64> {
  let seconds = value.as_f64()?;
  if !seconds.is_finite() || seconds < 0.0 {
    return None;
  }
  Some((seconds * 1_000.0).round() as u64)
}

fn run_voice_studio_future<F, T>(future: F) -> Result<T, String>
where
  F: std::future::Future<Output = Result<T, String>> + Send + 'static,
  T: Send + 'static,
{
  let join = std::thread::spawn(move || {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|error| format!("CAPCUT_VOICESTUDIO_RUNTIME_FAILED:{error}"))?;
    runtime.block_on(future)
  });
  join.join().map_err(|_| "CAPCUT_VOICESTUDIO_REQUEST_PANICKED".to_string())?
}

fn whisper_language_code(language: &str) -> &str {
  match language.trim().to_ascii_lowercase().as_str() {
    "zt" | "zh-tw" | "zh-hant" | "chi_tra" | "zh-cn" | "zh-hans" | "chi_sim" => "zh",
    _ => language,
  }
}

fn whisper_json_path(output_prefix: &Path) -> PathBuf {
  output_prefix.with_extension("json")
}

fn classify_whisper_process_result(success: bool, stderr: &str) -> Result<(), String> {
  let category = stderr_category(stderr);
  if !success {
    return Err(format!("CAPCUT_TRANSCRIPTION_PROCESS_FAILED:{}", category.unwrap_or("NON_ZERO_EXIT")));
  }
  if let Some(category) = category {
    return Err(format!("CAPCUT_TRANSCRIPTION_PROCESS_FAILED:WHISPER_REPORTED_ERROR:{category}"));
  }
  Ok(())
}

fn stderr_category(stderr: &str) -> Option<&'static str> {
  let normalized = stderr.to_ascii_lowercase();
  if normalized.contains("unknown language") {
    Some("UNKNOWN_LANGUAGE")
  } else if normalized.lines().any(|line| line.trim_start().starts_with("error:")) {
    Some("WHISPER_ERROR")
  } else {
    None
  }
}

fn matching_output_files(workspace: &Path, output_prefix: &Path) -> Vec<String> {
  let prefix = output_prefix.file_name().and_then(|name| name.to_str()).unwrap_or_default();
  let mut files = std::fs::read_dir(workspace)
    .ok()
    .into_iter()
    .flatten()
    .filter_map(Result::ok)
    .filter_map(|entry| {
      let name = entry.file_name().to_string_lossy().to_string();
      name.starts_with(prefix).then_some(name)
    })
    .collect::<Vec<_>>();
  files.sort();
  files
}

fn parse_whisper_json(json: &str, requested_language: Option<String>) -> Result<ParsedWhisperOutput, String> {
  let parsed: WhisperJson = serde_json::from_str(json).map_err(|_| "CAPCUT_TRANSCRIPTION_OUTPUT_INVALID".to_string())?;
  let language = requested_language.filter(|value| !value.trim().is_empty() && value != "auto").or_else(|| parsed.result.as_ref().and_then(|result| result.language.clone())).or(parsed.language.clone());
  let segments = parsed
    .transcription
    .into_iter()
    .enumerate()
    .map(|(index, segment)| {
      let start_ms = parse_timestamp_ms(&segment.timestamps.from);
      let end_ms = parse_timestamp_ms(&segment.timestamps.to);
      // whisper.cpp on some Windows builds writes CJK JSON through the active
      // Windows-1252 code page.  That produces mojibake such as
      // `æˆ‘å€‘` instead of `我々` even though the JSON itself is valid UTF-8.
      // Repair only when the reversible CP1252 -> UTF-8 conversion yields
      // Han/Vietnamese text; genuine UTF-8 is left untouched.
      let text = repair_mojibake(segment.text.trim());
      let quality = assess_transcript_quality(&text, end_ms.saturating_sub(start_ms));
      let duration_ms = end_ms.saturating_sub(start_ms);
      let quality_state = quality_state_with_language_and_whisper_metrics(language.as_deref(), quality.state, segment.avg_log_prob, segment.no_speech_probability, segment.compression_ratio, &text, duration_ms);
      TranscriptSegment { id: format!("segment-{}", index + 1), start_ms, end_ms, text, language: language.clone(), confidence: segment.avg_log_prob.map(|value| value.clamp(-5.0, 0.0)), speaker_id: None, quality_state, repetition_ratio: quality.repetition_ratio, token_count: quality.token_count, character_count: quality.character_count, avg_log_prob: segment.avg_log_prob, no_speech_probability: segment.no_speech_probability, compression_ratio: segment.compression_ratio, duration_ms }
    })
    .filter(|segment| !segment.text.is_empty() && segment.end_ms >= segment.start_ms)
    .collect();
  Ok(ParsedWhisperOutput { language, segments })
}

fn quality_state_with_whisper_metrics(base: &str, avg_log_prob: Option<f32>, no_speech_probability: Option<f32>, compression_ratio: Option<f32>, text: &str, duration_ms: u64) -> String {
  if base == "HALLUCINATION_SUSPECTED" {
    return base.to_string();
  }
  let token_count = text.split_whitespace().count();
  let no_speech_hallucination = no_speech_probability.is_some_and(|value| value >= 0.80) && token_count >= 4;
  let compression_hallucination = compression_ratio.is_some_and(|value| value >= 2.8) && token_count >= 4;
  let too_low_confidence = avg_log_prob.is_some_and(|value| value < -1.5) && duration_ms >= 1_000;
  if no_speech_hallucination || compression_hallucination {
    "HALLUCINATION_SUSPECTED".to_string()
  } else if too_low_confidence {
    "LOW_CONFIDENCE".to_string()
  } else {
    base.to_string()
  }
}

/// A Chinese language detection result must not be paired with an entirely
/// Latin, hyphenated pseudo-transcript. Whisper can emit romanised syllables
/// when it is invoked with an English/default language contract; accepting
/// that text would poison translation and subtitles. This is a conservative
/// consistency check: it only rejects Chinese-labelled segments with enough
/// text and no Han characters at all.
fn quality_state_with_language_and_whisper_metrics(language: Option<&str>, base: &str, avg_log_prob: Option<f32>, no_speech_probability: Option<f32>, compression_ratio: Option<f32>, text: &str, duration_ms: u64) -> String {
  let normalized_language = language.map(|value| value.trim().to_ascii_lowercase());
  let is_chinese = normalized_language.as_deref().is_some_and(|value| matches!(value, "zh" | "zt" | "zh-cn" | "zh-tw" | "zh-hans" | "zh-hant" | "chi_sim" | "chi_tra"));
  let alphanumeric_count = text.chars().filter(|character| character.is_alphanumeric()).count();
  let has_han = text.chars().any(|character| ('\u{3400}'..='\u{9fff}').contains(&character));
  let hyphenated_tokens = text.split_whitespace().filter(|token| token.matches('-').count() >= 2).count();
  // Chinese speech may legitimately contain short English words, names, or
  // phrases. Only reject the characteristic multi-hyphen romanisation that
  // indicates Whisper was run with the wrong language contract; rejecting all
  // ASCII text here incorrectly fails mixed Chinese/English transcripts.
  if is_chinese && alphanumeric_count >= 4 && !has_han && hyphenated_tokens > 0 {
    return "LANGUAGE_UNRESOLVED".to_string();
  }
  quality_state_with_whisper_metrics(base, avg_log_prob, no_speech_probability, compression_ratio, text, duration_ms)
}

fn repair_mojibake(value: &str) -> String {
  let suspicious = value.chars().any(|character| matches!(character, 'Ã' | 'Â' | 'â' | 'ð' | 'æ' | 'å' | 'ç' | 'é' | 'ê' | 'ë' | 'ï' | 'œ' | '�'));
  if !suspicious {
    return value.to_string();
  }
  let mut bytes = Vec::with_capacity(value.len());
  for character in value.chars() {
    let byte = match character as u32 {
      0x00..=0x7f | 0xa0..=0xff => character as u8,
      // Some producers decode UTF-8 as ISO-8859-1 instead of CP1252,
      // leaving C1 bytes as control characters. Preserve those bytes too;
      // the UTF-8 validation below decides whether a repair is real.
      0x80..=0x9f => character as u8,
      0x20ac => 0x80,
      0x201a => 0x82,
      0x0192 => 0x83,
      0x201e => 0x84,
      0x2026 => 0x85,
      0x2020 => 0x86,
      0x2021 => 0x87,
      0x02c6 => 0x88,
      0x2030 => 0x89,
      0x0160 => 0x8a,
      0x2039 => 0x8b,
      0x0152 => 0x8c,
      0x017d => 0x8e,
      0x2018 => 0x91,
      0x2019 => 0x92,
      0x201c => 0x93,
      0x201d => 0x94,
      0x2022 => 0x95,
      0x2013 => 0x96,
      0x2014 => 0x97,
      0x02dc => 0x98,
      0x2122 => 0x99,
      0x0161 => 0x9a,
      0x203a => 0x9b,
      0x0153 => 0x9c,
      0x017e => 0x9e,
      0x0178 => 0x9f,
      _ => return value.to_string(),
    };
    bytes.push(byte);
  }
  let Ok(candidate) = String::from_utf8(bytes) else {
    return value.to_string();
  };
  let candidate_is_target_script = candidate.chars().any(|character| ('\u{3400}'..='\u{9fff}').contains(&character) || matches!(character, 'ă' | 'Ă' | 'â' | 'Â' | 'đ' | 'Đ' | 'ê' | 'Ê' | 'ô' | 'Ô' | 'ơ' | 'Ơ' | 'ư' | 'Ư'));
  if candidate_is_target_script {
    candidate
  } else {
    value.to_string()
  }
}

fn whisper_arguments(model: &Path, input: &Path, output_prefix: &Path, language: Option<&str>) -> Vec<String> {
  let language = language.filter(|value| !value.trim().is_empty()).unwrap_or("auto");
  vec!["-m".to_string(), model.to_string_lossy().to_string(), "-f".to_string(), input.to_string_lossy().to_string(), "-oj".to_string(), "-of".to_string(), output_prefix.to_string_lossy().to_string(), "-l".to_string(), whisper_language_code(language).to_string()]
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TranscriptQuality {
  state: &'static str,
  repetition_ratio: f32,
  token_count: usize,
  character_count: usize,
}

/// Detect the repeated hyphenated output produced when multilingual Whisper is
/// accidentally run with its English default. This is deliberately a gate,
/// not a language detector: questionable segments never reach translation.
fn assess_transcript_quality(text: &str, duration_ms: u64) -> TranscriptQuality {
  let tokens = text.split_whitespace().map(|token| token.trim_matches(|character: char| !character.is_alphanumeric() && character != '-').to_ascii_lowercase()).filter(|token| !token.is_empty()).collect::<Vec<_>>();
  let mut counts = std::collections::HashMap::<&str, usize>::new();
  for token in &tokens {
    *counts.entry(token.as_str()).or_default() += 1;
  }
  let repeated_tokens = counts.values().map(|count| count.saturating_sub(1)).sum::<usize>();
  let repetition_ratio = if tokens.is_empty() { 0.0 } else { repeated_tokens as f32 / tokens.len() as f32 };
  let hyphenated = tokens.iter().filter(|token| token.matches('-').count() >= 2).count();
  let hyphen_ratio = if tokens.is_empty() { 0.0 } else { hyphenated as f32 / tokens.len() as f32 };
  let duration_seconds = duration_ms as f32 / 1_000.0;
  let too_many_characters = duration_seconds >= 1.0 && text.chars().count() as f32 > duration_seconds * 42.0;
  // A short repeated hyphenated sequence is already enough to indicate the
  // multilingual-Whisper romanisation failure (the common fixture has four
  // repeated tokens). Keep the repetition-only checks conservative so normal
  // speech with a repeated word is not rejected.
  let repeated_hyphenated = tokens.len() >= 4 && hyphen_ratio >= 0.7;
  let hallucination = repeated_hyphenated || (tokens.len() >= 8 && ((repetition_ratio >= 0.45 && hyphen_ratio >= 0.25) || too_many_characters && repetition_ratio >= 0.35));
  TranscriptQuality { state: if hallucination { "HALLUCINATION_SUSPECTED" } else { "ACCEPTED" }, repetition_ratio, token_count: tokens.len(), character_count: text.chars().count() }
}

pub fn validate_transcript_quality(segments: &[TranscriptSegment]) -> Result<(), String> {
  if segments.is_empty() {
    return Err("CAPCUT_TRANSCRIPTION_EMPTY".to_string());
  }
  let suspicious = segments.iter().filter(|segment| matches!(segment.quality_state.as_str(), "HALLUCINATION_SUSPECTED" | "LOW_CONFIDENCE" | "LANGUAGE_UNRESOLVED")).count();
  // Whisper may omit the language field in JSON output. In that case a
  // romanised pseudo-transcript can otherwise evade the per-segment Chinese
  // script check (the failure observed in production was one hyphenated token
  // per cue). Detect the characteristic pattern across the transcript as a
  // whole, while requiring multiple independent cues to avoid rejecting a
  // legitimate single proper name.
  let romanized_cues = segments.iter().filter(|segment| looks_like_romanized_pseudotranscript(&segment.text)).count();
  let romanized_majority = romanized_cues >= 2 && romanized_cues * 2 >= segments.len();
  // Never pass even a single confidently suspicious cue downstream. A single
  // hallucinated segment is enough to create a repeated/garbled subtitle and
  // can also poison the translation context for the following cues.
  if suspicious > 0 || romanized_majority {
    return Err("CAPCUT_TRANSCRIPTION_QUALITY_UNRESOLVED".to_string());
  }
  Ok(())
}

fn looks_like_romanized_pseudotranscript(text: &str) -> bool {
  let trimmed = text.trim();
  if trimmed.is_empty() || trimmed.chars().any(|character| ('\u{3400}'..='\u{9fff}').contains(&character)) {
    return false;
  }
  let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
  // The bad Whisper mode emits ASCII syllable chains such as
  // "Seqi-Wang-Lio-Defu-Gai". Require at least two hyphens and enough letters;
  // ordinary hyphenated prose (or a single proper name) is not sufficient.
  let hyphenated = tokens.iter().filter(|token| token.matches('-').count() >= 2 && token.chars().filter(|character| character.is_ascii_alphabetic()).count() >= 6).count();
  hyphenated > 0 && trimmed.chars().all(|character| character.is_ascii() || character.is_whitespace() || "-'.,!?\"()".contains(character))
}

fn sha256_file(path: &Path) -> Result<String, String> {
  let bytes = std::fs::read(path).map_err(|_| "CAPCUT_TRANSCRIPTION_RESOURCE_READ_FAILED".to_string())?;
  let digest = Sha256::digest(bytes);
  Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn sha256_bytes(bytes: &[u8]) -> String {
  let digest = Sha256::digest(bytes);
  digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_timestamp_ms(value: &str) -> u64 {
  let mut parts = value.split(':');
  let hours = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
  let minutes = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
  let seconds = parts.next().unwrap_or("0");
  // whisper.cpp emits both ISO-style `00:00:01.250` and SRT-style
  // `00:00:01,250` timestamps depending on the output build.  Treat the
  // comma as an equivalent decimal separator so cue timing remains usable
  // for diarization/speaker correlation.
  let (whole, fraction) = seconds.split_once('.').or_else(|| seconds.split_once(',')).unwrap_or((seconds, "0"));
  let milliseconds = format!("{fraction:0<3}").chars().take(3).collect::<String>().parse::<u64>().unwrap_or(0);
  (hours * 3_600 + minutes * 60 + whole.parse::<u64>().unwrap_or(0)) * 1_000 + milliseconds
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_whisper_timestamps_without_network_or_capcap() {
    assert_eq!(parse_timestamp_ms("00:01:02.5"), 62_500);
    assert_eq!(parse_timestamp_ms("00:00:00.125"), 125);
    assert_eq!(parse_timestamp_ms("00:00:04,000"), 4_000);
  }

  #[test]
  fn maps_product_chinese_codes_to_whisper_language_contract() {
    assert_eq!(whisper_language_code("zt"), "zh");
    assert_eq!(whisper_language_code("zh"), "zh");
    assert_eq!(whisper_language_code("en"), "en");
  }

  #[test]
  fn resolves_json_output_from_the_exact_output_prefix() {
    let prefix = Path::new(r"C:\attempt\whisper-transcript");
    assert_eq!(whisper_json_path(prefix), Path::new(r"C:\attempt\whisper-transcript.json"));
  }

  #[test]
  fn empty_speech_json_is_valid_and_not_reported_as_missing_output() {
    let response = parse_whisper_json(r#"{"result":{"language":"zh"},"transcription":[]}"#, Some("zt".into())).unwrap();
    assert_eq!(response.language.as_deref(), Some("zt"));
    assert!(response.segments.is_empty());
  }

  #[test]
  fn successful_exit_with_whisper_error_is_a_process_failure() {
    assert_eq!(classify_whisper_process_result(true, "whisper_lang_id: unknown language 'zt'\nerror: unknown language 'zt'").unwrap_err(), "CAPCUT_TRANSCRIPTION_PROCESS_FAILED:WHISPER_REPORTED_ERROR:UNKNOWN_LANGUAGE");
  }

  #[test]
  fn auto_language_is_explicit_at_whisper_boundary() {
    let arguments = whisper_arguments(Path::new("model.bin"), Path::new("audio.wav"), Path::new("out"), None);
    assert_eq!(arguments.last().map(String::as_str), Some("auto"));
    assert_eq!(arguments[arguments.len() - 2], "-l");
  }

  #[test]
  fn repeated_hyphenated_transcript_is_rejected_as_hallucination() {
    let quality = assess_transcript_quality("Segi-Wang-Luo-Defu-Gai Segi-Wang-Luo-Defu-Gai Segi-Wang-Luo-Defu-Gai Segi-Wang-Luo-Defu-Gai", 4_000);
    assert_eq!(quality.state, "HALLUCINATION_SUSPECTED");
  }

  #[test]
  fn ordinary_sentence_passes_transcript_quality_gate() {
    let quality = assess_transcript_quality("This is a short sentence with useful context.", 4_000);
    assert_eq!(quality.state, "ACCEPTED");
  }

  #[test]
  fn parses_voicestudio_verbose_segment_with_second_timestamps() {
    let value = serde_json::json!({"id": "seg-7", "start": 1.25, "end": 3.5, "text": "繁體字幕", "confidence": 0.91});
    let segment = parse_voicestudio_segment(&value, 6, Some("zt")).expect("segment should parse");
    assert_eq!(segment.id, "seg-7");
    assert_eq!((segment.start_ms, segment.end_ms), (1_250, 3_500));
    assert_eq!(segment.language.as_deref(), Some("zt"));
    assert_eq!(segment.text, "繁體字幕");
  }

  #[test]
  fn whisper_metrics_reject_speech_probability_hallucination() {
    let state = quality_state_with_whisper_metrics("ACCEPTED", Some(-0.2), Some(0.95), Some(1.1), "one two three four", 4_000);
    assert_eq!(state, "HALLUCINATION_SUSPECTED");
  }

  #[test]
  fn whisper_metrics_mark_low_confidence_without_rewriting_text() {
    let state = quality_state_with_whisper_metrics("ACCEPTED", Some(-2.0), Some(0.05), Some(1.1), "a normal sentence", 4_000);
    assert_eq!(state, "LOW_CONFIDENCE");
  }

  #[test]
  fn chinese_language_with_romanized_hyphenated_text_is_unresolved() {
    let state = quality_state_with_language_and_whisper_metrics(Some("zh"), "ACCEPTED", Some(-0.2), Some(0.05), Some(1.1), "Segi-Wang-Lio-Defu-Gai", 2_000);
    assert_eq!(state, "LANGUAGE_UNRESOLVED");
  }

  #[test]
  fn chinese_language_with_han_text_is_not_rejected_by_script_check() {
    let state = quality_state_with_language_and_whisper_metrics(Some("zh"), "ACCEPTED", Some(-0.2), Some(0.05), Some(1.1), "今天天氣很好", 2_000);
    assert_eq!(state, "ACCEPTED");
  }

  #[test]
  fn chinese_transcript_may_contain_english_phrase() {
    let state = quality_state_with_language_and_whisper_metrics(Some("zh"), "ACCEPTED", None, None, None, "Thank you for being here", 2_000);
    assert_eq!(state, "ACCEPTED");
  }

  #[test]
  fn repairs_windows_codepage_mojibake_without_touching_real_utf8() {
    assert_eq!(repair_mojibake("æˆ‘å€‘ç¾åœ¨"), "我們現在");
    assert_eq!(repair_mojibake("Chúng ta đang ở đây"), "Chúng ta đang ở đây");
  }

  #[test]
  fn missing_language_metadata_rejects_repeated_romanized_cues() {
    let segments = vec![TranscriptSegment { id: "segment-1".into(), start_ms: 0, end_ms: 2_000, text: "Seqi-Wang-Lio-Defu-Gai".into(), language: None, confidence: None, speaker_id: None, quality_state: "ACCEPTED".into(), repetition_ratio: 0.0, token_count: 1, character_count: 24, avg_log_prob: None, no_speech_probability: None, compression_ratio: None, duration_ms: 2_000 }, TranscriptSegment { id: "segment-2".into(), start_ms: 2_000, end_ms: 4_000, text: "Sang-Gang-Gu-Gu-Gang".into(), language: None, confidence: None, speaker_id: None, quality_state: "ACCEPTED".into(), repetition_ratio: 0.0, token_count: 1, character_count: 20, avg_log_prob: None, no_speech_probability: None, compression_ratio: None, duration_ms: 2_000 }];
    assert_eq!(validate_transcript_quality(&segments).unwrap_err(), "CAPCUT_TRANSCRIPTION_QUALITY_UNRESOLVED");
  }

  #[test]
  fn one_hyphenated_proper_name_is_not_a_transcript_failure() {
    assert!(!looks_like_romanized_pseudotranscript("Jean-Pierre"));
    let segment = TranscriptSegment { id: "segment-1".into(), start_ms: 0, end_ms: 1_000, text: "Jean-Pierre".into(), language: None, confidence: None, speaker_id: None, quality_state: "ACCEPTED".into(), repetition_ratio: 0.0, token_count: 1, character_count: 11, avg_log_prob: None, no_speech_probability: None, compression_ratio: None, duration_ms: 1_000 };
    assert!(validate_transcript_quality(&[segment]).is_ok());
  }
}
