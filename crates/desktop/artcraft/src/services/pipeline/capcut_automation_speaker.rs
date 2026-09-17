//! ArtCraft-owned speaker diarization boundary.
//!
//! A verified Sherpa-ONNX worker is used when its model files are installed;
//! otherwise the command fails explicitly instead of fabricating labels.

use super::capcut_automation_engine_manager::{CapcutAutomationEngineManager, CapcutEngineKind};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerTurn {
  pub speaker_id: String,
  pub start_ms: u64,
  pub end_ms: u64,
  #[serde(default)]
  pub confidence: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerSegment {
  pub id: String,
  pub start_ms: u64,
  pub end_ms: u64,
  pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerAssignment {
  pub segment_id: String,
  pub speaker_id: String,
  pub voice_id: Option<String>,
  pub overlap_ms: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSpeakerRequest {
  pub audio_path: String,
  #[serde(default = "default_num_speakers")]
  pub num_speakers: u32,
}

fn default_num_speakers() -> u32 {
  0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSpeakerResponse {
  pub engine: String,
  pub turns: Vec<SpeakerTurn>,
  pub sample_rate: u32,
}

/// Assign each transcript segment to the turn with greatest temporal overlap.
pub fn assign_speakers(segments: &[SpeakerSegment], turns: &[SpeakerTurn], voices: &std::collections::HashMap<String, String>) -> Vec<SpeakerAssignment> {
  segments
    .iter()
    .filter_map(|segment| {
      let best = turns
        .iter()
        .filter_map(|turn| {
          let start = segment.start_ms.max(turn.start_ms);
          let end = segment.end_ms.min(turn.end_ms);
          (end > start).then(|| (end - start, turn))
        })
        .max_by(|(overlap_a, turn_a), (overlap_b, turn_b)| overlap_a.cmp(overlap_b).then_with(|| turn_b.speaker_id.cmp(&turn_a.speaker_id)));
      best.map(|(overlap_ms, turn)| SpeakerAssignment { segment_id: segment.id.clone(), speaker_id: turn.speaker_id.clone(), voice_id: voices.get(&turn.speaker_id).cloned(), overlap_ms })
    })
    .collect()
}

/// Run the verified local worker. No PATH lookup, network request, or legacy
/// source fallback is allowed here.
pub fn detect_speakers_local(manager: &CapcutAutomationEngineManager, request: LocalSpeakerRequest) -> Result<LocalSpeakerResponse, String> {
  let audio = PathBuf::from(request.audio_path.trim());
  if !audio.is_file() {
    return Err("SPEAKER_AUDIO_NOT_FOUND".to_string());
  }
  if voicestudio_diarization_enabled() {
    return detect_speakers_with_voicestudio(request);
  }
  let resolved = manager.resolve_first(CapcutEngineKind::SpeakerDiarization)?;
  let executable = resolved.executable_path.ok_or_else(|| "SPEAKER_EXECUTABLE_MISSING".to_string())?;
  let worker = resolved.worker_script_path.ok_or_else(|| "SPEAKER_WORKER_MISSING".to_string())?;
  let model_root = resolved.resource_path.parent().and_then(|path| path.parent()).ok_or_else(|| "SPEAKER_MODEL_ROOT_INVALID".to_string())?;
  let resource_root = model_root.parent().and_then(|path| path.parent()).ok_or_else(|| "SPEAKER_RESOURCE_ROOT_INVALID".to_string())?;
  let python_root = resource_root.join("python");
  let output = crate::core::lifecycle::startup::tasks::background_command::background_command(Command::new(executable)).env("PYTHONPATH", python_root).arg(worker).arg("--audio").arg(&audio).arg("--model-root").arg(model_root).arg("--num-speakers").arg(request.num_speakers.to_string()).output().map_err(|error| format!("SPEAKER_WORKER_START_FAILED:{error}"))?;
  if !output.status.success() {
    return Err(format!("SPEAKER_WORKER_FAILED:{}", String::from_utf8_lossy(&output.stderr).trim()));
  }
  serde_json::from_slice::<LocalSpeakerResponse>(&output.stdout).map_err(|error| format!("SPEAKER_RESPONSE_INVALID:{error}"))
}

fn voicestudio_diarization_enabled() -> bool {
  std::env::var("FLOWORD_DIARIZATION_PROVIDER").ok().is_some_and(|value| value.trim().eq_ignore_ascii_case("voicestudio"))
}

/// Use VoiceStudio's full-audio dubbing transcribe endpoint when explicitly
/// selected. That endpoint performs ASR and diarization in one persisted job;
/// the returned speaker-labelled segments are collapsed into temporal turns so
/// ArtCraft's existing deterministic assignment stage can remain unchanged.
fn detect_speakers_with_voicestudio(request: LocalSpeakerRequest) -> Result<LocalSpeakerResponse, String> {
  let audio = PathBuf::from(request.audio_path.trim());
  let num_speakers = (request.num_speakers > 0).then_some(request.num_speakers.min(20) as u16);
  let response = run_voice_studio_future(async move {
    let client = voicestudio_client::VoiceStudioClient::from_env().map_err(|error| error.to_string())?;
    let upload = client.dub_upload(&audio, None, "audio", None).await.map_err(|error| error.to_string())?;
    let job_id = upload.get("job_id").and_then(serde_json::Value::as_str).ok_or_else(|| "SPEAKER_VOICESTUDIO_JOB_ID_MISSING".to_string())?.to_string();
    if let Some(task_id) = upload.get("task_id").and_then(serde_json::Value::as_str) {
      let timeout_seconds = std::env::var("FLOWORD_DIARIZATION_TIMEOUT_SECONDS").ok().and_then(|value| value.trim().parse::<u64>().ok()).map(|value| value.clamp(60, 7_200)).unwrap_or(600);
      client.wait_for_task(task_id, Duration::from_secs(timeout_seconds)).await.map_err(|error| error.to_string())?;
    }
    client.dub_transcribe(&job_id, num_speakers).await.map_err(|error| error.to_string())
  })?;
  let turns = parse_voicestudio_turns(&response)?;
  Ok(LocalSpeakerResponse { engine: "voicestudio-diarization".to_string(), turns, sample_rate: 16_000 })
}

fn parse_voicestudio_turns(response: &serde_json::Value) -> Result<Vec<SpeakerTurn>, String> {
  let segments = response.get("segments").and_then(serde_json::Value::as_array).ok_or_else(|| "SPEAKER_RESPONSE_INVALID:segments".to_string())?;
  let mut turns: Vec<SpeakerTurn> = Vec::new();
  for segment in segments {
    let Some(speaker_id) = segment.get("speaker_id").or_else(|| segment.get("speaker")).and_then(serde_json::Value::as_str).filter(|value| !value.trim().is_empty()) else {
      continue;
    };
    let start_ms = segment.get("start").and_then(value_to_millis).or_else(|| segment.get("start_ms").and_then(serde_json::Value::as_u64)).unwrap_or(0);
    let end_ms = segment.get("end").and_then(value_to_millis).or_else(|| segment.get("end_ms").and_then(serde_json::Value::as_u64)).unwrap_or(start_ms);
    if end_ms <= start_ms {
      continue;
    }
    if let Some(previous) = turns.last_mut() {
      if previous.speaker_id == speaker_id && previous.end_ms >= start_ms.saturating_sub(250) {
        previous.end_ms = previous.end_ms.max(end_ms);
        continue;
      }
    }
    turns.push(SpeakerTurn { speaker_id: speaker_id.to_string(), start_ms, end_ms, confidence: segment.get("confidence").and_then(serde_json::Value::as_f64) });
  }
  if turns.is_empty() {
    return Err("SPEAKER_RESPONSE_EMPTY".to_string());
  }
  Ok(turns)
}

fn value_to_millis(value: &serde_json::Value) -> Option<u64> {
  let seconds = value.as_f64()?;
  (seconds.is_finite() && seconds >= 0.0).then(|| (seconds * 1_000.0).round() as u64)
}

fn run_voice_studio_future<F, T>(future: F) -> Result<T, String>
where
  F: std::future::Future<Output = Result<T, String>> + Send + 'static,
  T: Send + 'static,
{
  let join = std::thread::spawn(move || {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|error| format!("SPEAKER_VOICESTUDIO_RUNTIME_FAILED:{error}"))?;
    runtime.block_on(future)
  });
  join.join().map_err(|_| "SPEAKER_VOICESTUDIO_REQUEST_PANICKED".to_string())?
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap;

  #[test]
  fn assigns_by_largest_overlap_and_preserves_voice_mapping() {
    let segments = vec![SpeakerSegment { id: "s1".into(), start_ms: 0, end_ms: 1_000, text: "hello".into() }];
    let turns = vec![SpeakerTurn { speaker_id: "speaker-1".into(), start_ms: 0, end_ms: 300, confidence: Some(0.8) }, SpeakerTurn { speaker_id: "speaker-2".into(), start_ms: 250, end_ms: 900, confidence: Some(0.9) }];
    let voices = HashMap::from([(String::from("speaker-2"), String::from("vi-VN-NamMinhNeural"))]);
    let result = assign_speakers(&segments, &turns, &voices);
    assert_eq!(result, vec![SpeakerAssignment { segment_id: "s1".into(), speaker_id: "speaker-2".into(), voice_id: Some("vi-VN-NamMinhNeural".into()), overlap_ms: 650 }]);
  }

  #[test]
  fn does_not_assign_without_overlap() {
    let segments = vec![SpeakerSegment { id: "s1".into(), start_ms: 1_000, end_ms: 2_000, text: "none".into() }];
    let turns = vec![SpeakerTurn { speaker_id: "speaker-1".into(), start_ms: 0, end_ms: 999, confidence: None }];
    assert!(assign_speakers(&segments, &turns, &HashMap::new()).is_empty());
  }

  #[test]
  fn parses_and_merges_voice_studio_speaker_segments() {
    let response = serde_json::json!({"segments": [
      {"speaker_id": "S1", "start": 0.0, "end": 1.0},
      {"speaker_id": "S1", "start": 1.1, "end": 2.0},
      {"speaker_id": "S2", "start": 2.0, "end": 3.5}
    ]});
    let turns = parse_voicestudio_turns(&response).expect("speaker turns should parse");
    assert_eq!(turns.len(), 2);
    assert_eq!((turns[0].start_ms, turns[0].end_ms), (0, 2_000));
    assert_eq!(turns[1].speaker_id, "S2");
  }
}
