//! Native CapCut draft assembly for Floword.
//!
//! This is the hidden EditingAdapter described by the product architecture:
//! canonical artifacts in, editable CapCut project (and optionally a rendered
//! video) out. It links the vendored CapCut Rust crates and never drives
//! the CapCut UI.

use crate::services::pipeline::capcut::CapcutInput;
use capcut_core::{Canvas, CropRect, InternalTimeline, TextStyle};
use capcut_ffmpeg::probe::probe;
use capcut_ffmpeg::render::{render_with_options, RenderOptions};
use capcut_validate::{lint_timeline, LintOptions, Severity};
use capcut_capcut::enums::Namespace;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DRAFT_CONTENT_TEMPLATE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/capcut/capcut/templates/_init/draft_content.json"));
const DRAFT_META_TEMPLATE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/capcut/capcut/templates/_init/draft_meta_info.json"));
const DRAFT_AGENCY_TEMPLATE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/capcut/capcut/templates/_init/draft_agency_config.json"));
const DRAFT_ATTACHMENT_TEMPLATE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/capcut/capcut/templates/_init/attachment_pc_common.json"));

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct EditingPreset {
  pub aspect: String,
  pub subtitle_style: String,
  pub transition_style: String,
  pub music_volume: f32,
  pub video_speed: f64,
}

impl Default for EditingPreset {
  fn default() -> Self {
    Self { aspect: "9:16".into(), subtitle_style: "dynamic".into(), transition_style: "smooth".into(), music_volume: 0.18, video_speed: 1.0 }
  }
}

impl EditingPreset {
  pub fn from_payload(payload: &Value) -> Self {
    let raw = payload.get("editing_preset").cloned().unwrap_or_else(|| json!({}));
    let mut preset = serde_json::from_value::<Self>(raw).unwrap_or_default();
    if !matches!(preset.aspect.as_str(), "9:16" | "16:9" | "1:1" | "4:5") {
      preset.aspect = "9:16".into();
    }
    if !preset.video_speed.is_finite() || !(0.25..=4.0).contains(&preset.video_speed) {
      preset.video_speed = 1.0;
    }
    preset.music_volume = preset.music_volume.clamp(0.0, 1.0);
    preset
  }

  fn canvas(&self) -> Canvas {
    match self.aspect.as_str() {
      "16:9" => Canvas { width: 1920, height: 1080 },
      "1:1" => Canvas { width: 1080, height: 1080 },
      "4:5" => Canvas { width: 1080, height: 1350 },
      _ => Canvas { width: 1080, height: 1920 },
    }
  }

  fn text_style(&self) -> TextStyle {
    match self.subtitle_style.as_str() {
      "cinematic" => TextStyle { font: None, size: Some(58), color: Some("#F6D365".into()) },
      "clean" => TextStyle { font: None, size: Some(52), color: Some("#FFFFFF".into()) },
      _ => TextStyle { font: None, size: Some(64), color: Some("#FFFFFF".into()) },
    }
  }
}

#[derive(Debug)]
pub struct NativeDraftResult {
  pub draft_id: String,
  pub draft_path: PathBuf,
  pub desktop_root: PathBuf,
  pub timeline: InternalTimeline,
  pub visual_track_count: u64,
  pub audio_track_count: u64,
  pub caption_track_count: u64,
  pub lint_warnings: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeRenderResult {
  pub output_path: PathBuf,
  pub size_bytes: u64,
}

pub fn build_draft(input: &CapcutInput, preset: &EditingPreset, job_id: &str) -> anyhow::Result<NativeDraftResult> {
  let desktop_root = resolve_draft_root()?;
  let draft_id = uuid::Uuid::new_v4().to_string();
  let draft_name = format!("Floword-{}", job_id.chars().take(12).collect::<String>());
  let staging = desktop_root.join(format!(".{draft_id}.floword-building"));
  let final_path = desktop_root.join(&draft_id);
  if final_path.exists() || staging.exists() {
    anyhow::bail!("CAPCUT_DRAFT_CREATE_FAILED: generated draft destination already exists");
  }

  let build_result = (|| -> anyhow::Result<NativeDraftResult> {
    std::fs::create_dir_all(staging.join("media"))?;
    std::fs::write(staging.join("draft_content.json"), DRAFT_CONTENT_TEMPLATE)?;
    std::fs::write(staging.join("draft_info.json"), DRAFT_CONTENT_TEMPLATE)?;
    std::fs::write(staging.join("draft_meta_info.json"), DRAFT_META_TEMPLATE)?;
    std::fs::write(staging.join("draft_agency_config.json"), DRAFT_AGENCY_TEMPLATE)?;
    std::fs::write(staging.join("attachment_pc_common.json"), DRAFT_ATTACHMENT_TEMPLATE)?;

    let base = capcut_capcut::store::load_draft(&staging.join("draft_content.json"))?;
    let mut timeline = InternalTimeline::new(&draft_name, preset.canvas(), 30.0);
    timeline.id = draft_id.clone();
    let mut copied_media = HashMap::<String, PathBuf>::new();
    let mut visual_segment_ids = Vec::new();

    for placement in &input.video_segments {
      let local = materialize_media(&staging, &final_path, &placement.artifact_id, Path::new(&placement.path), &mut copied_media)?;
      let measured = probe(Path::new(&placement.path)).ok();
      let width = measured.as_ref().map(|item| item.width).filter(|value| *value > 0).unwrap_or(timeline.canvas.width);
      let height = measured.as_ref().map(|item| item.height).filter(|value| *value > 0).unwrap_or(timeline.canvas.height);
      let start = checked_us(placement.start_us)?;
      let duration = checked_us(placement.duration_us)?;
      let (segment_id, _) = timeline.add_video(local.to_string_lossy().to_string(), start, duration, width, height);
      visual_segment_ids.push(segment_id.clone());
      if let Some((track, index)) = timeline.find_segment_mut(&segment_id) {
        if let Some(source) = track.segments[index].source_timerange.as_mut() {
          source.start = checked_us(placement.source_start_us)?;
        }
      }
      timeline.set_volume(&segment_id, 0.0)?;
      if (preset.video_speed - 1.0).abs() > f64::EPSILON {
        timeline.set_speed(&segment_id, preset.video_speed)?;
      }
      timeline.set_crop(&segment_id, CropRect::for_ratio(width, height, &preset.aspect))?;
    }

    let voice_path = materialize_media(&staging, &final_path, &input.voice_audio_artifact_id, Path::new(&input.voice_segment.path), &mut copied_media)?;
    let (voice_segment_id, _) = timeline.add_audio(voice_path.to_string_lossy().to_string(), checked_us(input.voice_segment.start_us)?, checked_us(input.voice_segment.duration_us)?);
    if let Some((track, index)) = timeline.find_segment_mut(&voice_segment_id) {
      if let Some(source) = track.segments[index].source_timerange.as_mut() {
        source.start = checked_us(input.voice_segment.source_start_us)?;
      }
    }
    timeline.set_volume(&voice_segment_id, 1.0)?;

    let text_style = preset.text_style();
    for caption in &input.captions {
      if caption.end <= caption.start || caption.text.trim().is_empty() {
        continue;
      }
      timeline.add_text(caption.text.trim(), checked_us(caption.start)?, checked_us(caption.end - caption.start)?, text_style.clone());
    }
    timeline.recalc_duration();
    timeline.sort_tracks();

    // Paths in the published draft point at the final directory after the
    // staging directory is atomically renamed.
    let lint_options = LintOptions { check_local_paths: false, draft_dir: Some(final_path.to_string_lossy().to_string()), ..LintOptions::default() };
    let issues = lint_timeline(&timeline, &lint_options);
    let errors = issues.iter().filter(|issue| issue.severity == Severity::Error).map(|issue| issue.message.as_str()).collect::<Vec<_>>();
    if !errors.is_empty() {
      anyhow::bail!("CAPCUT_DRAFT_LINT_FAILED: {}", errors.join("; "));
    }
    let lint_warnings = issues.iter().filter(|issue| issue.severity == Severity::Warning).count();
    let mut final_draft = capcut_capcut::writer::into_draft(&timeline, base.clone());
    if let Some(slug) = transition_slug(&preset.transition_style) {
      for segment_id in visual_segment_ids.iter().take(visual_segment_ids.len().saturating_sub(1)) {
        capcut_capcut::decorators::add_transition(&mut final_draft, segment_id, slug, Some(400_000), Namespace::CapCut).map_err(|error| anyhow::anyhow!("CAPCUT_TRANSITION_FAILED: {error}"))?;
      }
    }
    capcut_capcut::store::save_draft_pretty(&staging.join("draft_content.json"), &final_draft)?;
    capcut_capcut::store::save_draft_pretty(&staging.join("draft_info.json"), &final_draft)?;
    update_meta(&staging, &final_path, &desktop_root, &draft_id, &draft_name, timeline.duration)?;

    std::fs::rename(&staging, &final_path)?;
    update_root_index(&desktop_root, &final_path.join("draft_meta_info.json"))?;

    Ok(NativeDraftResult { draft_id, draft_path: final_path, desktop_root, visual_track_count: timeline.tracks.iter().filter(|track| track.kind == capcut_core::TrackKind::Video && !track.segments.is_empty()).count() as u64, audio_track_count: timeline.tracks.iter().filter(|track| track.kind == capcut_core::TrackKind::Audio && !track.segments.is_empty()).count() as u64, caption_track_count: timeline.tracks.iter().filter(|track| track.kind == capcut_core::TrackKind::Text && !track.segments.is_empty()).count() as u64, timeline, lint_warnings })
  })();

  if build_result.is_err() && staging.is_dir() {
    let _ = std::fs::remove_dir_all(&staging);
  }
  build_result
}

pub fn render_video(result: &NativeDraftResult, output_path: &Path) -> anyhow::Result<NativeRenderResult> {
  if let Some(parent) = output_path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let inputs = result.timeline.materials.videos.iter().map(|material| PathBuf::from(&material.path)).chain(result.timeline.materials.audios.iter().map(|material| PathBuf::from(&material.path))).collect::<Vec<_>>();
  let options = RenderOptions { burn_captions: true, scale: Some(1.0), ..RenderOptions::default() };
  render_with_options(&result.timeline, &inputs, output_path, &options).map_err(|error| anyhow::anyhow!("CAPCUT_RENDER_FAILED: {error}"))?;
  let size_bytes = output_path.metadata()?.len();
  if size_bytes == 0 {
    anyhow::bail!("CAPCUT_RENDER_FAILED: rendered video is empty");
  }
  Ok(NativeRenderResult { output_path: output_path.to_path_buf(), size_bytes })
}

fn checked_us(value: u64) -> anyhow::Result<i64> {
  i64::try_from(value).map_err(|_| anyhow::anyhow!("CAPCUT_TIMELINE_INVALID: timestamp exceeds i64 microseconds"))
}

fn transition_slug(style: &str) -> Option<&'static str> {
  match style {
    "none" => None,
    "flash" => Some("white-flash"),
    "fade" => Some("black-fade"),
    _ => Some("dissolve"),
  }
}

fn resolve_draft_root() -> anyhow::Result<PathBuf> {
  if let Ok(configured) = std::env::var("CAPCUT_DESKTOP_DRAFT_ROOT") {
    let path = PathBuf::from(configured.trim());
    if path.is_dir() {
      return path.canonicalize().map_err(Into::into);
    }
  }
  let root = capcut_capcut::store::default_drafts_dir().ok_or_else(|| anyhow::anyhow!("CAPCUT_DESKTOP_ROOT_NOT_FOUND: no CapCut/JianYing draft store is available"))?;
  if !root.is_dir() {
    anyhow::bail!("CAPCUT_DESKTOP_ROOT_NOT_FOUND: {} does not exist", root.display());
  }
  root.canonicalize().map_err(Into::into)
}

fn materialize_media(staging: &Path, final_path: &Path, artifact_id: &str, source: &Path, cache: &mut HashMap<String, PathBuf>) -> anyhow::Result<PathBuf> {
  if let Some(path) = cache.get(artifact_id) {
    return Ok(path.clone());
  }
  if !source.is_file() || source.metadata()?.len() == 0 {
    anyhow::bail!("CAPCUT_MEDIA_REFERENCE_INVALID: {} is missing or empty", source.display());
  }
  let ext = source.extension().and_then(|value| value.to_str()).filter(|value| !value.is_empty()).unwrap_or("bin");
  let safe_id = artifact_id.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_').take(80).collect::<String>();
  let published_name = format!("{safe_id}.{ext}");
  let destination = staging.join("media").join(&published_name);
  std::fs::copy(source, &destination)?;
  let published_path = final_path.join("media").join(published_name);
  cache.insert(artifact_id.to_string(), published_path.clone());
  Ok(published_path)
}

fn update_meta(staging: &Path, final_path: &Path, root: &Path, draft_id: &str, name: &str, duration: i64) -> anyhow::Result<()> {
  let path = staging.join("draft_meta_info.json");
  let mut value: Value = serde_json::from_str(DRAFT_META_TEMPLATE)?;
  let object = value.as_object_mut().ok_or_else(|| anyhow::anyhow!("CAPCUT_DRAFT_CREATE_FAILED: meta template is not an object"))?;
  let now = chrono::Utc::now().timestamp_micros();
  object.insert("draft_id".into(), json!(draft_id));
  object.insert("draft_name".into(), json!(name));
  object.insert("draft_fold_path".into(), json!(final_path));
  object.insert("draft_json_file".into(), json!(final_path.join("draft_content.json")));
  object.insert("draft_root_path".into(), json!(root));
  object.insert("draft_cover".into(), json!(""));
  object.insert("draft_is_invisible".into(), json!(false));
  object.insert("streaming_edit_draft_ready".into(), json!(true));
  object.insert("tm_duration".into(), json!(duration));
  object.insert("tm_draft_create".into(), json!(now));
  object.insert("tm_draft_modified".into(), json!(now));
  capcut_capcut::draft::write_atomic(&path, &serde_json::to_string_pretty(&value)?)?;
  Ok(())
}

fn update_root_index(root: &Path, meta_path: &Path) -> anyhow::Result<()> {
  let meta: Value = serde_json::from_slice(&std::fs::read(meta_path)?)?;
  let index_path = root.join("root_meta_info.json");
  let mut index: Value = if index_path.is_file() { serde_json::from_slice(&std::fs::read(&index_path)?)? } else { json!({"all_draft_store": [], "draft_ids": 0, "root_path": root}) };
  let object = index.as_object_mut().ok_or_else(|| anyhow::anyhow!("CAPCUT_DRAFT_INDEX_INVALID: root_meta_info.json is not an object"))?;
  let entries = object.entry("all_draft_store").or_insert_with(|| json!([])).as_array_mut().ok_or_else(|| anyhow::anyhow!("CAPCUT_DRAFT_INDEX_INVALID: all_draft_store is not an array"))?;
  let draft_id = meta.get("draft_id").and_then(Value::as_str).unwrap_or_default();
  entries.retain(|entry| entry.get("draft_id").and_then(Value::as_str) != Some(draft_id));
  entries.push(meta);
  let count = entries.len();
  object.insert("draft_ids".into(), json!(count));
  object.insert("root_path".into(), json!(root));
  capcut_capcut::draft::write_atomic(&index_path, &serde_json::to_string_pretty(&index)?)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn invalid_preset_values_fall_back_safely() {
    let preset = EditingPreset::from_payload(&json!({"editing_preset":{"aspect":"bad","video_speed":99,"music_volume":4}}));
    assert_eq!(preset.aspect, "9:16");
    assert_eq!(preset.video_speed, 1.0);
    assert_eq!(preset.music_volume, 1.0);
  }

  #[test]
  fn preset_selects_expected_canvas() {
    assert_eq!(EditingPreset::default().canvas(), Canvas { width: 1080, height: 1920 });
  }

  #[test]
  fn transition_presets_map_to_capcut_resources() {
    assert_eq!(transition_slug("smooth"), Some("dissolve"));
    assert_eq!(transition_slug("fade"), Some("black-fade"));
    assert_eq!(transition_slug("flash"), Some("white-flash"));
    assert_eq!(transition_slug("none"), None);
  }
}
