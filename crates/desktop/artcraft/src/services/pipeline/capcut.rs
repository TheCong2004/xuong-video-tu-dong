//! Canonical Phase 6 artifact handoff into the existing CapCut Mate client.

use crate::services::pipeline::caption_segmenter::CaptionSegment;
use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef, PipelineContext, PipelineContractError, StageId};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaPlacement {
  pub artifact_id: String,
  pub path: String,
  pub source_start_us: u64,
  pub start_us: u64,
  pub duration_us: u64,
}

#[derive(Clone, Debug)]
pub struct CapcutInput {
  pub visual_artifact_ids: Vec<String>,
  pub voice_audio_artifact_id: String,
  pub timeline_artifact_id: String,
  pub captions_artifact_id: String,
  pub input_artifact_ids: Vec<String>,
  pub video_segments: Vec<MediaPlacement>,
  pub voice_segment: MediaPlacement,
  pub captions: Vec<CaptionSegment>,
  pub duration_us: u64,
}

pub fn prepare_capcut(context: &PipelineContext) -> Result<CapcutInput, PipelineContractError> {
  let visuals = context.artifact_refs.iter().filter(|artifact| matches!(artifact.kind, ArtifactKind::SourceVideo | ArtifactKind::GeneratedVideo)).collect::<Vec<_>>();
  if visuals.is_empty() {
    return Err(PipelineContractError::MissingArtifact { stage_id: StageId::Capcut, kind: ArtifactKind::GeneratedVideo });
  }
  let voice = stage_artifact(context, StageId::Voice, ArtifactKind::VoiceAudio)?;
  let timeline_ref = stage_artifact(context, StageId::MediaTimeline, ArtifactKind::Timeline)?;
  let captions_ref = stage_artifact(context, StageId::MediaTimeline, ArtifactKind::Captions)?;
  let timeline = read_json(timeline_ref)?;
  let captions_doc = read_json(captions_ref)?;
  let duration_us = seconds_to_us(required_number(&timeline, "durationSeconds", timeline_ref)?, timeline_ref)?;

  let tracks = timeline.get("tracks").and_then(Value::as_array).ok_or_else(|| invalid(timeline_ref, "timeline tracks are missing"))?;
  let video_track = track(tracks, "video", timeline_ref)?;
  let voice_track = track(tracks, "voice", timeline_ref)?;

  let video_segments = segments(video_track, timeline_ref)?.iter().map(|segment| {
    let artifact_id = segment.get("artifactId").and_then(Value::as_str).ok_or_else(|| invalid(timeline_ref, "video segment artifact ID is missing"))?;
    let visual = visuals.iter().find(|artifact| artifact.artifact_id == artifact_id).ok_or_else(|| invalid(timeline_ref, "video segment references an unregistered visual artifact"))?;
    placement(segment, visual, duration_us)
  }).collect::<Result<Vec<_>, _>>()?;
  if video_segments.is_empty() {
    return Err(invalid(timeline_ref, "video track has no segments"));
  }

  let voice_segments = segments(voice_track, timeline_ref)?;
  if voice_segments.len() != 1 {
    return Err(invalid(timeline_ref, "voice track must contain exactly one segment"));
  }
  let voice_segment = placement(&voice_segments[0], voice, duration_us)?;

  let cues = captions_doc.get("cues").and_then(Value::as_array).ok_or_else(|| invalid(captions_ref, "caption cues are missing"))?;
  let captions = cues
    .iter()
    .map(|cue| {
      let text = cue.get("text").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| invalid(captions_ref, "caption text is empty"))?;
      let start = cue.get("start").and_then(Value::as_f64).ok_or_else(|| invalid(captions_ref, "caption start is missing"))?;
      let end = cue.get("end").and_then(Value::as_f64).ok_or_else(|| invalid(captions_ref, "caption end is missing"))?;
      let start_us = seconds_to_us(start, captions_ref)?;
      let end_us = seconds_to_us(end, captions_ref)?;
      if end_us <= start_us || end_us > duration_us + 250_000 {
        return Err(invalid(captions_ref, "caption timing is outside timeline duration"));
      }
      Ok(CaptionSegment { text: text.to_string(), start: start_us, end: end_us })
    })
    .collect::<Result<Vec<_>, _>>()?;
  if captions.is_empty() {
    return Err(invalid(captions_ref, "caption artifact has no cues"));
  }

  let visual_artifact_ids = visuals.iter().map(|artifact| artifact.artifact_id.clone()).collect::<Vec<_>>();
  let mut input_artifact_ids = visual_artifact_ids.clone();
  input_artifact_ids.extend([voice.artifact_id.clone(), captions_ref.artifact_id.clone(), timeline_ref.artifact_id.clone()]);
  Ok(CapcutInput { visual_artifact_ids, voice_audio_artifact_id: voice.artifact_id.clone(), timeline_artifact_id: timeline_ref.artifact_id.clone(), captions_artifact_id: captions_ref.artifact_id.clone(), input_artifact_ids, video_segments, voice_segment, captions, duration_us })
}

fn stage_artifact(context: &PipelineContext, producer: StageId, kind: ArtifactKind) -> Result<&ArtifactRef, PipelineContractError> {
  let ids = context.stage_states.iter().find(|state| state.stage_id == producer).map(|state| state.output_artifact_ids.as_slice()).unwrap_or_default();
  let id = ids.iter().find(|id| context.artifact_refs.iter().any(|artifact| artifact.artifact_id == id.as_str() && artifact.kind == kind)).ok_or(PipelineContractError::MissingArtifact { stage_id: StageId::Capcut, kind })?;
  context.require_artifact_id(StageId::Capcut, id)
}

fn read_json(artifact: &ArtifactRef) -> Result<Value, PipelineContractError> {
  let bytes = std::fs::read(&artifact.location).map_err(|error| invalid(artifact, error.to_string()))?;
  serde_json::from_slice(&bytes).map_err(|error| invalid(artifact, error.to_string()))
}

fn track<'a>(tracks: &'a [Value], kind: &str, artifact: &ArtifactRef) -> Result<&'a Value, PipelineContractError> {
  tracks.iter().find(|track| track.get("kind").and_then(Value::as_str) == Some(kind)).ok_or_else(|| invalid(artifact, format!("{kind} track is missing")))
}

fn segments<'a>(track: &'a Value, artifact: &ArtifactRef) -> Result<&'a Vec<Value>, PipelineContractError> {
  track.get("segments").and_then(Value::as_array).ok_or_else(|| invalid(artifact, "track segments are missing"))
}

fn placement(segment: &Value, artifact: &ArtifactRef, timeline_duration_us: u64) -> Result<MediaPlacement, PipelineContractError> {
  if segment.get("artifactId").and_then(Value::as_str) != Some(artifact.artifact_id.as_str()) {
    return Err(invalid(artifact, "timeline segment references the wrong artifact ID"));
  }
  let path = segment.get("path").and_then(Value::as_str).ok_or_else(|| invalid(artifact, "timeline segment path is missing"))?;
  let timeline_path = std::path::Path::new(path).canonicalize().map_err(|error| invalid(artifact, error.to_string()))?;
  let artifact_path = std::path::Path::new(&artifact.location).canonicalize().map_err(|error| invalid(artifact, error.to_string()))?;
  if timeline_path != artifact_path || !artifact_path.is_file() {
    return Err(invalid(artifact, "timeline segment path does not resolve to its artifact"));
  }
  let start_us = seconds_to_us(segment.get("startSeconds").and_then(Value::as_f64).ok_or_else(|| invalid(artifact, "segment start is missing"))?, artifact)?;
  let end_us = seconds_to_us(segment.get("endSeconds").and_then(Value::as_f64).ok_or_else(|| invalid(artifact, "segment end is missing"))?, artifact)?;
  if end_us <= start_us || end_us > timeline_duration_us + 250_000 {
    return Err(invalid(artifact, "segment timing is outside timeline duration"));
  }
  let source_start_us = segment.get("sourceStartSeconds").and_then(Value::as_f64).map(|value| seconds_to_us(value, artifact)).transpose()?.unwrap_or(0);
  Ok(MediaPlacement { artifact_id: artifact.artifact_id.clone(), path: artifact_path.to_string_lossy().to_string(), source_start_us, start_us, duration_us: end_us - start_us })
}

fn required_number(value: &Value, key: &str, artifact: &ArtifactRef) -> Result<f64, PipelineContractError> {
  value.get(key).and_then(Value::as_f64).ok_or_else(|| invalid(artifact, format!("{key} is missing")))
}

fn seconds_to_us(seconds: f64, artifact: &ArtifactRef) -> Result<u64, PipelineContractError> {
  if !seconds.is_finite() || seconds < 0.0 || seconds > (u64::MAX as f64 / 1_000_000.0) {
    return Err(invalid(artifact, "timing value is invalid"));
  }
  Ok((seconds * 1_000_000.0).round() as u64)
}

fn invalid(artifact: &ArtifactRef, message: impl Into<String>) -> PipelineContractError {
  PipelineContractError::InvalidArtifact { artifact_id: artifact.artifact_id.clone(), message: message.into() }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::artifact_store::ArtifactStore;
  use crate::services::pipeline::clients::capcut_mate_client::{add_audio, add_captions, add_videos, create_draft, inspect_draft, register_artifact_asset, save_draft, verify_draft_exists, DEFAULT_HEIGHT, DEFAULT_WIDTH};
  use crate::services::pipeline::contracts::{ContentSource, StageState, StageStatus};
  use serde_json::json;

  fn artifact(id: &str, kind: ArtifactKind, stage: StageId, path: &std::path::Path) -> ArtifactRef {
    ArtifactRef { artifact_id: id.into(), kind, produced_by_stage: stage, location: path.to_string_lossy().to_string(), mime_type: None, metadata: json!({}) }
  }

  #[test]
  fn prepares_real_artifact_id_handoff_and_measured_captions() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.mp4");
    let voice = temp.path().join("voice.mp3");
    let timeline = temp.path().join("timeline.json");
    let captions = temp.path().join("captions.json");
    std::fs::write(&source, b"video").unwrap();
    std::fs::write(&voice, b"audio").unwrap();
    std::fs::write(&timeline, serde_json::to_vec(&json!({"durationSeconds": 2.0, "tracks": [{"kind":"video","segments":[{"artifactId":"source","path":source,"startSeconds":0.0,"endSeconds":2.0}]},{"kind":"voice","segments":[{"artifactId":"voice","path":voice,"startSeconds":0.0,"endSeconds":2.0}]}]})).unwrap()).unwrap();
    std::fs::write(&captions, br#"{"cues":[{"start":0.0,"end":2.0,"text":"hello"}]}"#).unwrap();
    let refs = vec![artifact("source", ArtifactKind::SourceVideo, StageId::IngestAnalyze, &source), artifact("voice", ArtifactKind::VoiceAudio, StageId::Voice, &voice), artifact("timeline", ArtifactKind::Timeline, StageId::MediaTimeline, &timeline), artifact("captions", ArtifactKind::Captions, StageId::MediaTimeline, &captions)];
    let stage = |stage_id, ids: Vec<&str>| StageState { stage_id, status: StageStatus::Completed, attempt: 1, started_at: None, finished_at: None, input_artifact_ids: vec![], output_artifact_ids: ids.into_iter().map(str::to_string).collect(), service: None, error: None };
    let context = PipelineContext { job_id: "job".into(), project_id: None, workflow_mode: "storytelling".into(), content_source: Some(ContentSource::LocalMedia.as_str().to_string()), prompt: "p".into(), model_id: None, voice_id: None, language: "en".into(), target_duration_seconds: 2, output_mode: "draft_only".into(), source_url: None, local_file: Some(source.to_string_lossy().to_string()), story_url: None, research_enabled: false, research_platform: None, research_query: None, research_mode: None, xhs_variant: None, artifact_refs: refs, stage_states: vec![stage(StageId::IngestAnalyze, vec!["source"]), stage(StageId::Voice, vec!["voice"]), stage(StageId::MediaTimeline, vec!["timeline", "captions"])] };
    let input = prepare_capcut(&context).unwrap();
    assert_eq!(input.input_artifact_ids, vec!["source", "voice", "captions", "timeline"]);
    assert_eq!(input.video_segments[0].duration_us, 2_000_000);
    assert_eq!(input.captions[0], CaptionSegment { text: "hello".into(), start: 0, end: 2_000_000 });
  }

  #[test]
  fn accepts_registered_generated_video_without_source_artifact() {
    let temp = tempfile::tempdir().unwrap();
    let generated = temp.path().join("generated.mp4");
    let voice = temp.path().join("voice.mp3");
    let timeline = temp.path().join("timeline.json");
    let captions = temp.path().join("captions.json");
    std::fs::write(&generated, b"generated-video").unwrap();
    std::fs::write(&voice, b"voice").unwrap();
    std::fs::write(&timeline, serde_json::to_vec(&json!({"durationSeconds": 2.0, "tracks": [{"kind":"video","segments":[{"artifactId":"generated","path":generated,"sourceStartSeconds":0.0,"startSeconds":0.0,"endSeconds":2.0}]},{"kind":"voice","segments":[{"artifactId":"voice","path":voice,"startSeconds":0.0,"endSeconds":2.0}]}]})).unwrap()).unwrap();
    std::fs::write(&captions, br#"{"cues":[{"start":0.0,"end":2.0,"text":"hello"}]}"#).unwrap();
    let refs = vec![artifact("generated", ArtifactKind::GeneratedVideo, StageId::MediaTimeline, &generated), artifact("voice", ArtifactKind::VoiceAudio, StageId::Voice, &voice), artifact("timeline", ArtifactKind::Timeline, StageId::MediaTimeline, &timeline), artifact("captions", ArtifactKind::Captions, StageId::MediaTimeline, &captions)];
    let stage = |stage_id, ids: Vec<&str>| StageState { stage_id, status: StageStatus::Completed, attempt: 1, started_at: None, finished_at: None, input_artifact_ids: vec![], output_artifact_ids: ids.into_iter().map(str::to_string).collect(), service: None, error: None };
    let context = PipelineContext { job_id: "original".into(), project_id: None, workflow_mode: "original".into(), content_source: Some(ContentSource::PromptOnly.as_str().to_string()), prompt: "p".into(), model_id: None, voice_id: None, language: "en".into(), target_duration_seconds: 2, output_mode: "draft_only".into(), source_url: None, local_file: None, story_url: None, research_enabled: false, research_platform: None, research_query: None, research_mode: None, xhs_variant: None, artifact_refs: refs, stage_states: vec![stage(StageId::Voice, vec!["voice"]), stage(StageId::MediaTimeline, vec!["generated", "timeline", "captions"])] };
    let input = prepare_capcut(&context).unwrap();
    assert_eq!(input.visual_artifact_ids, vec!["generated"]);
    assert_eq!(input.video_segments[0].artifact_id, "generated");
  }

  #[tokio::test]
  #[ignore = "requires the local unified backend and Phase 4/5 runtime artifacts"]
  async fn runtime_creates_saves_verifies_and_registers_real_draft() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().to_path_buf();
    let source = root.join("test_data/video/mp4/golden_sun_garoh.mp4");
    let voice = root.join("artifacts/phase4-runtime-stage/voice/voice.mp3");
    let timeline = root.join("artifacts/phase5-runtime-rust/media_timeline/timeline.json");
    let captions = root.join("artifacts/phase5-runtime-rust/media_timeline/captions.json");
    let refs = vec![artifact("runtime-source-video-artifact", ArtifactKind::SourceVideo, StageId::IngestAnalyze, &source), artifact("runtime-voice-audio-artifact", ArtifactKind::VoiceAudio, StageId::Voice, &voice), artifact("runtime-timeline-artifact", ArtifactKind::Timeline, StageId::MediaTimeline, &timeline), artifact("runtime-captions-artifact", ArtifactKind::Captions, StageId::MediaTimeline, &captions)];
    let stage = |stage_id, ids: Vec<&str>| StageState { stage_id, status: StageStatus::Completed, attempt: 1, started_at: None, finished_at: None, input_artifact_ids: vec![], output_artifact_ids: ids.into_iter().map(str::to_string).collect(), service: None, error: None };
    let context = PipelineContext { job_id: "phase6-runtime".into(), project_id: None, workflow_mode: "storytelling".into(), content_source: Some(ContentSource::LocalMedia.as_str().to_string()), prompt: "runtime".into(), model_id: None, voice_id: None, language: "vi".into(), target_duration_seconds: 8, output_mode: "draft_only".into(), source_url: None, local_file: Some(source.to_string_lossy().to_string()), story_url: None, research_enabled: false, research_platform: None, research_query: None, research_mode: None, xhs_variant: None, artifact_refs: refs, stage_states: vec![stage(StageId::IngestAnalyze, vec!["runtime-source-video-artifact"]), stage(StageId::Voice, vec!["runtime-voice-audio-artifact"]), stage(StageId::MediaTimeline, vec!["runtime-timeline-artifact", "runtime-captions-artifact"])] };
    let input = prepare_capcut(&context).unwrap();
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).build().unwrap();
    let created = create_draft(&client, DEFAULT_WIDTH, DEFAULT_HEIGHT).await.unwrap();
    let draft_url = created.draft_url.clone();
    let draft_id = created.draft_id.clone();
    let source_url = register_artifact_asset(&client, &input.visual_artifact_ids[0], &source).await.unwrap();
    let voice_url = register_artifact_asset(&client, &input.voice_audio_artifact_id, &voice).await.unwrap();
    add_videos(&client, &draft_url, &source_url, &input.video_segments).await.unwrap();
    add_audio(&client, &draft_url, &voice_url, &input.voice_segment).await.unwrap();
    add_captions(&client, &draft_url, &input.captions).await.unwrap();
    let saved_url = save_draft(&client, &draft_url).await.unwrap();
    verify_draft_exists(&client, &draft_id).await.unwrap();
    let draft = inspect_draft(&client, &draft_id, &created.draft_path).await.unwrap();
    assert!(draft.visual_track_count.unwrap_or(0) > 0);
    assert!(draft.audio_track_count.unwrap_or(0) > 0);
    assert!(draft.caption_track_count.unwrap_or(0) > 0);

    let workflow_root = root.join("artifacts/phase6-runtime-rust");
    let capcut_dir = workflow_root.join("capcut");
    std::fs::create_dir_all(&capcut_dir).unwrap();
    let path = capcut_dir.join("draft_manifest.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&json!({"draftId": draft_id, "draftPath": saved_url, "visualTrackCount": draft.visual_track_count, "audioTrackCount": draft.audio_track_count, "captionTrackCount": draft.caption_track_count, "timelineDurationUs": input.duration_us, "source": draft.source})).unwrap()).unwrap();
    let stored = ArtifactStore::register_typed_artifact(&workflow_root, "phase6-runtime", StageId::Capcut, "capcut_mate", ArtifactKind::CapcutDraft, &path, json!({"input_artifact_ids": input.input_artifact_ids})).unwrap();
    assert_eq!(stored.to_artifact_ref(StageId::Capcut).unwrap().kind, ArtifactKind::CapcutDraft);
  }
}
