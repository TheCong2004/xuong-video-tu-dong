//! Native, local CapCut automation primitives.
//!
//! This module deliberately lives in the existing ArtCraft pipeline.  It does
//! not start a second application or HTTP service: media is rendered by the
//! packaged FFmpeg binary, artifacts are written to the per-job directory and
//! published through the existing `OutputPathResolver`/`ArtifactStore`.

use md5::{Digest as Md5Digest, Md5};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

const MAX_TEXT: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapcutAutomationPresetV1 {
  pub schema_version: u32,
  /// Stable one-click preset identifier.  `None` is accepted for legacy
  /// callers and is migrated to the built-in quick preset at the boundary.
  #[serde(default)]
  pub preset_id: Option<String>,
  pub name: String,
  pub mirror_horizontal: bool,
  pub playback_rate: f64,
  pub preserve_audio_pitch: bool,
  pub color: VideoColorAdjustments,
  pub localization: LocalizationSettings,
  pub hook: HookOverlaySettings,
  pub foreign_text: ForeignTextTreatment,
  pub output: VideoOutputSettings,
  pub audio_policy: AudioRightsPolicy,
  #[serde(default)]
  pub publish_schedule: Option<PublishScheduleSettings>,
  /// Optional local-AI stages. They default to disabled for backwards
  /// compatibility; the built-in quick preset enables the complete workflow.
  #[serde(default)]
  pub auto_transcribe: bool,
  #[serde(default = "default_auto_source_language")]
  pub source_language: String,
  #[serde(default = "default_target_language")]
  pub target_language: String,
  #[serde(default)]
  pub auto_translate: bool,
  #[serde(default)]
  pub auto_ocr: bool,
  #[serde(default)]
  pub ocr_languages: Vec<String>,
  /// OCR engine contract. RapidOCR is multilingual and does not consume
  /// Tesseract language-pack codes such as `chi_tra`; keeping the engine in
  /// the request prevents the UI from sending a misleading language hint.
  #[serde(default = "default_ocr_engine")]
  pub ocr_engine: String,
  #[serde(default = "default_ocr_interval")]
  pub ocr_sample_interval_ms: u64,
  #[serde(default)]
  pub auto_diarize: bool,
  #[serde(default)]
  pub auto_tts: bool,
  #[serde(default = "default_tts_audio_mode")]
  pub tts_audio_mode: String,
  #[serde(default = "default_original_audio_gain")]
  pub original_audio_gain: f64,
  #[serde(default)]
  pub speaker_voice_assignments: std::collections::HashMap<String, String>,
}

fn default_auto_source_language() -> String { "auto".to_string() }
fn default_target_language() -> String { "vi".to_string() }
fn default_ocr_interval() -> u64 { 1_000 }
fn default_ocr_engine() -> String { "rapidocr-onnxruntime".to_string() }
fn default_tts_audio_mode() -> String { "MIX".to_string() }
fn default_original_audio_gain() -> f64 { 1.0 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoColorAdjustments {
  pub brightness: f64,
  pub contrast: f64,
  pub saturation: f64,
  pub gamma: f64,
  pub hue: f64,
  pub temperature: f64,
  pub highlights: f64,
  pub shadows: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalizationSettings {
  pub enabled: bool,
  pub source_language: String,
  pub target_language: String,
  pub transcription_engine: String,
  pub translate: bool,
  pub burn_subtitles: bool,
  pub subtitle_style: SubtitleStyle,
  #[serde(default)]
  pub subtitle_path: Option<String>,
  #[serde(default)]
  pub manual_cues: Vec<SubtitleCue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCue {
  pub start_ms: u64,
  pub end_ms: u64,
  pub text: String,
  #[serde(default = "default_true")]
  pub enabled: bool,
}

fn default_true() -> bool {
  true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleStyle {
  pub font_name: String,
  pub font_size: u32,
  pub margin_v: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOverlaySettings {
  pub enabled: bool,
  pub text: String,
  pub start_ms: u64,
  pub end_ms: u64,
  #[serde(default)]
  pub style: TextOverlayStyle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextOverlayStyle {
  pub font_name: String,
  pub font_size: u32,
  pub alignment: String,
  pub margin: u32,
  pub outline: u32,
}

impl Default for TextOverlayStyle {
  fn default() -> Self {
    Self { font_name: "Arial".to_string(), font_size: 48, alignment: "top-center".to_string(), margin: 80, outline: 2 }
  }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishScheduleSettings {
  pub timezone: String,
  pub slots: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignTextTreatment {
  pub enabled: bool,
  pub detection_mode: ForeignTextDetectionMode,
  pub action: ForeignTextAction,
  pub manual_regions: Vec<TimedRegion>,
  #[serde(default)]
  pub sticker_path: Option<String>,
  #[serde(default)]
  pub stickers: Vec<StickerOverlay>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickerOverlay {
  pub id: String,
  pub path: String,
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
  #[serde(default = "default_opacity")]
  pub opacity: f64,
  pub start_ms: u64,
  pub end_ms: u64,
  #[serde(default = "default_true")]
  pub enabled: bool,
}

fn default_opacity() -> f64 {
  1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForeignTextDetectionMode {
  Manual,
  Ocr,
  OcrWithManualReview,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForeignTextAction {
  Blur,
  Cover,
  Sticker,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AudioRightsPolicy {
  KeepIfRightsConfirmed,
  MuteOriginal,
  ReplaceWithLicensedAudio,
  ReplaceWithUserAudio,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimedRegion {
  pub start_ms: u64,
  pub end_ms: u64,
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
}

impl TimedRegion {
  pub fn clamped(&self) -> Self {
    let x = self.x.clamp(0.0, 1.0);
    let y = self.y.clamp(0.0, 1.0);
    let width = self.width.max(0.0).min(1.0 - x);
    let height = self.height.max(0.0).min(1.0 - y);
    Self { start_ms: self.start_ms, end_ms: self.end_ms, x, y, width, height }
  }
}

pub fn output_time_ms(source_ms: u64, playback_rate: f64) -> u64 {
  ((source_ms as f64) / playback_rate).round().max(0.0) as u64
}

/// Convert an FFmpeg progress timestamp into a bounded fraction.  FFmpeg's
/// `out_time_ms`/`out_time_us` values are elapsed timestamps, not percentages,
/// so a media duration is required for an honest progress value.
pub fn progress_fraction(out_time_ms: f64, duration_ms: Option<f64>) -> f64 {
  if !out_time_ms.is_finite() || out_time_ms < 0.0 {
    return 0.0;
  }
  duration_ms.filter(|duration| duration.is_finite() && *duration > 0.0).map(|duration| (out_time_ms / duration).clamp(0.0, 1.0)).unwrap_or(0.0)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoOutputSettings {
  pub container: String,
  pub video_codec: String,
  pub audio_codec: String,
  pub ratio: String,
  pub width: u32,
  pub height: u32,
  pub fps: String,
  pub scale_mode: String,
  pub quality_preset: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CapcutAutomationReceipt {
  pub schema_version: u32,
  pub job_id: String,
  pub request_id: String,
  pub input_sha256: String,
  pub output_sha256: String,
  pub output_md5: String,
  pub output_path: String,
  pub duration_ms: u64,
  pub width: u32,
  pub height: u32,
  pub video_codec: String,
  pub audio_codec: String,
  pub playback_rate: f64,
  pub subtitle_burned: bool,
  #[serde(default)]
  pub subtitle_mode: String,
  #[serde(default)]
  pub subtitle_cue_count: usize,
  pub hook_applied: bool,
  pub foreign_text_regions_applied: usize,
  pub terminal: bool,
  pub state: String,
  #[serde(default)]
  pub preset_id: Option<String>,
  #[serde(default)]
  pub created_at: Option<u64>,
  #[serde(default)]
  pub started_at: Option<u64>,
  #[serde(default)]
  pub completed_at: Option<u64>,
  #[serde(default)]
  pub input_path: Option<String>,
  #[serde(default)]
  pub input_duration_ms: Option<u64>,
  #[serde(default)]
  pub input_dimensions: Option<MediaDimensions>,
  #[serde(default)]
  pub output_dimensions: Option<MediaDimensions>,
  #[serde(default)]
  pub encoder: Option<String>,
  #[serde(default)]
  pub stage_timings: Option<StageTimings>,
  #[serde(default)]
  pub transcript_segment_count: usize,
  #[serde(default)]
  pub translated_segment_count: usize,
  #[serde(default)]
  pub ocr_region_count: usize,
  #[serde(default)]
  pub speaker_count: usize,
  #[serde(default)]
  pub speaker_assignment_count: usize,
  #[serde(default)]
  pub tts_segment_count: usize,
  #[serde(default)]
  pub cancel_count: u32,
  #[serde(default)]
  pub retry_count: u32,
  #[serde(default)]
  pub restored: bool,
  #[serde(default)]
  pub input_md5: Option<String>,
  #[serde(default)]
  pub preview_sha256: Option<String>,
  #[serde(default)]
  pub resource_versions: std::collections::HashMap<String, String>,
  #[serde(default)]
  pub warnings: Vec<String>,
  #[serde(default)]
  pub requested_source_language: Option<String>,
  #[serde(default)]
  pub effective_source_language: Option<String>,
  #[serde(default)]
  pub ocr_languages: Vec<String>,
  #[serde(default)]
  pub route_kind: Option<String>,
  #[serde(default)]
  pub translation_hops: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDimensions { pub width: u32, pub height: u32 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StageTimings {
  pub probe_ms: u64,
  pub transcription_ms: u64,
  pub translation_ms: u64,
  pub ocr_ms: u64,
  pub diarization_ms: u64,
  pub tts_ms: u64,
  pub subtitle_ms: u64,
  pub render_ms: u64,
  pub verify_ms: u64,
  pub hash_ms: u64,
  pub total_ms: u64,
}

impl Default for CapcutAutomationPresetV1 {
  fn default() -> Self {
    Self {
      schema_version: 1,
      preset_id: Some("QUICK_LOCALIZED_VERTICAL_V1".to_string()),
      name: "Vietnamese Short Video".to_string(),
      mirror_horizontal: true,
      playback_rate: 1.1,
      preserve_audio_pitch: true,
      color: VideoColorAdjustments { brightness: 5.0, contrast: 3.0, saturation: -5.0, gamma: 0.0, hue: 0.0, temperature: 0.0, highlights: 0.0, shadows: 0.0 },
      localization: LocalizationSettings { enabled: true, source_language: "auto".to_string(), target_language: "vi".to_string(), transcription_engine: "existing_or_local".to_string(), translate: true, burn_subtitles: true, subtitle_style: SubtitleStyle { font_name: "Arial".to_string(), font_size: 48, margin_v: 80 }, subtitle_path: None, manual_cues: Vec::new() },
      hook: HookOverlaySettings { enabled: true, text: "Món này ai hay ở một mình nhất định phải có".to_string(), start_ms: 0, end_ms: 3_000, style: TextOverlayStyle::default() },
      foreign_text: ForeignTextTreatment { enabled: true, detection_mode: ForeignTextDetectionMode::OcrWithManualReview, action: ForeignTextAction::Blur, manual_regions: Vec::new(), sticker_path: None, stickers: Vec::new() },
      output: VideoOutputSettings { container: "mp4".to_string(), video_codec: "h264".to_string(), audio_codec: "aac".to_string(), ratio: "9:16".to_string(), width: 1_080, height: 1_920, fps: "source_or_30".to_string(), scale_mode: "fill".to_string(), quality_preset: "balanced".to_string() },
      audio_policy: AudioRightsPolicy::KeepIfRightsConfirmed,
      publish_schedule: None,
      auto_transcribe: false,
      source_language: "auto".to_string(),
      target_language: "vi".to_string(),
      auto_translate: false,
      auto_ocr: false,
      // OCR language is selected explicitly by the workspace.  An empty
      // default avoids silently treating Chinese media as English.
      ocr_languages: Vec::new(),
      ocr_engine: default_ocr_engine(),
      ocr_sample_interval_ms: 1_000,
      auto_diarize: false,
      auto_tts: false,
      tts_audio_mode: "MIX".to_string(),
      original_audio_gain: 1.0,
      speaker_voice_assignments: std::collections::HashMap::new(),
    }
  }
}

impl CapcutAutomationPresetV1 {
  pub fn validate(&self) -> Result<(), String> {
    if self.schema_version != 1 {
      return Err("PRESET_SCHEMA_UNSUPPORTED".to_string());
    }
    if !self.playback_rate.is_finite() || !(0.5..=2.0).contains(&self.playback_rate) {
      return Err("PLAYBACK_RATE_OUT_OF_RANGE".to_string());
    }
    if let Some(preset_id) = self.preset_id.as_deref() {
      if preset_id != "QUICK_LOCALIZED_VERTICAL_V1" || preset_id.len() > 64 {
        return Err("PRESET_ID_UNSUPPORTED".to_string());
      }
    }
    for value in [self.color.brightness, self.color.contrast, self.color.saturation, self.color.gamma, self.color.hue, self.color.temperature, self.color.highlights, self.color.shadows] {
      if !value.is_finite() || !(-100.0..=100.0).contains(&value) {
        return Err("COLOR_VALUE_OUT_OF_RANGE".to_string());
      }
    }
    if self.output.width == 0 || self.output.height == 0 || self.output.width % 2 != 0 || self.output.height % 2 != 0 {
      return Err("OUTPUT_DIMENSIONS_INVALID".to_string());
    }
    if self.hook.enabled {
      if self.hook.end_ms <= self.hook.start_ms || self.hook.text.trim().is_empty() || self.hook.text.chars().count() > MAX_TEXT {
        return Err("HOOK_INVALID".to_string());
      }
    }
    for cue in &self.localization.manual_cues {
      if cue.end_ms <= cue.start_ms || cue.text.trim().is_empty() || cue.text.chars().count() > MAX_TEXT {
        return Err("SUBTITLE_CUE_INVALID".to_string());
      }
    }
    for region in &self.foreign_text.manual_regions {
      if region.end_ms <= region.start_ms || ![region.x, region.y, region.width, region.height].iter().all(|v| v.is_finite()) || region.x < 0.0 || region.y < 0.0 || region.width <= 0.0 || region.height <= 0.0 || region.x + region.width > 1.0 || region.y + region.height > 1.0 {
        return Err("MANUAL_REGION_INVALID".to_string());
      }
    }
    for sticker in &self.foreign_text.stickers {
      if sticker.id.trim().is_empty() || sticker.path.trim().is_empty() || sticker.end_ms <= sticker.start_ms || !sticker.opacity.is_finite() || !(0.0..=1.0).contains(&sticker.opacity) || ![sticker.x, sticker.y, sticker.width, sticker.height].iter().all(|v| v.is_finite()) || sticker.x < 0.0 || sticker.y < 0.0 || sticker.width <= 0.0 || sticker.height <= 0.0 || sticker.x + sticker.width > 1.0 || sticker.y + sticker.height > 1.0 {
        return Err("STICKER_OVERLAY_INVALID".to_string());
      }
    }
    if let Some(schedule) = &self.publish_schedule {
      if schedule.timezone != "Asia/Ho_Chi_Minh" {
        return Err("SCHEDULE_TIMEZONE_UNSUPPORTED".to_string());
      }
      if schedule.slots.iter().any(|slot| !is_valid_schedule_slot(slot)) {
        return Err("SCHEDULE_SLOT_INVALID".to_string());
      }
    }
    if self.source_language.trim().is_empty() || self.target_language.trim().is_empty() {
      return Err("LANGUAGE_INVALID".to_string());
    }
    if !matches!(self.tts_audio_mode.as_str(), "REPLACE" | "DUCK_ORIGINAL" | "MIX") {
      return Err("TTS_AUDIO_MODE_INVALID".to_string());
    }
    if !self.original_audio_gain.is_finite() || !(0.0..=2.0).contains(&self.original_audio_gain) {
      return Err("ORIGINAL_AUDIO_GAIN_INVALID".to_string());
    }
    if self.ocr_sample_interval_ms == 0 {
      return Err("OCR_SAMPLE_INTERVAL_INVALID".to_string());
    }
    Ok(())
  }

  /// The canonical one-click workflow requested by the CapCut Automation
  /// product.  Keeping this constructor in Rust prevents UI clients from
  /// drifting away from the documented local preset contract.
  pub fn quick_localized_vertical_v1() -> Self {
    let mut preset = Self::default();
    preset.auto_transcribe = true;
    preset.auto_translate = true;
    preset.auto_ocr = true;
    preset.auto_diarize = true;
    preset.auto_tts = true;
    preset
  }

  /// Load old presets while filling fields introduced by schema v1.
  pub fn migrate_from_value(value: serde_json::Value) -> Result<Self, String> {
    let mut merged = serde_json::to_value(Self::default()).map_err(|e| format!("PRESET_MIGRATION_FAILED: {e}"))?;
    merge_defaults(&mut merged, value);
    serde_json::from_value(merged).map_err(|e| format!("PRESET_MIGRATION_FAILED: {e}"))
  }
}

fn merge_defaults(defaults: &mut serde_json::Value, supplied: serde_json::Value) {
  match (defaults, supplied) {
    (serde_json::Value::Object(defaults), serde_json::Value::Object(supplied)) => {
      for (key, value) in supplied {
        if let Some(default) = defaults.get_mut(&key) {
          merge_defaults(default, value);
        } else {
          defaults.insert(key, value);
        }
      }
    },
    (target, supplied) => *target = supplied,
  }
}

fn clamp(value: f64, low: f64, high: f64) -> f64 {
  value.max(low).min(high)
}

fn is_valid_schedule_slot(slot: &str) -> bool {
  let parts = slot.split(':').collect::<Vec<_>>();
  parts.len() == 2 && parts[0].parse::<u8>().ok().is_some_and(|hour| hour < 24) && parts[1].parse::<u8>().ok().is_some_and(|minute| minute < 60)
}

/// Build the canonical video graph.  The order is intentional: orientation,
/// mirror, geometry, colour, privacy treatment, subtitles/hook, retime, format.
pub fn build_filter_graph(preset: &CapcutAutomationPresetV1, subtitle_path: Option<&Path>, hook_ass_path: Option<&Path>) -> Result<String, String> {
  preset.validate()?;
  let mut parts = Vec::new();
  parts.push("setdar=dar".to_string());
  if preset.mirror_horizontal {
    parts.push("hflip".to_string());
  }
  let geometry = match preset.output.scale_mode.to_ascii_lowercase().as_str() {
    "fit" | "fit_with_background" | "keep_source_ratio" => format!("scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2", preset.output.width, preset.output.height, preset.output.width, preset.output.height),
    "manual_crop" | "center_crop" | "fill" => format!("scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}", preset.output.width, preset.output.height, preset.output.width, preset.output.height),
    _ => format!("scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}", preset.output.width, preset.output.height, preset.output.width, preset.output.height),
  };
  parts.push(geometry);
  let c = &preset.color;
  if [c.brightness, c.contrast, c.saturation, c.gamma].iter().any(|v| v.abs() > 0.001) {
    // UI values are percentages on a -100..100 scale.  Keep this mapping
    // explicit so the canonical quick preset (+5/-5/+3) reaches FFmpeg as
    // brightness +0.05, saturation 0.95 and contrast 1.03.
    parts.push(format!("eq=brightness={:.4}:contrast={:.4}:saturation={:.4}:gamma={:.4}", clamp(c.brightness / 100.0, -1.0, 1.0), clamp(1.0 + c.contrast / 100.0, 0.4, 1.8), clamp(1.0 + c.saturation / 100.0, 0.0, 2.2), clamp(1.0 + c.gamma / 100.0 * 0.75, 0.5, 2.0)));
  }
  if c.hue.abs() > 0.001 {
    parts.push(format!("hue=h={:.3}", clamp(c.hue * 1.8, -180.0, 180.0)));
  }
  if preset.foreign_text.enabled {
    // Timed regions are applied only in the labelled filter-complex below.
    // Adding a full-frame blur/drawbox here would both duplicate the effect
    // and, for blur, make the entire frame unreadable before the crop overlay.
  }
  let overlay_parts = [subtitle_path, hook_ass_path]
    .into_iter()
    .flatten()
    .map(|path| {
      let escaped = escape_filter_path(path)?;
      let filter = if path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| extension.eq_ignore_ascii_case("srt")) { "subtitles" } else { "ass" };
      Ok::<_, String>(format!("{filter}=filename='{escaped}'"))
    })
    .collect::<Result<Vec<_>, _>>()?;

  let mut suffix = overlay_parts;
  if (preset.playback_rate - 1.0).abs() > 0.001 {
    suffix.push(format!("setpts=PTS/{:.6}", preset.playback_rate));
  }
  suffix.push("format=yuv420p".to_string());

  let regions = if preset.foreign_text.enabled { preset.foreign_text.manual_regions.iter().map(TimedRegion::clamped).collect::<Vec<_>>() } else { Vec::new() };
  let stickers = if preset.foreign_text.enabled { preset.foreign_text.stickers.iter().filter(|sticker| sticker.enabled).collect::<Vec<_>>() } else { Vec::new() };
  if regions.is_empty() && stickers.is_empty() {
    parts.extend(suffix);
    return Ok(parts.join(","));
  }

  // A region blur must not blur the entire frame.  Split the current stream,
  // crop only the selected rectangle, blur that crop, then overlay it back.
  // The labelled graph is consumed with `-filter_complex` by the renderer.
  let mut graph = format!("[0:v]{}[capcut_pre0]", parts.join(","));
  let mut current = 0usize;
  for (index, region) in regions.iter().enumerate() {
    match preset.foreign_text.action {
      ForeignTextAction::Blur => {
        graph.push_str(&format!(";[capcut_pre{current}]split=2[capcut_base{index}][capcut_region{index}];[capcut_region{index}]crop=w=iw*{:.6}:h=ih*{:.6}:x=iw*{:.6}:y=ih*{:.6},boxblur=10:1[capcut_blurred{index}];[capcut_base{index}][capcut_blurred{index}]overlay=x=main_w*{:.6}:y=main_h*{:.6}:enable='between(t,{:.3},{:.3})'[capcut_pre{}]", region.width, region.height, region.x, region.y, region.x, region.y, region.start_ms as f64 / 1_000.0, region.end_ms as f64 / 1_000.0, index + 1));
      },
      ForeignTextAction::Cover | ForeignTextAction::Sticker => {
        if matches!(preset.foreign_text.action, ForeignTextAction::Sticker) {
          if let Some(path) = preset.foreign_text.sticker_path.as_deref().filter(|path| Path::new(path).is_file()) {
            let escaped = escape_filter_path(Path::new(path))?;
            graph.push_str(&format!(";movie=filename='{escaped}'[capcut_sticker{index}];[capcut_pre{current}][capcut_sticker{index}]overlay=x=main_w*{:.6}:y=main_h*{:.6}:enable='between(t,{:.3},{:.3})'[capcut_pre{}]", region.x, region.y, region.start_ms as f64 / 1_000.0, region.end_ms as f64 / 1_000.0, index + 1));
          } else {
            graph.push_str(&format!(";[capcut_pre{current}]drawbox=x=iw*{:.6}:y=ih*{:.6}:w=iw*{:.6}:h=ih*{:.6}:color=white@0.01:t=fill:enable='between(t,{:.3},{:.3})'[capcut_pre{}]", region.x, region.y, region.width, region.height, region.start_ms as f64 / 1_000.0, region.end_ms as f64 / 1_000.0, index + 1));
          }
        } else {
          graph.push_str(&format!(";[capcut_pre{current}]drawbox=x=iw*{:.6}:y=ih*{:.6}:w=iw*{:.6}:h=ih*{:.6}:color=black@0.85:t=fill:enable='between(t,{:.3},{:.3})'[capcut_pre{}]", region.x, region.y, region.width, region.height, region.start_ms as f64 / 1_000.0, region.end_ms as f64 / 1_000.0, index + 1));
        }
      },
    }
    current = index + 1;
  }
  for (offset, sticker) in stickers.iter().enumerate() {
    let Some(path) = Path::new(&sticker.path).canonicalize().ok().filter(|path| path.is_file()) else {
      continue;
    };
    let escaped = escape_filter_path(&path)?;
    let label = regions.len() + offset;
    graph.push_str(&format!(";movie=filename='{escaped}',format=rgba,colorchannelmixer=aa={:.4}[capcut_sticker{label}];[capcut_pre{current}][capcut_sticker{label}]overlay=x=main_w*{:.6}:y=main_h*{:.6}:enable='between(t,{:.3},{:.3})'[capcut_pre{}]", sticker.opacity, sticker.x, sticker.y, sticker.start_ms as f64 / 1_000.0, sticker.end_ms as f64 / 1_000.0, label + 1));
    current = label + 1;
  }
  if suffix.is_empty() {
    graph.push_str(&format!(";[capcut_pre{current}]null[capcut_out]"));
  } else {
    graph.push_str(&format!(";[capcut_pre{current}]{}[capcut_out]", suffix.join(",")));
  }
  Ok(graph)
}

pub fn build_audio_filter(preset: &CapcutAutomationPresetV1) -> Result<Option<String>, String> {
  preset.validate()?;
  if matches!(preset.audio_policy, AudioRightsPolicy::MuteOriginal) {
    return Ok(Some("volume=0".to_string()));
  }
  if !preset.preserve_audio_pitch || (preset.playback_rate - 1.0).abs() <= 0.001 {
    return Ok(None);
  }
  Ok(Some(format!("atempo={:.6}", preset.playback_rate)))
}

fn escape_filter_path(path: &Path) -> Result<String, String> {
  let canonical = path.canonicalize().map_err(|e| format!("FONT_OR_SUBTITLE_PATH_INVALID: {e}"))?;
  // FFmpeg's filters do not understand the Windows extended-length `//?/`
  // prefix returned by canonicalize().  Keep the path absolute while
  // converting it to the drive-letter form accepted by libass.
  let mut value = canonical.to_string_lossy().replace('\\', "/");
  if let Some(stripped) = value.strip_prefix("//?/") {
    value = stripped.to_string();
  }
  let value = value.replace(':', "\\:").replace('\'', "\\'");
  Ok(value)
}

pub fn file_hashes(path: &Path) -> Result<(String, String), String> {
  let mut file = File::open(path).map_err(|e| format!("ARTIFACT_VERIFY_FAILED: {e}"))?;
  let mut sha = Sha256::new();
  let mut md5 = Md5::new();
  let mut buffer = [0u8; 64 * 1024];
  loop {
    let n = file.read(&mut buffer).map_err(|e| format!("ARTIFACT_VERIFY_FAILED: {e}"))?;
    if n == 0 {
      break;
    }
    sha.update(&buffer[..n]);
    md5.update(&buffer[..n]);
  }
  let hex_encode = |bytes: &[u8]| -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
      encoded.push(HEX[(byte >> 4) as usize] as char);
      encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
  };
  let sha_bytes = sha.finalize();
  let md5_bytes = md5.finalize();
  Ok((hex_encode(&sha_bytes), hex_encode(&md5_bytes)))
}

/// Render to a job-local `.partial` file and atomically publish it.  The input
/// is never overwritten and user text is passed through an ASS file, not a
/// shell command string.
pub fn render_video(ffmpeg: &Path, input: &Path, output: &Path, preset: &CapcutAutomationPresetV1) -> Result<(String, String), String> {
  render_video_with_overlays(ffmpeg, input, output, preset, None, None)
}

pub fn render_video_with_overlays(ffmpeg: &Path, input: &Path, output: &Path, preset: &CapcutAutomationPresetV1, subtitle_path: Option<&Path>, hook_ass_path: Option<&Path>) -> Result<(String, String), String> {
  render_video_with_progress(ffmpeg, input, output, preset, subtitle_path, hook_ass_path, |_| {}, || false)
}

pub fn media_duration_ms(ffmpeg: &Path, input: &Path) -> Option<f64> {
  let probe_name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
  let probe = ffmpeg.parent()?.join(probe_name);
  if !probe.is_file() {
    return None;
  }
  let result = Command::new(probe).args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"]).arg(input).output().ok()?;
  if !result.status.success() {
    return None;
  }
  let seconds = String::from_utf8_lossy(&result.stdout).trim().parse::<f64>().ok()?;
  if seconds.is_finite() && seconds > 0.0 {
    Some(seconds * 1_000.0)
  } else {
    None
  }
}

/// Render while consuming FFmpeg's machine-readable progress stream.  The
/// cancellation closure is checked between progress records and kills only
/// this child process, never a process by name.
pub fn render_video_with_progress<F, C>(ffmpeg: &Path, input: &Path, output: &Path, preset: &CapcutAutomationPresetV1, subtitle_path: Option<&Path>, hook_ass_path: Option<&Path>, mut on_progress: F, should_cancel: C) -> Result<(String, String), String>
where
  F: FnMut(f64),
  C: Fn() -> bool,
{
  render_video_with_progress_and_pid_and_audio(ffmpeg, input, output, preset, subtitle_path, hook_ass_path, None, |_| {}, on_progress, should_cancel)
}

pub fn render_video_with_progress_and_pid<P, F, C>(ffmpeg: &Path, input: &Path, output: &Path, preset: &CapcutAutomationPresetV1, subtitle_path: Option<&Path>, hook_ass_path: Option<&Path>, mut on_pid: P, mut on_progress: F, should_cancel: C) -> Result<(String, String), String>
where
  P: FnMut(u32),
  F: FnMut(f64),
  C: Fn() -> bool,
{
  render_video_with_progress_and_pid_and_audio(ffmpeg, input, output, preset, subtitle_path, hook_ass_path, None, on_pid, on_progress, should_cancel)
}

/// Render with an optional locally synthesized audio track. When supplied the
/// track is selected according to the preset TTS mode; the original track is
/// retained only for MIX/DUCK_ORIGINAL.
pub fn render_video_with_progress_and_pid_and_audio<P, F, C>(ffmpeg: &Path, input: &Path, output: &Path, preset: &CapcutAutomationPresetV1, subtitle_path: Option<&Path>, hook_ass_path: Option<&Path>, audio_override: Option<&Path>, mut on_pid: P, mut on_progress: F, should_cancel: C) -> Result<(String, String), String>
where
  P: FnMut(u32),
  F: FnMut(f64),
  C: Fn() -> bool,
{
  preset.validate()?;
  let input = input.canonicalize().map_err(|e| format!("INPUT_NOT_READABLE: {e}"))?;
  if !input.is_file() {
    return Err("INPUT_NOT_FOUND".to_string());
  }
  if !ffmpeg.is_file() {
    return Err("RENDER_START_FAILED: FFmpeg is unavailable".to_string());
  }
  if input == output {
    return Err("INPUT_OUTPUT_MUST_DIFFER".to_string());
  }
  if let Some(parent) = output.parent() {
    fs::create_dir_all(parent).map_err(|e| format!("OUTPUT_DIRECTORY_CREATE_FAILED: {e}"))?;
  }
  let partial = output.with_extension(format!("{}.partial", output.extension().and_then(|e| e.to_str()).unwrap_or("mp4")));
  let graph = build_filter_graph(preset, subtitle_path, hook_ass_path)?;
  let mut command = Command::new(ffmpeg);
  command.args(["-hide_banner", "-loglevel", "error", "-nostats", "-progress", "pipe:1", "-y", "-i"]).arg(&input);
  if let Some(audio) = audio_override.filter(|path| path.is_file()) {
    command.args(["-i"]).arg(audio);
    if graph.starts_with("[0:v]") {
      let audio_label = match preset.tts_audio_mode.as_str() {
        "REPLACE" => "[1:a]aresample=async=1[capcut_audio]".to_string(),
        "DUCK_ORIGINAL" => format!("[0:a]volume={:.4}[capcut_orig];[1:a]aresample=async=1[capcut_tts];[capcut_orig][capcut_tts]amix=inputs=2:duration=first:dropout_transition=2[capcut_audio]", preset.original_audio_gain.clamp(0.0, 2.0) * 0.25),
        _ => format!("[0:a]volume={:.4}[capcut_orig];[1:a]aresample=async=1[capcut_tts];[capcut_orig][capcut_tts]amix=inputs=2:duration=first:dropout_transition=2[capcut_audio]", preset.original_audio_gain.clamp(0.0, 2.0)),
      };
      // When TTS is mixed through the complex graph, applying `-af` as a
      // separate simple filter is invalid in FFmpeg.  Keep the playback-rate
      // correction inside the same graph and map the filtered label.
      let audio_map = if let Some(audio_filter) = build_audio_filter(preset)? {
        format!("{audio_label};[capcut_audio]{audio_filter}[capcut_audio_final]")
      } else {
        audio_label
      };
      let graph_with_audio = format!("{graph};{audio_map}");
      let mapped_audio = if build_audio_filter(preset)?.is_some() { "[capcut_audio_final]" } else { "[capcut_audio]" };
      command.args(["-filter_complex", &graph_with_audio, "-map", "[capcut_out]", "-map", mapped_audio]);
    } else {
      let graph_with_audio = format!("[0:v]{graph}[capcut_out];[1:a]aresample=async=1[capcut_audio]");
      command.args(["-filter_complex", &graph_with_audio, "-map", "[capcut_out]", "-map", "[capcut_audio]"]);
    }
  } else if graph.starts_with("[0:v]") {
    command.args(["-filter_complex", &graph, "-map", "[capcut_out]", "-map", "0:a?"]);
  } else {
    command.args(["-vf", &graph, "-map", "0:v:0", "-map", "0:a?"]);
  }
  command.args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p", "-c:a", "aac", "-movflags", "+faststart"]);
  if audio_override.is_none() {
    if let Some(audio) = build_audio_filter(preset)? {
    command.args(["-af", &audio]);
    }
  }
  if audio_override.is_some() {
    command.args(["-shortest"]);
  }
  // The atomic `.partial` suffix is intentionally not a media extension, so
  // tell FFmpeg the container explicitly instead of relying on filename
  // probing (which otherwise exits with EINVAL on Windows).
  command.args(["-f", &preset.output.container]).arg(&partial).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
  let mut child: Child = command.spawn().map_err(|e| format!("RENDER_START_FAILED: {e}"))?;
  on_pid(child.id());
  let stdout = child.stdout.take().ok_or_else(|| "RENDER_START_FAILED: progress pipe unavailable".to_string())?;
  // Drain stderr concurrently with the machine-readable progress stream.
  // Waiting until stdout closes can deadlock FFmpeg when libass/filters emit
  // enough diagnostics to fill the OS pipe buffer; in that case FFmpeg blocks
  // before producing the next progress record and the job appears stuck at 0%.
  let stderr = child.stderr.take();
  let stderr_thread = stderr.map(|mut stream| {
    std::thread::spawn(move || {
      let mut text = String::new();
      let _ = std::io::Read::read_to_string(&mut stream, &mut text);
      text
    })
  });
  // Read progress on a dedicated thread so the control loop remains
  // responsive even when FFmpeg is busy initialising a large filter graph.
  // A blocking read_line here would otherwise make cancellation and timeout
  // impossible to observe while the first frame is being processed.
  let (progress_tx, progress_rx) = std::sync::mpsc::channel::<Option<String>>();
  let progress_thread = std::thread::spawn(move || {
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    loop {
      line.clear();
      match std::io::BufRead::read_line(&mut reader, &mut line) {
        Ok(0) => break,
        Ok(_) => {
          if progress_tx.send(Some(line.clone())).is_err() {
            break;
          }
        }
        Err(_) => break,
      }
    }
    let _ = progress_tx.send(None);
  });
  let mut last_ms = 0.0_f64;
  let duration_ms = media_duration_ms(ffmpeg, &input);
  // Rendering should be bounded even if a codec/filter deadlocks. Allow a
  // generous multiplier for slower machines while preventing hour-long
  // zombie jobs. The bound is independent of the UI's estimated duration.
  let expected_seconds = duration_ms.unwrap_or(60_000.0) / 1_000.0;
  let render_timeout = std::time::Duration::from_secs_f64((expected_seconds * 10.0 + 300.0).max(600.0));
  let render_started = std::time::Instant::now();
  let mut progress_closed = false;
  loop {
    if should_cancel() {
      let _ = child.kill();
      let _ = child.wait();
      let _ = progress_thread.join();
      let _ = fs::remove_file(&partial);
      return Err("JOB_CANCELLED".to_string());
    }
    if render_started.elapsed() >= render_timeout {
      let _ = child.kill();
      let _ = child.wait();
      let _ = progress_thread.join();
      let _ = fs::remove_file(&partial);
      return Err(format!("RENDER_TIMEOUT: FFmpeg produced no complete output within {}s", render_timeout.as_secs()));
    }
    match progress_rx.recv_timeout(std::time::Duration::from_millis(250)) {
      Ok(Some(line)) => {
        if let Some(raw) = line.strip_prefix("out_time_ms=").or_else(|| line.strip_prefix("out_time_us=")) {
          if let Ok(value) = raw.trim().parse::<f64>() {
            let ms = value / 1_000.0;
            last_ms = last_ms.max(ms);
            on_progress(progress_fraction(last_ms, duration_ms));
          }
        }
        if line.trim() == "progress=end" {
          on_progress(1.0);
        }
      }
      Ok(None) => {
        progress_closed = true;
      }
      Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
      Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
        progress_closed = true;
      }
    }
    if progress_closed {
      break;
    }
  }
  let _ = progress_thread.join();
  let stderr_text = stderr_thread.and_then(|thread| thread.join().ok()).unwrap_or_default();
  let result = child.wait().map_err(|e| format!("RENDER_FAILED: {e}"))?;
  if !result.success() {
    let _ = fs::remove_file(&partial);
    let detail = stderr_text.trim();
    return Err(if detail.is_empty() { format!("RENDER_FAILED: FFmpeg exited with {result}") } else { format!("RENDER_FAILED: FFmpeg exited with {result}: {detail}") });
  }
  let meta = fs::metadata(&partial).map_err(|e| format!("ARTIFACT_NOT_FOUND: {e}"))?;
  if meta.len() == 0 {
    let _ = fs::remove_file(&partial);
    return Err("ARTIFACT_VERIFY_FAILED: rendered file is empty".to_string());
  }
  fs::rename(&partial, output).map_err(|e| format!("ATOMIC_OUTPUT_FAILED: {e}"))?;
  file_hashes(output)
}

/// Write a minimal UTF-8-safe ASS overlay for the configured hook.
pub fn write_hook_ass(path: &Path, preset: &CapcutAutomationPresetV1) -> Result<(), String> {
  preset.validate()?;
  if !preset.hook.enabled {
    return Err("HOOK_DISABLED".to_string());
  }
  let start = ass_timestamp(preset.hook.start_ms);
  let end = ass_timestamp(preset.hook.end_ms);
  let text = escape_ass_text(&preset.hook.text);
  let style = &preset.localization.subtitle_style;
  let body = format!("[Script Info]\nScriptType: v4.00+\nPlayResX: {}\nPlayResY: {}\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Hook,{},{},&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,1,0,1,2,1,8,40,40,{},1\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,{}, {},Hook,,0,0,0,,{}\n", preset.output.width, preset.output.height, style.font_name, style.font_size, style.margin_v, start, end, text);
  let partial = path.with_extension("ass.partial");
  let mut file = File::create(&partial).map_err(|e| format!("OVERLAY_WRITE_FAILED: {e}"))?;
  file.write_all(body.as_bytes()).map_err(|e| format!("OVERLAY_WRITE_FAILED: {e}"))?;
  file.sync_all().ok();
  fs::rename(partial, path).map_err(|e| format!("OVERLAY_WRITE_FAILED: {e}"))
}

/// Materialize manually edited cues as an ASS file so the renderer uses the
/// same safe, deterministic overlay path as imported subtitles.
pub fn write_manual_subtitle_ass(path: &Path, preset: &CapcutAutomationPresetV1) -> Result<(), String> {
  preset.validate()?;
  let cues = preset.localization.manual_cues.iter().filter(|cue| cue.enabled).collect::<Vec<_>>();
  if cues.is_empty() {
    return Err("SUBTITLE_CUES_EMPTY".to_string());
  }
  let style = &preset.localization.subtitle_style;
  let mut body = format!("[Script Info]\nScriptType: v4.00+\nPlayResX: {}\nPlayResY: {}\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,{},{},&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,0,0,1,2,1,2,40,40,{},1\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n", preset.output.width, preset.output.height, style.font_name, style.font_size, style.margin_v);
  for cue in cues {
    body.push_str(&format!("Dialogue: 0,{}, {},Default,,0,0,0,,{}\n", ass_timestamp(cue.start_ms), ass_timestamp(cue.end_ms), escape_ass_text(&cue.text)));
  }
  let partial = path.with_extension("ass.partial");
  let mut file = File::create(&partial).map_err(|e| format!("OVERLAY_WRITE_FAILED: {e}"))?;
  file.write_all(body.as_bytes()).map_err(|e| format!("OVERLAY_WRITE_FAILED: {e}"))?;
  file.sync_all().ok();
  fs::rename(partial, path).map_err(|e| format!("OVERLAY_WRITE_FAILED: {e}"))
}

fn ass_timestamp(ms: u64) -> String {
  let hours = ms / 3_600_000;
  let minutes = (ms % 3_600_000) / 60_000;
  let seconds = (ms % 60_000) / 1_000;
  let centiseconds = (ms % 1_000) / 10;
  format!("{hours}:{minutes:02}:{seconds:02}.{centiseconds:02}")
}

fn escape_ass_text(value: &str) -> String {
  value.replace('\\', "\\\\").replace('{', "\\{").replace('}', "\\}").replace('\n', "\\N")
}

pub fn write_receipt(path: &Path, receipt: &CapcutAutomationReceipt) -> Result<(), String> {
  let partial = path.with_extension("json.partial");
  let bytes = serde_json::to_vec_pretty(receipt).map_err(|e| format!("RECEIPT_WRITE_FAILED: {e}"))?;
  let mut file = File::create(&partial).map_err(|e| format!("RECEIPT_WRITE_FAILED: {e}"))?;
  file.write_all(&bytes).map_err(|e| format!("RECEIPT_WRITE_FAILED: {e}"))?;
  file.sync_all().ok();
  fs::rename(partial, path).map_err(|e| format!("RECEIPT_WRITE_FAILED: {e}"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  #[test]
  fn default_preset_matches_canonical_editor_values() {
    let preset = CapcutAutomationPresetV1::default();
    assert_eq!(preset.schema_version, 1);
    assert!(preset.mirror_horizontal);
    assert_eq!(preset.playback_rate, 1.1);
    assert_eq!(preset.hook.start_ms, 0);
    assert_eq!(preset.hook.end_ms, 3_000);
    assert!(preset.validate().is_ok());
  }

  #[test]
  fn validation_rejects_invalid_playback_and_regions() {
    let mut preset = CapcutAutomationPresetV1::default();
    preset.playback_rate = 2.1;
    assert_eq!(preset.validate().unwrap_err(), "PLAYBACK_RATE_OUT_OF_RANGE");
    preset.playback_rate = 1.0;
    preset.foreign_text.manual_regions.push(TimedRegion { start_ms: 0, end_ms: 1_000, x: 0.9, y: 0.0, width: 0.2, height: 0.2 });
    assert_eq!(preset.validate().unwrap_err(), "MANUAL_REGION_INVALID");
  }

  #[test]
  fn filter_order_mirror_colour_retime_is_deterministic() {
    let preset = CapcutAutomationPresetV1::default();
    let graph = build_filter_graph(&preset, None, None).unwrap();
    assert!(graph.find("hflip").unwrap() < graph.find("scale=").unwrap());
    assert!(graph.find("scale=").unwrap() < graph.find("eq=").unwrap());
    assert!(graph.find("eq=").unwrap() < graph.find("setpts=").unwrap());
    assert!(graph.ends_with("format=yuv420p"));
  }

  #[test]
  fn color_mapping_uses_editor_scale() {
    let preset = CapcutAutomationPresetV1::default();
    let graph = build_filter_graph(&preset, None, None).unwrap();
    assert!(graph.contains("brightness=0.0500"));
    assert!(graph.contains("contrast=1.0300"));
    assert!(graph.contains("saturation=0.9500"));
  }

  #[test]
  fn audio_retime_and_mute_follow_rights_policy() {
    let mut preset = CapcutAutomationPresetV1::default();
    assert_eq!(build_audio_filter(&preset).unwrap().as_deref(), Some("atempo=1.100000"));
    preset.audio_policy = AudioRightsPolicy::MuteOriginal;
    assert_eq!(build_audio_filter(&preset).unwrap().as_deref(), Some("volume=0"));
  }

  #[test]
  fn file_hashes_are_real_and_receipt_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("artifact.bin");
    fs::write(&file, b"abc").unwrap();
    let (sha, md5) = file_hashes(&file).unwrap();
    assert_eq!(sha, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(md5, "900150983cd24fb0d6963f7d28e17f72");
    let receipt_path = dir.path().join("receipt.json");
    let receipt = CapcutAutomationReceipt { schema_version: 1, job_id: "job".into(), request_id: "req".into(), input_sha256: sha.clone(), output_sha256: sha, output_md5: md5, output_path: file.to_string_lossy().into(), duration_ms: 0, width: 1080, height: 1920, video_codec: "h264".into(), audio_codec: "aac".into(), playback_rate: 1.1, subtitle_burned: false, subtitle_mode: "NONE".into(), subtitle_cue_count: 0, hook_applied: false, foreign_text_regions_applied: 0, terminal: true, state: "COMPLETED".into(), ..Default::default() };
    write_receipt(&receipt_path, &receipt).unwrap();
    assert!(receipt_path.is_file());
    assert!(!receipt_path.with_extension("json.partial").exists());
  }

  #[test]
  fn legacy_preset_migrates_nested_defaults() {
    let value = serde_json::json!({"name":"legacy","playbackRate":1.0,"mirrorHorizontal":false});
    let preset = CapcutAutomationPresetV1::migrate_from_value(value).unwrap();
    assert_eq!(preset.name, "legacy");
    assert_eq!(preset.output.width, 1080);
    assert!(preset.hook.style.font_size > 0);
  }

  #[test]
  fn output_timeline_is_retimed_without_shrinking_hook() {
    assert_eq!(output_time_ms(10_000, 1.1), 9_091);
    let preset = CapcutAutomationPresetV1::default();
    assert_eq!(preset.hook.start_ms, 0);
    assert_eq!(preset.hook.end_ms, 3_000);
  }

  #[test]
  fn progress_fraction_requires_duration_and_is_bounded() {
    assert_eq!(progress_fraction(500.0, Some(1_000.0)), 0.5);
    assert_eq!(progress_fraction(2_000.0, Some(1_000.0)), 1.0);
    assert_eq!(progress_fraction(-1.0, Some(1_000.0)), 0.0);
    assert_eq!(progress_fraction(500.0, None), 0.0);
  }

  #[test]
  fn ass_overlay_escapes_text_and_uses_output_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hook.ass");
    let mut preset = CapcutAutomationPresetV1::default();
    preset.hook.text = "Xin {chào}\nViệt".into();
    write_hook_ass(&path, &preset).unwrap();
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("0:00:00.00"));
    assert!(text.contains("0:00:03.00"));
    assert!(text.contains("\\{chào\\}"));
  }

  #[test]
  fn regions_are_clamped_for_renderer_bounds() {
    let region = TimedRegion { start_ms: 0, end_ms: 1_000, x: 0.9, y: -1.0, width: 0.5, height: 2.0 };
    let clamped = region.clamped();
    assert_eq!(clamped.x, 0.9);
    assert_eq!(clamped.y, 0.0);
    assert!((clamped.width - 0.1).abs() < 1e-9);
    assert_eq!(clamped.height, 1.0);
  }

  #[test]
  fn output_dimensions_must_be_even() {
    let mut preset = CapcutAutomationPresetV1::default();
    preset.output.width = 1079;
    assert_eq!(preset.validate().unwrap_err(), "OUTPUT_DIMENSIONS_INVALID");
  }

  #[test]
  fn quick_localized_vertical_preset_matches_canonical_contract() {
    let preset = CapcutAutomationPresetV1::quick_localized_vertical_v1();
    assert_eq!(preset.preset_id.as_deref(), Some("QUICK_LOCALIZED_VERTICAL_V1"));
    assert!(preset.mirror_horizontal);
    assert_eq!(preset.playback_rate, 1.1);
    assert!(preset.preserve_audio_pitch);
    assert_eq!(preset.color.brightness, 5.0);
    assert_eq!(preset.color.saturation, -5.0);
    assert_eq!(preset.color.contrast, 3.0);
    assert_eq!(preset.hook.start_ms, 0);
    assert_eq!(preset.hook.end_ms, 3_000);
    assert_eq!(preset.output.ratio, "9:16");
    assert_eq!(preset.output.width, 1_080);
    assert_eq!(preset.output.height, 1_920);
    preset.validate().unwrap();
  }

  #[test]
  fn schedule_validation_accepts_local_slots_and_rejects_other_timezones() {
    let mut preset = CapcutAutomationPresetV1::default();
    preset.publish_schedule = Some(PublishScheduleSettings { timezone: "Asia/Ho_Chi_Minh".into(), slots: vec!["11:30".into(), "20:00".into()] });
    preset.validate().unwrap();
    preset.publish_schedule.as_mut().unwrap().timezone = "UTC".into();
    assert_eq!(preset.validate().unwrap_err(), "SCHEDULE_TIMEZONE_UNSUPPORTED");
  }

  #[test]
  fn multiple_overlay_paths_are_ordered_after_privacy_filters() {
    let dir = tempfile::tempdir().unwrap();
    let subtitle = dir.path().join("subtitle.ass");
    let hook = dir.path().join("hook.ass");
    fs::write(&subtitle, "[Events]").unwrap();
    fs::write(&hook, "[Events]").unwrap();
    let graph = build_filter_graph(&CapcutAutomationPresetV1::default(), Some(&subtitle), Some(&hook)).unwrap();
    assert!(graph.find("boxblur").unwrap_or(0) < graph.find("ass=filename").unwrap());
    assert!(graph.matches("ass=filename").count() == 2);
  }

  #[test]
  fn srt_subtitles_use_subtitles_filter() {
    let dir = tempfile::tempdir().unwrap();
    let subtitle = dir.path().join("subtitle.srt");
    fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:01,000\nHello\n").unwrap();
    let graph = build_filter_graph(&CapcutAutomationPresetV1::default(), Some(&subtitle), None).unwrap();
    assert!(graph.contains("subtitles=filename"));
  }

  #[test]
  fn manual_subtitle_cues_validate_and_emit_ass() {
    let temp = tempfile::tempdir().unwrap();
    let mut preset = CapcutAutomationPresetV1::default();
    preset.hook.enabled = false;
    preset.localization.manual_cues.push(SubtitleCue { start_ms: 0, end_ms: 1_500, text: "Xin chào\nworld".into(), enabled: true });
    let path = temp.path().join("manual.ass");
    write_manual_subtitle_ass(&path, &preset).unwrap();
    let body = fs::read_to_string(path).unwrap();
    assert!(body.contains("Dialogue: 0,0:00:00.00, 0:00:01.50"));
    assert!(body.contains("Xin chào\\Nworld"));
  }

  #[test]
  fn sticker_overlay_rejects_invalid_opacity_and_accepts_normalized_timeline() {
    let mut preset = CapcutAutomationPresetV1::default();
    preset.foreign_text.enabled = true;
    preset.foreign_text.stickers.push(StickerOverlay { id: "s1".into(), path: "sticker.png".into(), x: 0.1, y: 0.2, width: 0.3, height: 0.25, opacity: 0.75, start_ms: 100, end_ms: 900, enabled: true });
    assert!(preset.validate().is_ok());
    preset.foreign_text.stickers[0].opacity = 1.5;
    assert_eq!(preset.validate().unwrap_err(), "STICKER_OVERLAY_INVALID");
  }

  #[test]
  fn enabled_blur_regions_use_crop_overlay_graph() {
    let mut preset = CapcutAutomationPresetV1::default();
    preset.foreign_text.enabled = true;
    preset.foreign_text.manual_regions = vec![TimedRegion { start_ms: 0, end_ms: 2_000, x: 0.1, y: 0.2, width: 0.3, height: 0.2 }];
    let graph = build_filter_graph(&preset, None, None).unwrap();
    assert!(graph.starts_with("[0:v]"));
    assert!(graph.contains("crop=w=iw*0.300000"));
    assert!(graph.contains("overlay=x=main_w*0.100000"));
  }

  #[test]
  fn sticker_region_requires_existing_asset_and_adds_movie_overlay() {
    let dir = tempfile::tempdir().unwrap();
    let sticker = dir.path().join("mask.png");
    fs::write(&sticker, b"not-a-real-image").unwrap();
    let mut preset = CapcutAutomationPresetV1::default();
    preset.foreign_text.enabled = true;
    preset.foreign_text.action = ForeignTextAction::Sticker;
    preset.foreign_text.sticker_path = Some(sticker.to_string_lossy().to_string());
    preset.foreign_text.manual_regions = vec![TimedRegion { start_ms: 0, end_ms: 1_000, x: 0.1, y: 0.1, width: 0.2, height: 0.2 }];
    let graph = build_filter_graph(&preset, None, None).unwrap();
    assert!(graph.contains("movie=filename"));
    assert!(graph.contains("overlay=x=main_w*0.100000"));
  }
}
