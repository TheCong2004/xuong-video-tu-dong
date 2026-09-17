//! Local Argos/OPUS-MT translation adapter.
//!
//! The worker is ArtCraft-owned and communicates over stdin/stdout.  A missing
//! or unverified worker is a hard error; there is no cloud or CapCap fallback.

use super::capcut_automation_engine_manager::{CapcutAutomationEngineManager, CapcutEngineKind};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TranslationRouteKind {
  Direct,
  Pivot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationHop {
  pub source_language: String,
  pub target_language: String,
  pub model_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationRoute {
  pub requested_source_language: String,
  pub effective_source_language: String,
  pub route_kind: TranslationRouteKind,
  pub hops: Vec<TranslationHop>,
}

/// Sanitized diagnostics emitted when the offline worker exits unsuccessfully.
/// These fields deliberately describe the boundary and never contain transcript
/// text, tokens, or model input.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationFailureDiagnostics {
  pub error_code: String,
  pub hop_index: usize,
  pub operation_stage: String,
  pub exit_code: i32,
  pub model_id: String,
  pub segment_count: usize,
  pub outer_type: Option<String>,
  pub first_item_type: Option<String>,
  pub first_token_type: Option<String>,
  pub empty_token_sequence_count: Option<usize>,
  pub max_token_count: Option<usize>,
  pub exception_type: Option<String>,
  pub sanitized_message: String,
}

impl TranslationFailureDiagnostics {
  pub fn from_worker(stderr: &str, hop_index: usize, model_id: &str, segment_count: usize, exit_code: i32, fallback_message: &str) -> Self {
    let latest = stderr.lines().filter_map(|line| line.strip_prefix("CAPCUT_TRANSLATION_DIAGNOSTICS ")).filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok()).last();
    let get_string = |name: &str| latest.as_ref().and_then(|value| value.get(name)).and_then(serde_json::Value::as_str).map(str::to_owned);
    let get_usize = |name: &str| latest.as_ref().and_then(|value| value.get(name)).and_then(serde_json::Value::as_u64).map(|value| value as usize);
    let operation_stage = get_string("operationStage").unwrap_or_else(|| "PROCESS_EXIT".to_string());
    let message = get_string("sanitizedMessage").unwrap_or_else(|| fallback_message.to_string());
    Self { error_code: "CAPCUT_TRANSLATION_FAILED".to_string(), hop_index: latest.as_ref().and_then(|value| value.get("hopIndex")).and_then(serde_json::Value::as_u64).map(|value| value as usize).unwrap_or(hop_index), operation_stage, exit_code, model_id: get_string("modelId").unwrap_or_else(|| model_id.to_string()), segment_count, outer_type: get_string("outerType"), first_item_type: get_string("firstItemType"), first_token_type: get_string("firstTokenType"), empty_token_sequence_count: get_usize("emptyTokenSequenceCount"), max_token_count: get_usize("maxTokenCount"), exception_type: get_string("exceptionType"), sanitized_message: message.chars().filter(|character| !character.is_control()).take(240).collect() }
  }
}

/// Resolve an offline translation route without guessing a Chinese→Vietnamese
/// model. Chinese routes deliberately use a verified English pivot.
pub fn plan_translation_route(source_language: &str, target_language: &str) -> Result<TranslationRoute, String> {
  let requested_source_language = source_language.trim().to_ascii_lowercase();
  let target = target_language.trim().to_ascii_lowercase();
  let effective_source_language = match requested_source_language.as_str() {
    "auto" | "" => return Err("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:auto_source_requires_detection".into()),
    "zh" | "zh-cn" | "zh-hans" | "chi_sim" => "zh".to_string(),
    "zt" | "zh-tw" | "zh-hant" | "chi_tra" => "zt".to_string(),
    "en" | "eng" => "en".to_string(),
    _ => return Err(format!("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:unsupported_source:{requested_source_language}")),
  };
  if target != "vi" {
    return Err(format!("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:unsupported_target:{target}"));
  }
  let (route_kind, hops) = match effective_source_language.as_str() {
    "en" => (TranslationRouteKind::Direct, vec![TranslationHop { source_language: "en".into(), target_language: "vi".into(), model_id: "argos-en-vi-1-9".into() }]),
    "zh" => (TranslationRouteKind::Pivot, vec![TranslationHop { source_language: "zh".into(), target_language: "en".into(), model_id: "argos-zh-en-1-9".into() }, TranslationHop { source_language: "en".into(), target_language: "vi".into(), model_id: "argos-en-vi-1-9".into() }]),
    "zt" => (TranslationRouteKind::Pivot, vec![TranslationHop { source_language: "zt".into(), target_language: "en".into(), model_id: "argos-zt-en-1-9".into() }, TranslationHop { source_language: "en".into(), target_language: "vi".into(), model_id: "argos-en-vi-1-9".into() }]),
    _ => unreachable!(),
  };
  Ok(TranslationRoute { requested_source_language, effective_source_language, route_kind, hops })
}

#[derive(Clone, Debug, Deserialize)]
pub struct LocalTranslationRequest {
  pub segments: Vec<TranslationSegment>,
  pub source_language: String,
  pub target_language: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TranslationSegment {
  pub id: String,
  pub start_ms: u64,
  pub end_ms: u64,
  pub text: String,
  pub translated_text: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalTranslationResponse {
  pub provider: String,
  pub model: String,
  pub segments: Vec<TranslationSegment>,
}

#[derive(Serialize)]
struct WorkerRequest<'a> {
  segments: Vec<WorkerSegment<'a>>,
}

#[derive(Serialize)]
struct WorkerSegment<'a> {
  id: &'a str,
  text: &'a str,
}

#[derive(Deserialize)]
struct WorkerResponse {
  provider: String,
  model: String,
  segments: Vec<WorkerSegmentResult>,
}

#[derive(Deserialize)]
struct WorkerSegmentResult {
  id: String,
  #[serde(rename = "translatedText")]
  translated_text: String,
}

fn invoke_translation_worker(manager: &CapcutAutomationEngineManager, executable: &std::path::Path, worker_script: &std::path::Path, model_path: &std::path::Path, route: &TranslationRoute, hop_index: usize, segments: &[TranslationSegment]) -> Result<(String, Vec<TranslationSegment>), String> {
  let payload = WorkerRequest { segments: segments.iter().map(|segment| WorkerSegment { id: &segment.id, text: &segment.text }).collect() };
  let mut child = crate::core::lifecycle::startup::tasks::background_command::background_command(Command::new(executable));
  let packaged_python_path = executable.parent().and_then(|parent| parent.parent()).and_then(|runtime| runtime.parent()).map(|root| root.join("python")).filter(|path| path.is_dir());
  let python_path = packaged_python_path.unwrap_or_else(|| manager.install_root().join("python"));
  let worker_sha = route
    .hops
    .get(hop_index)
    .and_then(|_| std::fs::read(worker_script).ok())
    .map(|bytes| {
      use sha2::{Digest, Sha256};
      Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    })
    .unwrap_or_else(|| "unknown".to_string());
  let hop = route.hops.get(hop_index).ok_or_else(|| "CAPCUT_TRANSLATION_ROUTE_INVALID".to_string())?;
  let effective = &route.effective_source_language;
  let route_kind = match route.route_kind {
    TranslationRouteKind::Direct => "DIRECT",
    TranslationRouteKind::Pivot => "PIVOT",
  };
  child.arg(worker_script).arg("--model-package").arg(model_path).arg("--model-id").arg(&hop.model_id).arg("--source-language").arg(&route.requested_source_language).arg("--effective-source-language").arg(effective).arg("--route-kind").arg(route_kind).arg("--hop-index").arg(hop_index.to_string()).env("CAPCUT_WORKER_SHA256", worker_sha).env("PYTHONPATH", python_path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
  let mut child = child.spawn().map_err(|_| "CAPCUT_TRANSLATION_PROCESS_START_FAILED".to_string())?;
  // Drain both pipes while the worker is running.  The worker emits one
  // sanitized diagnostic per OCR segment; waiting for exit before reading
  // stderr can fill the OS pipe and deadlock the worker until our watchdog
  // incorrectly reports CAPCUT_TRANSLATION_TIMEOUT.
  let mut stdout_pipe = child.stdout.take().ok_or_else(|| "CAPCUT_TRANSLATION_STDOUT_FAILED".to_string())?;
  let mut stderr_pipe = child.stderr.take().ok_or_else(|| "CAPCUT_TRANSLATION_STDERR_FAILED".to_string())?;
  let stdout_reader = thread::spawn(move || {
    let mut bytes = Vec::new();
    stdout_pipe.read_to_end(&mut bytes).map(|_| bytes)
  });
  let stderr_reader = thread::spawn(move || {
    let mut bytes = Vec::new();
    stderr_pipe.read_to_end(&mut bytes).map(|_| bytes)
  });
  let input = serde_json::to_vec(&payload).map_err(|_| "CAPCUT_TRANSLATION_REQUEST_INVALID".to_string())?;
  let stdin = match child.stdin.as_mut() {
    Some(stdin) => stdin,
    None => {
      let _ = child.kill();
      let _ = child.wait();
      let _ = stdout_reader.join();
      let _ = stderr_reader.join();
      return Err("CAPCUT_TRANSLATION_STDIN_FAILED".to_string());
    },
  };
  if stdin.write_all(&input).is_err() {
    let _ = child.kill();
    let _ = child.wait();
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    return Err("CAPCUT_TRANSLATION_STDIN_FAILED".to_string());
  }
  child.stdin.take();
  let deadline = Instant::now() + Duration::from_secs(120);
  let status = loop {
    match child.try_wait() {
      Ok(Some(status)) => break status,
      Err(_) => {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        return Err("CAPCUT_TRANSLATION_PROCESS_FAILED".to_string());
      },
      Ok(None) if Instant::now() >= deadline => {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        return Err("CAPCUT_TRANSLATION_TIMEOUT".to_string());
      },
      Ok(None) => thread::sleep(Duration::from_millis(25)),
    }
  };
  let stdout = stdout_reader.join().map_err(|_| "CAPCUT_TRANSLATION_PROCESS_FAILED".to_string())?.map_err(|_| "CAPCUT_TRANSLATION_PROCESS_FAILED".to_string())?;
  let stderr = stderr_reader.join().map_err(|_| "CAPCUT_TRANSLATION_PROCESS_FAILED".to_string())?.map_err(|_| "CAPCUT_TRANSLATION_PROCESS_FAILED".to_string())?;
  let stderr_text = String::from_utf8_lossy(&stderr).to_string();
  for line in stderr_text.lines().filter(|line| line.starts_with("CAPCUT_TRANSLATION_DIAGNOSTICS ")) {
    eprintln!("{line}");
  }
  if !status.success() {
    let worker_detail = serde_json::from_slice::<serde_json::Value>(&stdout).ok().and_then(|value| value.get("detail").and_then(serde_json::Value::as_str).map(str::to_owned)).or_else(|| String::from_utf8_lossy(&stderr).lines().next().map(str::to_owned)).unwrap_or_else(|| "worker exited without diagnostics".to_string());
    let sanitized = worker_detail.chars().map(|character| if character.is_control() { ' ' } else { character }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let failure = TranslationFailureDiagnostics::from_worker(&stderr_text, hop_index, &hop.model_id, segments.len(), status.code().unwrap_or(-1), &sanitized);
    return Err(format!("CAPCUT_TRANSLATION_FAILED:{}", serde_json::to_string(&failure).unwrap_or_else(|_| "{\"errorCode\":\"CAPCUT_TRANSLATION_FAILED\"}".to_string())));
  }
  let worker: WorkerResponse = serde_json::from_slice(&stdout).map_err(|_| "CAPCUT_TRANSLATION_OUTPUT_INVALID".to_string())?;
  let by_id = worker.segments.into_iter().map(|segment| (segment.id, segment.translated_text)).collect::<std::collections::HashMap<_, _>>();
  if segments.iter().any(|segment| by_id.get(&segment.id).map(|text| text.trim().is_empty()).unwrap_or(true)) {
    return Err("CAPCUT_TRANSLATION_OUTPUT_INCOMPLETE".to_string());
  }
  let translated = segments
    .iter()
    .map(|segment| {
      let text = by_id.get(&segment.id).cloned().unwrap_or_default();
      TranslationSegment { id: segment.id.clone(), start_ms: segment.start_ms, end_ms: segment.end_ms, text: text.clone(), translated_text: Some(text) }
    })
    .collect();
  Ok((worker.provider, translated))
}

pub fn translate_local(manager: &CapcutAutomationEngineManager, request: LocalTranslationRequest) -> Result<LocalTranslationResponse, String> {
  if request.segments.is_empty() {
    return Err("CAPCUT_TRANSLATION_SEGMENTS_EMPTY".to_string());
  }
  let route = plan_translation_route(&request.source_language, &request.target_language)?;
  if route.route_kind == TranslationRouteKind::Pivot {
    let engine = manager.resolve_first(CapcutEngineKind::Translation).map_err(|_| format!("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:{}", route.hops.iter().map(|hop| hop.model_id.as_str()).collect::<Vec<_>>().join(",")))?;
    let executable = engine.executable_path.ok_or_else(|| "CAPCUT_TRANSLATION_EXECUTABLE_MISSING".to_string())?;
    let worker_script = engine.worker_script_path.ok_or_else(|| "CAPCUT_TRANSLATION_WORKER_SCRIPT_MISSING".to_string())?;
    let mut current = request.segments.clone();
    let mut provider = String::new();
    for (hop_index, hop) in route.hops.iter().enumerate() {
      let (_, model_path) = manager.resolve_model_path(&hop.model_id).map_err(|_| format!("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:{}", hop.model_id))?;
      let (hop_provider, translated) = invoke_translation_worker(manager, &executable, &worker_script, &model_path, &route, hop_index, &current)?;
      provider = hop_provider;
      current = translated;
    }
    return Ok(LocalTranslationResponse { provider, model: route.hops.iter().map(|hop| hop.model_id.clone()).collect::<Vec<_>>().join("→"), segments: current });
  }
  if route.route_kind == TranslationRouteKind::Pivot {
    // Pivot execution is enabled only when both verified hop packages are
    // staged. Never silently feed Chinese text to the en→vi worker.
    return Err(format!("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:{}", route.hops.iter().map(|hop| hop.model_id.as_str()).collect::<Vec<_>>().join(",")));
  }
  let engine = manager.resolve_first(CapcutEngineKind::Translation).map_err(|_| format!("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:{}", route.hops[0].model_id))?;
  let executable = engine.executable_path.ok_or_else(|| "CAPCUT_TRANSLATION_EXECUTABLE_MISSING".to_string())?;
  let worker_script = engine.worker_script_path.ok_or_else(|| "CAPCUT_TRANSLATION_WORKER_SCRIPT_MISSING".to_string())?;
  let (_, model_path) = manager.resolve_model_path(&route.hops[0].model_id).map_err(|_| format!("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:{}", route.hops[0].model_id))?;
  let (provider, segments) = invoke_translation_worker(manager, &executable, &worker_script, &model_path, &route, 0, &request.segments)?;
  Ok(LocalTranslationResponse { provider, model: route.hops[0].model_id.clone(), segments })
}

/// Translate mixed-language transcript segments without forcing every segment
/// through the first segment's route. The returned route list is deterministic
/// and can be persisted as receipt evidence.
pub fn translate_segments_by_language(manager: &CapcutAutomationEngineManager, segments: &[TranslationSegment], target_language: &str) -> Result<(Vec<TranslationSegment>, Vec<TranslationRoute>), String> {
  let target = target_language.trim().to_ascii_lowercase();
  let mut groups: Vec<(String, Vec<TranslationSegment>)> = Vec::new();
  for segment in segments {
    let source = crate::services::pipeline::capcut_automation_ocr::infer_text_language(&segment.text);
    if let Some((last_source, last_segments)) = groups.last_mut() {
      if *last_source == source {
        last_segments.push(segment.clone());
        continue;
      }
    }
    groups.push((source, vec![segment.clone()]));
  }

  // Keep contiguous segments together.  This preserves source order while
  // avoiding one Python/Argos process per subtitle segment on real videos.
  let mut translated = Vec::with_capacity(segments.len());
  let mut routes = Vec::new();
  for (source, group) in groups {
    if source == "vi" || source == target {
      translated.extend(group.into_iter().map(|segment| TranslationSegment { translated_text: Some(segment.text.clone()), ..segment }));
      continue;
    }
    let route = plan_translation_route(&source, &target)?;
    let response = translate_local(manager, LocalTranslationRequest { segments: group, source_language: source, target_language: target.clone() })?;
    routes.push(route);
    translated.extend(response.segments);
  }
  Ok((translated, routes))
}

/// Reject clearly corrupted model output before it reaches TTS or subtitle
/// rendering. This is intentionally conservative: it only catches repeated
/// token/romanization loops and never attempts to rewrite the translation.
pub fn validate_translation_quality(segments: &[TranslationSegment]) -> Result<(), String> {
  validate_translation_quality_with_sources(segments, None)
}

/// Validate translated output against the immutable source segments.  A
/// pivot route reuses `TranslationSegment.text` for the next hop, so the
/// translated `text` field is intentionally not the original transcript by
/// the time the final quality gate runs.  Supplying the original segments
/// prevents the echo check from comparing the output with itself.
pub fn validate_translation_quality_against_source(segments: &[TranslationSegment], source_segments: &[TranslationSegment]) -> Result<(), String> {
  let sources = source_segments.iter().map(|segment| (segment.id.as_str(), segment.text.as_str())).collect::<std::collections::HashMap<_, _>>();
  validate_translation_quality_with_sources(segments, Some(&sources))
}

fn validate_translation_quality_with_sources(segments: &[TranslationSegment], sources: Option<&std::collections::HashMap<&str, &str>>) -> Result<(), String> {
  if segments.is_empty() {
    return Err("CAPCUT_TRANSLATION_OUTPUT_INCOMPLETE".to_string());
  }
  for segment in segments {
    let text = segment.translated_text.as_deref().unwrap_or(&segment.text).trim();
    if text.is_empty() {
      return Err("CAPCUT_TRANSLATION_OUTPUT_INCOMPLETE".to_string());
    }
    // A non-Vietnamese source echoed byte-for-byte is not a translation.  Do
    // this only for languages that should change so Vietnamese names and
    // already-localized segments remain valid.
    let source_text = sources.and_then(|items| items.get(segment.id.as_str()).copied()).unwrap_or(&segment.text);
    let source_language = crate::services::pipeline::capcut_automation_ocr::infer_text_language(source_text);
    if source_language != "vi" && normalize_quality_text(text) == normalize_quality_text(source_text) {
      return Err("CAPCUT_TRANSLATION_QUALITY_UNRESOLVED".to_string());
    }
    let tokens = text.split_whitespace().map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '-').to_ascii_lowercase()).filter(|token| !token.is_empty()).collect::<Vec<_>>();
    let hyphenated = tokens.iter().filter(|token| token.matches('-').count() >= 2).count();
    let repeated_hyphenated = tokens.len() >= 4 && hyphenated as f32 / tokens.len() as f32 >= 0.7;
    if tokens.len() < 8 && !repeated_hyphenated {
      continue;
    }
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    for token in &tokens {
      *counts.entry(token.as_str()).or_default() += 1;
    }
    let repeated = counts.values().map(|count| count.saturating_sub(1)).sum::<usize>();
    if repeated as f32 / tokens.len() as f32 >= 0.45 || repeated_hyphenated {
      return Err("CAPCUT_TRANSLATION_QUALITY_UNRESOLVED".to_string());
    }
  }
  Ok(())
}

fn normalize_quality_text(value: &str) -> String {
  value.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  #[test]
  fn real_artcraft_worker_translates_fixture_when_resources_are_staged() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("capcut-automation");
    if !root.join("runtime/python314/python.exe").is_file() || !root.join("models/argos/translate-en_vi-1_9.argosmodel").is_file() {
      return;
    }
    let manager = CapcutAutomationEngineManager::new(Some(root), None, tempfile::tempdir().unwrap().path().to_path_buf());
    let response = translate_local(&manager, LocalTranslationRequest { segments: vec![TranslationSegment { id: "s1".into(), start_ms: 0, end_ms: 1_000, text: "Good morning, how are you?".into(), translated_text: None }], source_language: "en".into(), target_language: "vi".into() }).unwrap();
    assert_eq!(response.segments[0].id, "s1");
    assert!(response.segments[0].translated_text.as_deref().unwrap_or_default().contains("Chào"));
  }

  #[test]
  fn chinese_routes_are_explicit_pivots_and_never_direct_en_vi() {
    let simplified = plan_translation_route("zh-CN", "vi").unwrap();
    assert_eq!(simplified.effective_source_language, "zh");
    assert_eq!(simplified.route_kind, TranslationRouteKind::Pivot);
    assert_eq!(simplified.hops.iter().map(|hop| hop.model_id.as_str()).collect::<Vec<_>>(), ["argos-zh-en-1-9", "argos-en-vi-1-9"]);

    let traditional = plan_translation_route("chi_tra", "vi").unwrap();
    assert_eq!(traditional.effective_source_language, "zt");
    assert_eq!(traditional.route_kind, TranslationRouteKind::Pivot);
    assert_eq!(traditional.hops[0].model_id, "argos-zt-en-1-9");
  }

  #[test]
  fn unsupported_or_auto_source_fails_before_worker_spawn() {
    assert!(plan_translation_route("auto", "vi").unwrap_err().starts_with("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:"));
    assert!(plan_translation_route("ja", "vi").unwrap_err().starts_with("CAPCUT_TRANSLATION_ROUTE_UNAVAILABLE:"));
  }

  #[test]
  fn worker_failure_receipt_keeps_stage_hop_and_exit_code_separate() {
    let stderr = r#"CAPCUT_TRANSLATION_DIAGNOSTICS {"hopIndex":0,"operationStage":"DECODE","modelId":"argos-zt-en-1-9","outerType":"list","firstItemType":"list","firstTokenType":"str","emptyTokenSequenceCount":0,"maxTokenCount":12,"exceptionType":"TypeError","sanitizedMessage":"decode failed"}"#;
    let diagnostics = TranslationFailureDiagnostics::from_worker(stderr, 0, "argos-zt-en-1-9", 4, 1, "worker failed");
    assert_eq!(diagnostics.hop_index, 0);
    assert_eq!(diagnostics.operation_stage, "DECODE");
    assert_eq!(diagnostics.exit_code, 1);
    assert_eq!(diagnostics.segment_count, 4);
    assert_eq!(diagnostics.first_token_type.as_deref(), Some("str"));
    assert!(!diagnostics.sanitized_message.contains("decode failed\n"));
  }

  #[test]
  fn translation_quality_rejects_repeated_romanized_output() {
    let segments = vec![TranslationSegment { id: "s1".into(), start_ms: 0, end_ms: 1_000, text: "source".into(), translated_text: Some("Segi-Wang-Lio-Defu-Gai Segi-Wang-Lio-Defu-Gai Segi-Wang-Lio-Defu-Gai Segi-Wang-Lio-Defu-Gai".into()) }];
    assert_eq!(validate_translation_quality(&segments).unwrap_err(), "CAPCUT_TRANSLATION_QUALITY_UNRESOLVED");
  }

  #[test]
  fn translation_quality_rejects_non_vietnamese_echo() {
    let segments = vec![TranslationSegment { id: "s1".into(), start_ms: 0, end_ms: 1_000, text: "Good morning".into(), translated_text: Some("Good morning".into()) }];
    assert_eq!(validate_translation_quality(&segments).unwrap_err(), "CAPCUT_TRANSLATION_QUALITY_UNRESOLVED");
  }

  #[test]
  fn translation_quality_uses_original_source_after_pivot_hop() {
    let source = vec![TranslationSegment { id: "s1".into(), start_ms: 0, end_ms: 1_000, text: "ä»Šå¤©å¤©æ°£å¾ˆå¥½".into(), translated_text: None }];
    let translated = vec![TranslationSegment { id: "s1".into(), start_ms: 0, end_ms: 1_000, text: "Hôm nay thời tiết rất đẹp".into(), translated_text: Some("Hôm nay thời tiết rất đẹp".into()) }];
    assert!(validate_translation_quality_against_source(&translated, &source).is_ok());
  }

  #[test]
  #[ignore]
  fn real_traditional_pivot_uses_two_verified_workers() {
    let manager = CapcutAutomationEngineManager::new(None, None, PathBuf::from("C:\\Users\\thecong\\Artcraft"));
    let response = translate_local(&manager, LocalTranslationRequest { segments: vec![TranslationSegment { id: "s1".into(), start_ms: 0, end_ms: 1_000, text: "今天天气很好".into(), translated_text: None }], source_language: "zt".into(), target_language: "vi".into() }).unwrap();
    assert_eq!(response.model, "argos-zt-en-1-9→argos-en-vi-1-9");
    assert!(!response.segments[0].text.trim().is_empty());
    assert!(!response.segments[0].text.chars().all(|character| character >= '\u{4e00}' && character <= '\u{9fff}'));
  }
}
