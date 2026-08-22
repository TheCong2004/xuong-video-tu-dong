//! Phase 7 lifecycle hardening for the canonical seven business stages.

use crate::services::pipeline::contracts::{ArtifactKind, PipelineContext, PipelineContractError, StageId, StageState, StageStatus};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StagePolicy {
  pub max_attempts: u32,
  pub timeout_seconds: u64,
}

pub const fn stage_policy(stage: StageId) -> StagePolicy {
  match stage {
    StageId::Input => StagePolicy { max_attempts: 1, timeout_seconds: 5 },
    StageId::IngestAnalyze => StagePolicy { max_attempts: 2, timeout_seconds: 300 },
    StageId::Research => StagePolicy { max_attempts: 2, timeout_seconds: 460 },
    StageId::StoryScript => StagePolicy { max_attempts: 2, timeout_seconds: 180 },
    StageId::Voice => StagePolicy { max_attempts: 2, timeout_seconds: 180 },
    StageId::MediaTimeline => StagePolicy { max_attempts: 2, timeout_seconds: 120 },
    StageId::Capcut => StagePolicy { max_attempts: 2, timeout_seconds: 180 },
  }
}

pub fn should_retry_stage(stage: StageId, code: &str, attempt: u32, cancelled: bool) -> bool {
  if cancelled || attempt >= stage_policy(stage).max_attempts {
    return false;
  }
  matches!(code, "YOUWEE_DOWNLOAD_FAILED" | "VYNARO_UNAVAILABLE" | "MEDIACRAWLER_UNAVAILABLE" | "MEDIACRAWLER_TIMEOUT" | "STORY_STUDIO_UNAVAILABLE" | "STORY_STUDIO_TIMEOUT" | "LLM_UNAVAILABLE" | "LLM_TIMEOUT" | "LLM_RATE_LIMITED" | "VOICE_TTS_UNAVAILABLE" | "VOICE_TTS_TIMEOUT" | "OPENMONTAGE_UNAVAILABLE" | "OPENMONTAGE_TIMEOUT" | "CAPCUT_UNAVAILABLE" | "CAPCUT_ASSET_TRANSPORT_FAILED" | "CAPCUT_TIMEOUT" | "DRAFT_CREATE_FAILED" | "DRAFT_SAVE_FAILED")
}

pub fn prepare_resume(context: &mut PipelineContext) -> Result<Option<StageId>, PipelineContractError> {
  let first_incomplete = StageId::ALL.into_iter().find(|stage| !stage_is_reusable(context, *stage));
  if let Some(stage) = first_incomplete {
    invalidate_from(context, stage);
  }
  Ok(first_incomplete)
}

pub fn stage_needs_run(context: &PipelineContext, stage: StageId) -> bool {
  context.stage_states.iter().find(|state| state.stage_id == stage).is_none_or(|state| !matches!(state.status, StageStatus::Completed | StageStatus::Skipped))
}

pub fn invalidate_output_values(outputs: &mut Value, from: StageId) {
  let Some(object) = outputs.as_object_mut() else {
    return;
  };
  let keys = object.keys().filter(|key| output_stage(key).is_some_and(|stage| stage_index(stage) >= stage_index(from))).cloned().collect::<Vec<_>>();
  for key in keys {
    object.remove(&key);
  }
}

pub fn invalidate_from(context: &mut PipelineContext, from: StageId) {
  let from_index = stage_index(from);
  context.artifact_refs.retain(|artifact| stage_index(artifact.produced_by_stage) < from_index);
  for state in &mut context.stage_states {
    if stage_index(state.stage_id) >= from_index {
      let attempts = state.attempt;
      let mut pending = StageState::pending(state.stage_id);
      pending.attempt = attempts;
      *state = pending;
    }
  }
}

pub fn cancel_stage_and_block_downstream(context: &mut PipelineContext, stage: StageId, finished_at: &str) -> Result<(), PipelineContractError> {
  let state = context.stage_states.iter_mut().find(|state| state.stage_id == stage).ok_or(PipelineContractError::MissingStage(stage))?;
  state.cancel_stage(finished_at)?;
  let current_index = stage_index(stage);
  for downstream in &mut context.stage_states {
    if stage_index(downstream.stage_id) > current_index {
      *downstream = StageState::pending(downstream.stage_id);
    }
  }
  Ok(())
}

fn stage_is_reusable(context: &PipelineContext, stage: StageId) -> bool {
  let Some(state) = context.stage_states.iter().find(|state| state.stage_id == stage) else {
    return false;
  };
  if stage == StageId::Research && state.status == StageStatus::Skipped {
    return !context.research_enabled;
  }
  if stage == StageId::IngestAnalyze && state.status == StageStatus::Skipped && matches!(context.workflow_mode.as_str(), "original" | "original_creation") {
    return context.source_url.is_none() && context.local_file.is_none();
  }
  if state.status != StageStatus::Completed {
    return false;
  }
  let required = |kind: &ArtifactKind| state.output_artifact_ids.iter().any(|id| context.artifact_refs.iter().find(|artifact| artifact.artifact_id == *id && artifact.kind == *kind && artifact.produced_by_stage == stage).is_some_and(|artifact| artifact.validate().is_ok()));
  let generated_visual_is_valid = context.artifact_refs.iter().any(|artifact| artifact.kind == ArtifactKind::GeneratedVideo && artifact.validate().is_ok());
  expected_kinds(stage).iter().all(required) && (stage != StageId::MediaTimeline || !matches!(context.workflow_mode.as_str(), "original" | "original_creation") || generated_visual_is_valid) && (stage != StageId::Capcut || context.output_mode != "render_video" || required(&ArtifactKind::RenderedVideo))
}

const fn expected_kinds(stage: StageId) -> &'static [ArtifactKind] {
  match stage {
    StageId::Input => &[],
    StageId::IngestAnalyze => &[ArtifactKind::SourceVideo, ArtifactKind::SourceMetadata, ArtifactKind::Scenes, ArtifactKind::SourceAudio],
    StageId::Research => &[ArtifactKind::Research],
    StageId::StoryScript => &[ArtifactKind::Story, ArtifactKind::ScriptRequest, ArtifactKind::Script],
    StageId::Voice => &[ArtifactKind::VoiceAudio, ArtifactKind::VoiceTiming],
    StageId::MediaTimeline => &[ArtifactKind::Timeline, ArtifactKind::Captions],
    StageId::Capcut => &[ArtifactKind::CapcutDraft],
  }
}

const fn stage_index(stage: StageId) -> usize {
  match stage {
    StageId::Input => 0,
    StageId::IngestAnalyze => 1,
    StageId::Research => 2,
    StageId::StoryScript => 3,
    StageId::Voice => 4,
    StageId::MediaTimeline => 5,
    StageId::Capcut => 6,
  }
}

fn output_stage(key: &str) -> Option<StageId> {
  match key {
    "ingest_analyze" => Some(StageId::IngestAnalyze),
    "research" => Some(StageId::Research),
    "story" | "script_request" | "script" | "story_script" | "script_artifact" => Some(StageId::StoryScript),
    "voice" => Some(StageId::Voice),
    "media_assets" | "media_timeline" => Some(StageId::MediaTimeline),
    "draft_url" | "draft_id" | "draft_manifest" | "capcut_artifact" | "capcut" | "video_url" | "rendering_supported" | "render_error" => Some(StageId::Capcut),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::contracts::{ArtifactRef, ContentSource, StageState, StageStatus};
  use serde_json::json;

  fn artifact(root: &std::path::Path, stage: StageId, kind: ArtifactKind) -> ArtifactRef {
    let path = root.join(format!("{}.json", kind.as_str()));
    std::fs::write(&path, b"{}").unwrap();
    ArtifactRef { artifact_id: format!("id-{}", kind.as_str()), kind, produced_by_stage: stage, location: path.to_string_lossy().to_string(), mime_type: None, metadata: json!({}) }
  }

  fn completed(stage_id: StageId, artifacts: &[ArtifactRef]) -> StageState {
    StageState { stage_id, status: StageStatus::Completed, attempt: 1, started_at: Some("start".into()), finished_at: Some("finish".into()), input_artifact_ids: vec![], output_artifact_ids: artifacts.iter().filter(|item| item.produced_by_stage == stage_id).map(|item| item.artifact_id.clone()).collect(), service: Some("runtime".into()), error: None }
  }

  fn canonical_context(root: &std::path::Path) -> PipelineContext {
    let mut artifacts = Vec::new();
    for (stage, kinds) in [(StageId::IngestAnalyze, vec![ArtifactKind::SourceVideo, ArtifactKind::SourceMetadata, ArtifactKind::Scenes, ArtifactKind::SourceAudio]), (StageId::StoryScript, vec![ArtifactKind::Story, ArtifactKind::ScriptRequest, ArtifactKind::Script]), (StageId::Voice, vec![ArtifactKind::VoiceAudio, ArtifactKind::VoiceTiming]), (StageId::MediaTimeline, vec![ArtifactKind::Timeline, ArtifactKind::Captions]), (StageId::Capcut, vec![ArtifactKind::CapcutDraft])] {
      artifacts.extend(kinds.into_iter().map(|kind| artifact(root, stage, kind)));
    }
    let mut states = StageId::ALL.into_iter().map(|stage| completed(stage, &artifacts)).collect::<Vec<_>>();
    let research = states.iter_mut().find(|state| state.stage_id == StageId::Research).unwrap();
    *research = StageState::pending(StageId::Research);
    research.skip_stage("finish").unwrap();
    PipelineContext { job_id: "resume-job".into(), project_id: None, workflow_mode: "source_based".into(), content_source: Some(ContentSource::LocalMedia.as_str().to_string()), prompt: "Kể chuyện".into(), model_id: None, voice_id: None, language: "vi".into(), target_duration_seconds: 30, output_mode: "draft_only".into(), source_url: None, local_file: Some("fixture.mp4".into()), story_url: None, research_enabled: false, research_platform: None, research_query: None, research_mode: None, xhs_variant: None, artifact_refs: artifacts, stage_states: states }
  }

  #[test]
  fn missing_downstream_artifact_resumes_exact_stage_and_preserves_upstream() {
    let temp = tempfile::tempdir().unwrap();
    let mut context = canonical_context(temp.path());
    let voice = context.artifact_refs.iter().find(|item| item.kind == ArtifactKind::VoiceAudio).unwrap().clone();
    std::fs::remove_file(&voice.location).unwrap();

    assert_eq!(prepare_resume(&mut context).unwrap(), Some(StageId::Voice));
    assert_eq!(context.stage_states.iter().find(|state| state.stage_id == StageId::StoryScript).unwrap().status, StageStatus::Completed);
    assert_eq!(context.stage_states.iter().find(|state| state.stage_id == StageId::Voice).unwrap().status, StageStatus::Pending);
    assert!(!stage_needs_run(&context, StageId::IngestAnalyze));
    assert!(!stage_needs_run(&context, StageId::StoryScript));
    assert!(stage_needs_run(&context, StageId::Voice));
    assert!(context.artifact_refs.iter().any(|item| item.kind == ArtifactKind::Script));
    assert!(!context.artifact_refs.iter().any(|item| item.produced_by_stage == StageId::Voice));
    assert!(!context.artifact_refs.iter().any(|item| item.produced_by_stage == StageId::MediaTimeline));
  }

  #[test]
  fn complete_valid_context_is_idempotently_reusable_with_research_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let mut context = canonical_context(temp.path());
    assert_eq!(prepare_resume(&mut context).unwrap(), None);
    assert_eq!(context.stage_states.iter().find(|state| state.stage_id == StageId::Research).unwrap().status, StageStatus::Skipped);
  }

  #[test]
  fn structured_transient_failure_retries_with_small_stage_limit() {
    assert!(should_retry_stage(StageId::Voice, "VOICE_TTS_TIMEOUT", 1, false));
    assert!(!should_retry_stage(StageId::Voice, "VOICE_TTS_TIMEOUT", 2, false));
    assert!(!should_retry_stage(StageId::Voice, "VOICE_AUTH_REQUIRED", 1, false));
    assert!(!should_retry_stage(StageId::Voice, "VOICE_TTS_TIMEOUT", 1, true));
    assert!(StageId::ALL.into_iter().all(|stage| stage_policy(stage).timeout_seconds > 0));
    assert!(StageId::ALL.into_iter().all(|stage| stage_policy(stage).max_attempts <= 2));
  }

  #[test]
  fn research_stage_timeout_covers_login_and_crawl_defaults() {
    assert!(stage_policy(StageId::Research).timeout_seconds >= 450);
  }

  #[test]
  fn cancellation_marks_current_and_keeps_downstream_unstarted() {
    let temp = tempfile::tempdir().unwrap();
    let mut context = canonical_context(temp.path());
    invalidate_from(&mut context, StageId::Voice);
    context.stage_states.iter_mut().find(|state| state.stage_id == StageId::Voice).unwrap().start_stage(Some("tts".into()), "start").unwrap();
    cancel_stage_and_block_downstream(&mut context, StageId::Voice, "finish").unwrap();
    assert_eq!(context.stage_states.iter().find(|state| state.stage_id == StageId::Voice).unwrap().status, StageStatus::Cancelled);
    assert!(context.stage_states.iter().filter(|state| matches!(state.stage_id, StageId::MediaTimeline | StageId::Capcut)).all(|state| state.status == StageStatus::Pending));
  }

  #[test]
  fn resume_invalidation_removes_only_downstream_output_values() {
    let mut outputs = json!({"ingest_analyze": {}, "story_script": {}, "voice": {}, "media_timeline": {}, "capcut": {}, "retry_counts": {}});
    invalidate_output_values(&mut outputs, StageId::Voice);
    assert!(outputs.get("ingest_analyze").is_some());
    assert!(outputs.get("story_script").is_some());
    assert!(outputs.get("voice").is_none());
    assert!(outputs.get("media_timeline").is_none());
    assert!(outputs.get("capcut").is_none());
    assert!(outputs.get("retry_counts").is_some());
  }

  #[tokio::test]
  #[ignore = "requires real local FFmpeg, unified backend, OmniRoute/TTS, OpenMontage, and CapCut Mate runtimes"]
  async fn canonical_local_e2e_reaches_verified_draft_ready() {
    use crate::services::pipeline::artifact_store::ArtifactStore;
    use crate::services::pipeline::capcut::prepare_capcut;
    use crate::services::pipeline::clients::capcut_mate_client::{add_audio, add_captions, add_videos, create_draft, inspect_draft, register_artifact_asset, save_draft, verify_draft_exists, DEFAULT_HEIGHT, DEFAULT_WIDTH};
    use crate::services::pipeline::clients::omniroute_client::execute_story_script_request;
    use crate::services::pipeline::ingest_analyze::ingest_local_source;
    use crate::services::pipeline::media_timeline::{prepare_media_timeline, run_openmontage};
    use crate::services::pipeline::research::{prepare_research, ResearchPreparation};
    use crate::services::pipeline::story_script::{prepare_story_script, run_story_studio};
    use crate::services::pipeline::voice::{prepare_voice, synthesize_voice};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().to_path_buf();
    let work = root.join("artifacts/phase7-e2e-local");
    std::fs::create_dir_all(&work).unwrap();
    let fixture = root.join("test_data/video/mp4/golden_sun_garoh.mp4");
    let cancel = Arc::new(AtomicBool::new(false));
    let mut context = PipelineContext { job_id: "phase7-e2e-local".into(), project_id: None, workflow_mode: "source_based".into(), content_source: Some(ContentSource::LocalMedia.as_str().to_string()), prompt: "Kể lại video bằng tiếng Việt, ngắn gọn và hấp dẫn.".into(), model_id: None, voice_id: None, language: "vi".into(), target_duration_seconds: 30, output_mode: "draft_only".into(), source_url: None, local_file: Some(fixture.to_string_lossy().to_string()), story_url: None, research_enabled: false, research_platform: None, research_query: None, research_mode: None, xhs_variant: None, artifact_refs: vec![], stage_states: PipelineContext::initial_stage_states() };
    complete(&mut context, StageId::Input, vec![]);

    let ingest = ingest_local_source(&fixture, &work, &context.job_id, Arc::clone(&cancel)).await.unwrap();
    let ingest_ids = ingest.artifact_refs.iter().map(|item| item.artifact_id.clone()).collect();
    context.artifact_refs.extend(ingest.artifact_refs);
    complete(&mut context, StageId::IngestAnalyze, ingest_ids);
    assert_eq!(prepare_research(&mut context, "research-skip").unwrap(), ResearchPreparation::Skipped);

    let story_input = prepare_story_script(&context).unwrap();
    let studio = run_story_studio(&story_input, Arc::clone(&cancel)).await.unwrap();
    let script = execute_story_script_request(&studio.script_request, None).await.unwrap();
    let story_dir = work.join("story_script");
    std::fs::create_dir_all(&story_dir).unwrap();
    let story_items = [(ArtifactKind::Story, story_dir.join("story.json"), serde_json::to_value(&studio.story).unwrap(), "story_studio"), (ArtifactKind::ScriptRequest, story_dir.join("script_request.json"), serde_json::to_value(&studio.script_request).unwrap(), "story_studio"), (ArtifactKind::Script, story_dir.join("script.json"), serde_json::to_value(&script).unwrap(), "omniroute")];
    let mut story_ids = Vec::new();
    for (kind, path, value, service) in story_items {
      std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
      let stored = ArtifactStore::register_typed_artifact(&work, &context.job_id, StageId::StoryScript, service, kind, &path, json!({"input_artifact_ids": &story_input.input_artifact_ids})).unwrap();
      let reference = stored.to_artifact_ref(StageId::StoryScript).unwrap();
      story_ids.push(reference.artifact_id.clone());
      context.artifact_refs.push(reference);
    }
    complete(&mut context, StageId::StoryScript, story_ids);

    let voice_input = prepare_voice(&context).unwrap();
    let voice = synthesize_voice(&voice_input, &work, Arc::clone(&cancel)).await.unwrap();
    let mut voice_ids = Vec::new();
    for (kind, path) in [(ArtifactKind::VoiceAudio, voice.audio_path.as_path()), (ArtifactKind::VoiceTiming, voice.timing_path.as_path())] {
      let stored = ArtifactStore::register_typed_artifact(&work, &context.job_id, StageId::Voice, "omniroute_tts_vynaro", kind, path, json!({"script_artifact_id": &voice_input.script_artifact_id})).unwrap();
      let reference = stored.to_artifact_ref(StageId::Voice).unwrap();
      voice_ids.push(reference.artifact_id.clone());
      context.artifact_refs.push(reference);
    }
    complete(&mut context, StageId::Voice, voice_ids);

    let montage_input = prepare_media_timeline(&context, &work).unwrap();
    let montage = run_openmontage(&montage_input, Arc::clone(&cancel)).await.unwrap();
    let mut montage_ids = Vec::new();
    for (kind, path) in [(ArtifactKind::Timeline, montage.timeline_path.as_path()), (ArtifactKind::Captions, montage.captions_path.as_path())] {
      let stored = ArtifactStore::register_typed_artifact(&work, &context.job_id, StageId::MediaTimeline, "openmontage", kind, path, json!({"input_artifact_ids": &montage_input.input_artifact_ids})).unwrap();
      let reference = stored.to_artifact_ref(StageId::MediaTimeline).unwrap();
      montage_ids.push(reference.artifact_id.clone());
      context.artifact_refs.push(reference);
    }
    complete(&mut context, StageId::MediaTimeline, montage_ids);

    let capcut = prepare_capcut(&context).unwrap();
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).build().unwrap();
    let created = create_draft(&client, DEFAULT_WIDTH, DEFAULT_HEIGHT).await.unwrap();
    let draft_url = created.draft_url.clone();
    let draft_id = created.draft_id.clone();
    let source_url = register_artifact_asset(&client, &capcut.visual_artifact_ids[0], std::path::Path::new(&capcut.video_segments[0].path)).await.unwrap();
    let voice_url = register_artifact_asset(&client, &capcut.voice_audio_artifact_id, std::path::Path::new(&capcut.voice_segment.path)).await.unwrap();
    add_videos(&client, &draft_url, &source_url, &capcut.video_segments).await.unwrap();
    add_audio(&client, &draft_url, &voice_url, &capcut.voice_segment).await.unwrap();
    add_captions(&client, &draft_url, &capcut.captions).await.unwrap();
    let saved_url = save_draft(&client, &draft_url).await.unwrap();
    verify_draft_exists(&client, &draft_id).await.unwrap();
    let inspected = inspect_draft(&client, &draft_id, &created.draft_path).await.unwrap();
    assert!(inspected.visual_track_count.unwrap_or(0) > 0 && inspected.audio_track_count.unwrap_or(0) > 0 && inspected.caption_track_count.unwrap_or(0) > 0);
    let capcut_dir = work.join("capcut");
    std::fs::create_dir_all(&capcut_dir).unwrap();
    let manifest_path = capcut_dir.join("draft_manifest.json");
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&json!({"draftId": draft_id, "draftPath": saved_url, "visualTrackCount": inspected.visual_track_count, "audioTrackCount": inspected.audio_track_count, "captionTrackCount": inspected.caption_track_count, "timelineDurationUs": capcut.duration_us, "source": inspected.source})).unwrap()).unwrap();
    let stored = ArtifactStore::register_typed_artifact(&work, &context.job_id, StageId::Capcut, "capcut_mate", ArtifactKind::CapcutDraft, &manifest_path, json!({"input_artifact_ids": &capcut.input_artifact_ids})).unwrap();
    let reference = stored.to_artifact_ref(StageId::Capcut).unwrap();
    complete(&mut context, StageId::Capcut, vec![reference.artifact_id.clone()]);
    context.artifact_refs.push(reference);

    assert_eq!(prepare_resume(&mut context).unwrap(), None);
    let kinds = context.artifact_refs.iter().map(|item| item.kind.as_str()).collect::<std::collections::HashSet<_>>();
    for expected in ["source_video", "source_metadata", "scenes", "source_audio", "story", "script_request", "script", "voice_audio", "voice_timing", "captions", "timeline", "capcut_draft"] {
      assert!(kinds.contains(expected), "missing canonical artifact {expected}");
    }
    let report = json!({"final": "draft_ready", "research": "skipped", "draftId": draft_id, "stageStates": context.stage_states, "artifacts": context.artifact_refs});
    std::fs::write(work.join("e2e_report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
  }

  fn complete(context: &mut PipelineContext, stage: StageId, ids: Vec<String>) {
    let state = context.stage_states.iter_mut().find(|item| item.stage_id == stage).unwrap();
    state.start_stage(Some("e2e_runtime".into()), "start").unwrap();
    state.complete_stage(ids, "finish").unwrap();
  }
}
