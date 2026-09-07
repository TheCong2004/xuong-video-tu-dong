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
  let whisper_language = requested_language.as_deref().map(whisper_language_code).map(str::to_string);
  let mut arguments = vec!["-m".to_string(), engine.resource_path.to_string_lossy().to_string(), "-f".to_string(), input.to_string_lossy().to_string(), "-oj".to_string(), "-of".to_string(), output_prefix.to_string_lossy().to_string()];
  if let Some(language) = whisper_language.as_deref() {
    arguments.extend(["-l".to_string(), language.to_string()]);
  }
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
  let segments = parsed.transcription.into_iter().enumerate().map(|(index, segment)| TranscriptSegment { id: format!("segment-{}", index + 1), start_ms: parse_timestamp_ms(&segment.timestamps.from), end_ms: parse_timestamp_ms(&segment.timestamps.to), text: segment.text.trim().to_string(), language: language.clone(), confidence: None, speaker_id: None }).filter(|segment| !segment.text.is_empty() && segment.end_ms >= segment.start_ms).collect();
  Ok(ParsedWhisperOutput { language, segments })
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
}
