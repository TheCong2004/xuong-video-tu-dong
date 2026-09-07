//! ArtCraft-owned speaker diarization boundary.
//!
//! A verified Sherpa-ONNX worker is used when its model files are installed;
//! otherwise the command fails explicitly instead of fabricating labels.

use super::capcut_automation_engine_manager::{CapcutAutomationEngineManager, CapcutEngineKind};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

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
  let resolved = manager.resolve_first(CapcutEngineKind::SpeakerDiarization)?;
  let executable = resolved.executable_path.ok_or_else(|| "SPEAKER_EXECUTABLE_MISSING".to_string())?;
  let worker = resolved.worker_script_path.ok_or_else(|| "SPEAKER_WORKER_MISSING".to_string())?;
  let model_root = resolved.resource_path.parent().and_then(|path| path.parent()).ok_or_else(|| "SPEAKER_MODEL_ROOT_INVALID".to_string())?;
  let resource_root = model_root.parent().and_then(|path| path.parent()).ok_or_else(|| "SPEAKER_RESOURCE_ROOT_INVALID".to_string())?;
  let python_root = resource_root.join("python");
  let output = Command::new(executable).env("PYTHONPATH", python_root).arg(worker).arg("--audio").arg(&audio).arg("--model-root").arg(model_root).arg("--num-speakers").arg(request.num_speakers.to_string()).output().map_err(|error| format!("SPEAKER_WORKER_START_FAILED:{error}"))?;
  if !output.status.success() {
    return Err(format!("SPEAKER_WORKER_FAILED:{}", String::from_utf8_lossy(&output.stderr).trim()));
  }
  serde_json::from_slice::<LocalSpeakerResponse>(&output.stdout).map_err(|error| format!("SPEAKER_RESPONSE_INVALID:{error}"))
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
}
