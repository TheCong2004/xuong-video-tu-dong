use artcraft_app_lib::services::pipeline::capcut_automation::{build_audio_filter, build_filter_graph, file_hashes, render_video_with_overlays, write_hook_ass, write_manual_subtitle_ass, AudioRightsPolicy, CapcutAutomationPresetV1, StickerOverlay, SubtitleCue, ForeignTextAction};
use std::path::PathBuf;
use std::fs;

#[test]
fn local_automation_contract_is_stable_end_to_end() {
  let dir = tempfile::tempdir().expect("temp dir");
  let input = dir.path().join("input with spaces.mp4");
  fs::write(&input, b"fixture").unwrap();
  let (sha, md5) = file_hashes(&input).unwrap();
  assert_eq!(sha.len(), 64);
  assert_eq!(md5.len(), 32);

  let preset = CapcutAutomationPresetV1::default();
  assert_eq!(preset.preset_id.as_deref(), Some("QUICK_LOCALIZED_VERTICAL_V1"));
  assert!(preset.mirror_horizontal);
  assert_eq!(preset.playback_rate, 1.1);
  assert!(preset.preserve_audio_pitch);
  assert_eq!(preset.localization.target_language, "vi");
  assert!(preset.localization.translate);
  assert!(preset.localization.burn_subtitles);
  assert_eq!(preset.hook.start_ms, 0);
  assert_eq!(preset.hook.end_ms, 3_000);
  assert_eq!(preset.output.ratio, "9:16");
  assert_eq!(preset.output.width, 1_080);
  assert_eq!(preset.output.height, 1_920);
  assert_eq!(preset.output.scale_mode, "fill");
  assert!(matches!(preset.foreign_text.detection_mode, artcraft_app_lib::services::pipeline::capcut_automation::ForeignTextDetectionMode::OcrWithManualReview));
  let hook = dir.path().join("hook.ass");
  write_hook_ass(&hook, &preset).unwrap();
  let graph = build_filter_graph(&preset, None, Some(&hook)).unwrap();
  assert!(graph.contains("hflip"));
  assert!(graph.contains("ass=filename"));
  assert!(graph.contains("setpts=PTS/1.100000"));
  assert_eq!(build_audio_filter(&preset).unwrap().as_deref(), Some("atempo=1.100000"));

  let mut muted = preset;
  muted.audio_policy = AudioRightsPolicy::MuteOriginal;
  assert_eq!(build_audio_filter(&muted).unwrap().as_deref(), Some("volume=0"));
}

#[test]
fn local_fixture_smoke_renders_and_verifies_artifact() {
  let input = std::env::var_os("FLOWORD_SMOKE_INPUT").map(PathBuf::from);
  let ffmpeg = std::env::var_os("FLOWORD_SMOKE_FFMPEG").map(PathBuf::from);
  let (Some(input), Some(ffmpeg)) = (input, ffmpeg) else {
    return;
  };
  assert!(input.is_file(), "smoke input is missing");
  assert!(ffmpeg.is_file(), "smoke ffmpeg is missing");
  let dir = tempfile::tempdir().expect("temp dir");
  let output = dir.path().join("rendered.mp4");
  let hook = dir.path().join("hook.ass");
  let preset = CapcutAutomationPresetV1::default();
  write_hook_ass(&hook, &preset).unwrap();
  let (sha, md5) = render_video_with_overlays(&ffmpeg, &input, &output, &preset, None, Some(&hook)).unwrap();
  let (input_sha, _) = file_hashes(&input).unwrap();
  assert_ne!(sha, input_sha);
  assert_eq!(sha.len(), 64);
  assert_eq!(md5.len(), 32);
  assert!(output.is_file());
}

#[test]
fn subtitle_and_sticker_overlays_are_wired_into_the_native_graph() {
  let dir = tempfile::tempdir().expect("temp dir");
  let sticker = dir.path().join("sticker.png");
  fs::write(&sticker, b"fixture").unwrap();
  let subtitle = dir.path().join("manual.ass");
  let mut preset = CapcutAutomationPresetV1::default();
  preset.localization.manual_cues.push(SubtitleCue { start_ms: 0, end_ms: 1_000, text: "Xin chao".into(), enabled: true });
  preset.foreign_text.enabled = true;
  preset.foreign_text.action = ForeignTextAction::Sticker;
  preset.foreign_text.stickers.push(StickerOverlay { id: "sticker-1".into(), path: sticker.to_string_lossy().into_owned(), x: 0.1, y: 0.2, width: 0.25, height: 0.2, opacity: 0.8, start_ms: 0, end_ms: 1_000, enabled: true });

  write_manual_subtitle_ass(&subtitle, &preset).unwrap();
  let graph = build_filter_graph(&preset, Some(&subtitle), None).unwrap();
  assert!(graph.contains("ass=filename"));
  assert!(graph.contains("movie=filename"));
  assert!(graph.contains("between(t,0.000,1.000)"));
  assert!(graph.contains("aa=0.8000"));
}
