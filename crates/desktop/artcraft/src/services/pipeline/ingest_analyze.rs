//! Phase 1 `ingest_analyze` runtime owned by the canonical Rust pipeline worker.

use crate::services::pipeline::artifact_store::{ArtifactStore, FlowordArtifact};
use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef, StageId};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use vynaro_detect::{Ffmpeg, FfmpegProbe};
use app_lib::services::pipeline::PipelineDownloadConfig;

const SCENE_THRESHOLD: f64 = 0.30;

#[derive(Debug)]
pub struct IngestAnalyzeResult {
  pub artifact_refs: Vec<ArtifactRef>,
  pub physical_artifacts: Vec<FlowordArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestAnalyzeError {
  pub code: &'static str,
  pub message: String,
  pub retryable: bool,
  pub cancelled: bool,
}

impl std::fmt::Display for IngestAnalyzeError {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(formatter, "{}: {}", self.code, self.message)
  }
}

impl std::error::Error for IngestAnalyzeError {}

#[derive(Serialize)]
struct SceneDocument {
  threshold: f64,
  cuts_seconds: Vec<f64>,
  scenes: Vec<SceneSpan>,
}

#[derive(Serialize)]
struct SceneSpan {
  index: usize,
  start_seconds: f64,
  end_seconds: f64,
}

pub async fn ingest_local_source(source: &Path, workflow_root: &Path, workflow_id: &str, cancel_flag: Arc<AtomicBool>) -> Result<IngestAnalyzeResult, IngestAnalyzeError> {
  ingest_local_source_with_runtime(None, source, workflow_root, workflow_id, cancel_flag).await
}

pub async fn ingest_local_source_with_app(app: &AppHandle, source: &Path, workflow_root: &Path, workflow_id: &str, cancel_flag: Arc<AtomicBool>) -> Result<IngestAnalyzeResult, IngestAnalyzeError> {
  ingest_local_source_with_runtime(Some(app), source, workflow_root, workflow_id, cancel_flag).await
}

async fn ingest_local_source_with_runtime(app: Option<&AppHandle>, source: &Path, workflow_root: &Path, workflow_id: &str, cancel_flag: Arc<AtomicBool>) -> Result<IngestAnalyzeResult, IngestAnalyzeError> {
  validate_local_source(source)?;
  check_cancelled(&cancel_flag)?;
  std::fs::create_dir_all(workflow_root).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("cannot create artifact root: {error}"), false))?;
  let ingest_dir = workflow_root.join("ingest_analyze");
  std::fs::create_dir_all(&ingest_dir).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("cannot create ingest directory: {error}"), false))?;
  let extension = source.extension().and_then(|value| value.to_str()).unwrap_or("mp4");
  let source_video = ingest_dir.join(format!("source_video.{extension}"));
  std::fs::copy(source, &source_video).map_err(|error| runtime_error("LOCAL_SOURCE_COPY_FAILED", format!("cannot copy source video: {error}"), false))?;
  validate_local_source(&source_video)?;
  let source_artifact = register_source_video(workflow_root, workflow_id, &source_video, "local_input", json!({ "acquisition": "local_file", "youwee": "skipped" }))?;
  analyze_source_video(app, source_artifact, workflow_root, workflow_id, "local_file", cancel_flag).await
}

pub async fn ingest_url_source(app: &AppHandle, url: &str, workflow_root: &Path, workflow_id: &str, cancel_flag: Arc<AtomicBool>) -> Result<IngestAnalyzeResult, IngestAnalyzeError> {
  ingest_url_source_with_config(app, url, workflow_root, workflow_id, cancel_flag, &PipelineDownloadConfig::default()).await
}

pub async fn ingest_url_source_with_config(app: &AppHandle, url: &str, workflow_root: &Path, workflow_id: &str, cancel_flag: Arc<AtomicBool>, download_config: &PipelineDownloadConfig) -> Result<IngestAnalyzeResult, IngestAnalyzeError> {
  check_cancelled(&cancel_flag)?;
  std::fs::create_dir_all(workflow_root).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("cannot create artifact root: {error}"), false))?;
  let ingest_dir = workflow_root.join("ingest_analyze");
  std::fs::create_dir_all(&ingest_dir).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("cannot create ingest directory: {error}"), false))?;
  let source_video = ingest_dir.join("source_video.mp4");

  let download = app_lib::services::pipeline::download_pipeline_source_with_config(app, url, &source_video, Arc::clone(&cancel_flag), download_config).await.map_err(|error| IngestAnalyzeError { code: error.code, message: error.message, retryable: error.retryable, cancelled: error.cancelled })?;
  check_cancelled(&cancel_flag)?;
  let source_artifact = register_source_video(workflow_root, workflow_id, &download.source_path, "youwee", json!({ "acquisition": "source_url", "source_url": download.source_url, "extractor": download.extractor }))?;
  analyze_source_video(Some(app), source_artifact, workflow_root, workflow_id, "source_url", cancel_flag).await
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebArticleDocument {
  pub url: String,
  pub title: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub author: Option<String>,
  pub text: String,
  pub source_type: String,
  pub retrieved_at: String,
}

pub async fn ingest_web_story_source(url: &str, workflow_root: &Path, workflow_id: &str, cancel_flag: Arc<AtomicBool>) -> Result<IngestAnalyzeResult, IngestAnalyzeError> {
  check_cancelled(&cancel_flag)?;
  std::fs::create_dir_all(workflow_root).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("cannot create artifact root: {error}"), false))?;
  let ingest_dir = workflow_root.join("ingest_analyze");
  std::fs::create_dir_all(&ingest_dir).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("cannot create ingest directory: {error}"), false))?;

  let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36").build().map_err(|error| runtime_error("WEB_CONTENT_FETCH_FAILED", format!("cannot build HTTP client: {error}"), false))?;

  let response = tokio::select! {
    result = client.get(url).send() => result,
    _ = wait_for_cancel_flag(&cancel_flag) => return Err(IngestAnalyzeError { code: "INGEST_CANCELLED", message: "Web story ingest cancelled".to_string(), retryable: false, cancelled: true }),
  }
  .map_err(|error| runtime_error(if error.is_timeout() { "WEB_CONTENT_TIMEOUT" } else { "WEB_CONTENT_FETCH_FAILED" }, format!("failed to fetch article URL: {error}"), true))?;

  if !response.status().is_success() {
    return Err(runtime_error("WEB_CONTENT_FETCH_FAILED", format!("HTTP error {}: failed to load article URL", response.status().as_u16()), response.status().is_server_error()));
  }

  let html_text = response.text().await.map_err(|error| runtime_error("WEB_CONTENT_FETCH_FAILED", format!("failed to read article body: {error}"), true))?;
  check_cancelled(&cancel_flag)?;

  let article = extract_article_content(url, &html_text)?;
  if article.text.trim().is_empty() {
    return Err(runtime_error("WEB_CONTENT_EMPTY", "Extracted article text is empty".to_string(), false));
  }

  let source_text_path = ingest_dir.join("source_text.json");
  let metadata_path = ingest_dir.join("source_metadata.json");
  let scenes_path = ingest_dir.join("scenes.json");

  write_json(&source_text_path, &article)?;
  let word_count = article.text.split_whitespace().count();
  write_json(&metadata_path, &json!({ "acquisition": "web_story", "url": url, "title": &article.title, "author": &article.author, "word_count": word_count }))?;
  write_json(&scenes_path, &json!({ "threshold": 0.0, "cuts_seconds": [], "scenes": [] }))?;

  let source_text_artifact = ArtifactStore::register_typed_artifact(workflow_root, workflow_id, StageId::IngestAnalyze, "web_story_extractor", ArtifactKind::SourceText, &source_text_path, json!({ "url": url, "title": &article.title, "source_type": "web_article" })).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("failed to register source_text: {error}"), false))?;

  let source_meta_artifact = ArtifactStore::register_typed_artifact(workflow_root, workflow_id, StageId::IngestAnalyze, "web_story_extractor", ArtifactKind::SourceMetadata, &metadata_path, json!({ "url": url, "title": &article.title })).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("failed to register source_metadata: {error}"), false))?;

  let scenes_artifact = ArtifactStore::register_typed_artifact(workflow_root, workflow_id, StageId::IngestAnalyze, "web_story_extractor", ArtifactKind::Scenes, &scenes_path, json!({ "scene_count": 0 })).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("failed to register scenes: {error}"), false))?;

  let text_ref = source_text_artifact.to_artifact_ref(StageId::IngestAnalyze).map_err(|error| runtime_error("INGEST_ARTIFACT_INVALID", error.to_string(), false))?;
  let meta_ref = source_meta_artifact.to_artifact_ref(StageId::IngestAnalyze).map_err(|error| runtime_error("INGEST_ARTIFACT_INVALID", error.to_string(), false))?;
  let scenes_ref = scenes_artifact.to_artifact_ref(StageId::IngestAnalyze).map_err(|error| runtime_error("INGEST_ARTIFACT_INVALID", error.to_string(), false))?;

  Ok(IngestAnalyzeResult { artifact_refs: vec![text_ref, meta_ref, scenes_ref], physical_artifacts: vec![source_text_artifact, source_meta_artifact, scenes_artifact] })
}

pub fn extract_article_content(url: &str, raw_html: &str) -> Result<WebArticleDocument, IngestAnalyzeError> {
  let title = extract_html_title(raw_html).unwrap_or_else(|| "Web Article".to_string());
  let author = extract_html_author(raw_html);
  let text = extract_clean_text(raw_html);
  let retrieved_at = chrono::Utc::now().to_rfc3339();

  Ok(WebArticleDocument { url: url.to_string(), title, author, text, source_type: "web_article".to_string(), retrieved_at })
}

fn extract_html_title(html: &str) -> Option<String> {
  if let Some(og_title) = extract_meta_content(html, "og:title") {
    if !og_title.trim().is_empty() {
      return Some(og_title.trim().to_string());
    }
  }
  if let Some(start) = html.to_ascii_lowercase().find("<title>") {
    let after = &html[start + 7..];
    if let Some(end) = after.to_ascii_lowercase().find("</title>") {
      let title = &after[..end];
      let decoded = strip_html_tags(title);
      if !decoded.trim().is_empty() {
        return Some(decoded.trim().to_string());
      }
    }
  }
  None
}

fn extract_html_author(html: &str) -> Option<String> {
  extract_meta_content(html, "author").or_else(|| extract_meta_content(html, "article:author"))
}

fn extract_meta_content(html: &str, property_or_name: &str) -> Option<String> {
  let lower = html.to_ascii_lowercase();
  let target = property_or_name.to_ascii_lowercase();
  for token in ["property=", "name="] {
    let search = format!("{token}\"{target}\"");
    if let Some(pos) = lower.find(&search) {
      let tag_start = html[..pos].rfind('<')?;
      let tag_end = html[pos..].find('>')? + pos;
      let tag = &html[tag_start..=tag_end];
      if let Some(content_pos) = tag.to_ascii_lowercase().find("content=\"") {
        let content_after = &tag[content_pos + 9..];
        if let Some(content_end) = content_after.find('"') {
          return Some(content_after[..content_end].to_string());
        }
      }
    }
  }
  None
}

fn extract_clean_text(html: &str) -> String {
  if !html.contains('<') && !html.contains('>') {
    return html.trim().to_string();
  }

  let mut clean = html.to_string();
  for tag in &["script", "style", "nav", "header", "footer", "svg", "noscript"] {
    loop {
      let lower = clean.to_ascii_lowercase();
      let open = format!("<{tag}");
      let close = format!("</{tag}>");
      if let Some(start) = lower.find(&open) {
        if let Some(end_offset) = lower[start..].find(&close) {
          clean.replace_range(start..start + end_offset + close.len(), " ");
          continue;
        }
      }
      break;
    }
  }

  let text = strip_html_tags(&clean);
  let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
  lines.join("\n\n")
}

fn strip_html_tags(html: &str) -> String {
  let mut result = String::with_capacity(html.len());
  let mut inside_tag = false;
  for ch in html.chars() {
    if ch == '<' {
      inside_tag = true;
    } else if ch == '>' {
      inside_tag = false;
      result.push(' ');
    } else if !inside_tag {
      result.push(ch);
    }
  }
  result.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

async fn wait_for_cancel_flag(flag: &Arc<AtomicBool>) {
  while !flag.load(Ordering::SeqCst) {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
  }
}

async fn analyze_source_video(app: Option<&AppHandle>, source_artifact: (FlowordArtifact, ArtifactRef), workflow_root: &Path, workflow_id: &str, acquisition: &str, cancel_flag: Arc<AtomicBool>) -> Result<IngestAnalyzeResult, IngestAnalyzeError> {
  source_artifact.1.validate().map_err(|error| runtime_error("INGEST_ARTIFACT_INVALID", error.to_string(), false))?;
  let source_video = PathBuf::from(&source_artifact.1.location);
  let ffmpeg = resolve_vynaro_runtime(app).await?;
  check_cancelled(&cancel_flag)?;
  let probe = ffmpeg.probe(&source_video).await.map_err(|error| runtime_error("VYNARO_PROBE_FAILED", error.to_string(), true))?;
  check_cancelled(&cancel_flag)?;
  validate_probe(&probe)?;

  let cuts = ffmpeg.detect_scenes(&source_video, SCENE_THRESHOLD).await.map_err(|error| runtime_error("VYNARO_SCENE_DETECTION_FAILED", error.to_string(), true))?;
  check_cancelled(&cancel_flag)?;
  let scenes = build_scene_document(probe.duration_seconds, cuts);

  let ingest_dir = source_video.parent().ok_or_else(|| runtime_error("INGEST_STORAGE_FAILED", "source video has no parent".to_string(), false))?;
  let metadata_path = ingest_dir.join("source_metadata.json");
  let scenes_path = ingest_dir.join("scenes.json");
  let audio_path = ingest_dir.join("source_audio.wav");
  if audio_path.exists() {
    std::fs::remove_file(&audio_path).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("cannot replace prior audio artifact: {error}"), false))?;
  }
  write_json(&metadata_path, &json!({ "acquisition": acquisition, "probe": probe }))?;
  write_json(&scenes_path, &scenes)?;
  ffmpeg.extract_audio(&source_video, &audio_path).await.map_err(|error| runtime_error("VYNARO_AUDIO_EXTRACTION_FAILED", error.to_string(), true))?;
  check_cancelled(&cancel_flag)?;

  let source_artifact_id = source_artifact.1.artifact_id.clone();
  let registrations = [(ArtifactKind::SourceMetadata, metadata_path.as_path(), json!({ "duration_seconds": probe.duration_seconds, "source_video_artifact_id": &source_artifact_id })), (ArtifactKind::Scenes, scenes_path.as_path(), json!({ "scene_count": scenes.scenes.len(), "source_video_artifact_id": &source_artifact_id })), (ArtifactKind::SourceAudio, audio_path.as_path(), json!({ "sample_rate_hz": 16000, "channels": 1, "duration_seconds": probe.duration_seconds, "source_video_artifact_id": &source_artifact_id }))];
  let mut physical_artifacts = vec![source_artifact.0];
  let mut artifact_refs = vec![source_artifact.1];
  for (kind, path, metadata) in registrations {
    let artifact = ArtifactStore::register_typed_artifact(workflow_root, workflow_id, StageId::IngestAnalyze, "vynaro", kind, path, metadata).map_err(|error| runtime_error("INGEST_ARTIFACT_INVALID", error.to_string(), false))?;
    let artifact_ref = artifact.to_artifact_ref(StageId::IngestAnalyze).map_err(|error| runtime_error("INGEST_ARTIFACT_INVALID", error.to_string(), false))?;
    physical_artifacts.push(artifact);
    artifact_refs.push(artifact_ref);
  }
  Ok(IngestAnalyzeResult { artifact_refs, physical_artifacts })
}

fn register_source_video(workflow_root: &Path, workflow_id: &str, source_video: &Path, producer: &str, metadata: serde_json::Value) -> Result<(FlowordArtifact, ArtifactRef), IngestAnalyzeError> {
  validate_local_source(source_video)?;
  let artifact = ArtifactStore::register_typed_artifact(workflow_root, workflow_id, StageId::IngestAnalyze, producer, ArtifactKind::SourceVideo, source_video, metadata).map_err(|error| runtime_error("INGEST_ARTIFACT_INVALID", error.to_string(), false))?;
  let artifact_ref = artifact.to_artifact_ref(StageId::IngestAnalyze).map_err(|error| runtime_error("INGEST_ARTIFACT_INVALID", error.to_string(), false))?;
  artifact_ref.validate().map_err(|error| runtime_error("INGEST_ARTIFACT_INVALID", error.to_string(), false))?;
  Ok((artifact, artifact_ref))
}

async fn resolve_vynaro_runtime(app: Option<&AppHandle>) -> Result<Ffmpeg, IngestAnalyzeError> {
  if let Some(app) = app {
    if let Some(ffmpeg_path) = app_lib::services::get_ffmpeg_path(app).await {
      let ffprobe_name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
      if let Some(ffprobe_path) = ffmpeg_path.parent().map(|parent| parent.join(ffprobe_name)).filter(|path| path.is_file()) {
        return Ok(Ffmpeg::with_bins(ffmpeg_path, ffprobe_path));
      }
    }
  }
  Ffmpeg::discover().map_err(|error| runtime_error("VYNARO_RUNTIME_UNAVAILABLE", error.to_string(), false))
}

fn validate_local_source(source: &Path) -> Result<(), IngestAnalyzeError> {
  let metadata = std::fs::metadata(source).map_err(|error| runtime_error("LOCAL_SOURCE_INVALID", format!("{} is unavailable: {error}", source.display()), false))?;
  if !metadata.is_file() || metadata.len() == 0 {
    return Err(runtime_error("LOCAL_SOURCE_INVALID", format!("{} is not a non-empty file", source.display()), false));
  }
  Ok(())
}

fn validate_probe(probe: &FfmpegProbe) -> Result<(), IngestAnalyzeError> {
  if !probe.duration_seconds.is_finite() || probe.duration_seconds <= 0.0 || probe.width == 0 || probe.height == 0 {
    return Err(runtime_error("VYNARO_INVALID_METADATA", "ffprobe returned invalid duration or dimensions".to_string(), false));
  }
  if probe.audio_codec.is_none() {
    return Err(runtime_error("VYNARO_AUDIO_MISSING", "source video has no audio stream".to_string(), false));
  }
  Ok(())
}

fn build_scene_document(duration_seconds: f64, mut cuts: Vec<f64>) -> SceneDocument {
  cuts.retain(|cut| cut.is_finite() && *cut > 0.0 && *cut < duration_seconds);
  cuts.sort_by(f64::total_cmp);
  cuts.dedup_by(|left, right| (*left - *right).abs() < 0.001);
  let mut boundaries = Vec::with_capacity(cuts.len() + 2);
  boundaries.push(0.0);
  boundaries.extend(cuts.iter().copied());
  boundaries.push(duration_seconds);
  let scenes = boundaries.windows(2).enumerate().map(|(index, window)| SceneSpan { index, start_seconds: window[0], end_seconds: window[1] }).collect();
  SceneDocument { threshold: SCENE_THRESHOLD, cuts_seconds: cuts, scenes }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), IngestAnalyzeError> {
  let bytes = serde_json::to_vec_pretty(value).map_err(|error| runtime_error("INGEST_SERIALIZATION_FAILED", error.to_string(), false))?;
  std::fs::write(path, bytes).map_err(|error| runtime_error("INGEST_STORAGE_FAILED", format!("cannot write {}: {error}", path.display()), false))
}

fn check_cancelled(cancel_flag: &AtomicBool) -> Result<(), IngestAnalyzeError> {
  if cancel_flag.load(Ordering::SeqCst) {
    Err(cancelled_error())
  } else {
    Ok(())
  }
}

fn cancelled_error() -> IngestAnalyzeError {
  IngestAnalyzeError { code: "INGEST_CANCELLED", message: "User requested job cancellation".to_string(), retryable: false, cancelled: true }
}

fn runtime_error(code: &'static str, message: String, retryable: bool) -> IngestAnalyzeError {
  IngestAnalyzeError { code, message, retryable, cancelled: false }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashSet;

  #[tokio::test]
  async fn local_fixture_produces_real_valid_ingest_artifacts() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixture = workspace.join("test_data/video/mp4/golden_sun_garoh.mp4");
    let output_root = tempfile::tempdir().unwrap();
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let result = ingest_local_source(&fixture, output_root.path(), "fixture-job", cancel_flag).await.unwrap();
    let kinds = result.artifact_refs.iter().map(|artifact| artifact.kind).collect::<HashSet<_>>();

    assert_eq!(result.artifact_refs.len(), 4);
    assert_eq!(result.physical_artifacts.len(), 4);
    assert_eq!(kinds, HashSet::from([ArtifactKind::SourceVideo, ArtifactKind::SourceMetadata, ArtifactKind::Scenes, ArtifactKind::SourceAudio]));
    let source_id = result.artifact_refs.iter().find(|artifact| artifact.kind == ArtifactKind::SourceVideo).unwrap().artifact_id.clone();
    for artifact in &result.artifact_refs {
      artifact.validate().unwrap();
      if artifact.kind != ArtifactKind::SourceVideo {
        assert_eq!(artifact.metadata.get("source_video_artifact_id").and_then(serde_json::Value::as_str), Some(source_id.as_str()));
      }
    }
  }

  #[tokio::test]
  #[ignore = "requires FLOWORD_PHASE1_RUNTIME_VIDEO and real ffmpeg/ffprobe"]
  async fn external_runtime_video_produces_real_valid_ingest_artifacts() {
    let fixture = PathBuf::from(std::env::var("FLOWORD_PHASE1_RUNTIME_VIDEO").expect("FLOWORD_PHASE1_RUNTIME_VIDEO is required"));
    let output_root = tempfile::tempdir().unwrap();
    let result = ingest_local_source(&fixture, output_root.path(), "url-runtime-job", Arc::new(AtomicBool::new(false))).await.unwrap();

    assert_eq!(result.artifact_refs.len(), 4);
    for artifact in &result.artifact_refs {
      artifact.validate().unwrap();
    }
  }

  #[tokio::test]
  async fn cancelled_local_ingest_stops_before_vynaro() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixture = workspace.join("test_data/video/mp4/golden_sun_garoh.mp4");
    let output_root = tempfile::tempdir().unwrap();
    let cancel_flag = Arc::new(AtomicBool::new(true));

    let error = ingest_local_source(&fixture, output_root.path(), "fixture-job", cancel_flag).await.unwrap_err();

    assert_eq!(error.code, "INGEST_CANCELLED");
    assert!(error.cancelled);
    assert!(!output_root.path().join("ingest_analyze").exists());
  }

  #[test]
  fn scene_document_covers_entire_video() {
    let document = build_scene_document(10.0, vec![7.0, 2.0, 2.0, -1.0, 12.0]);
    assert_eq!(document.scenes.len(), 3);
    assert_eq!(document.scenes[0].start_seconds, 0.0);
    assert_eq!(document.scenes[2].end_seconds, 10.0);
  }

  #[test]
  fn extract_article_content_extracts_title_author_and_clean_text() {
    let html = r#"
      <!DOCTYPE html>
      <html>
        <head>
          <title>AI Revolution in 2026 - TechNews</title>
          <meta name="author" content="John Doe">
          <style>body { font-family: sans-serif; }</style>
          <script>console.log('ad tracker');</script>
        </head>
        <body>
          <header><nav><a href="/">Home</a></nav></header>
          <main>
            <h1>AI Revolution in 2026</h1>
            <p>Artificial intelligence is transforming storytelling and media automation rapidly &amp; safely.</p>
            <p>Second paragraph with &quot;quoted text&quot; and details.</p>
          </main>
          <footer>Copyright 2026</footer>
        </body>
      </html>
    "#;
    let doc = extract_article_content("https://example.com/ai-2026", html).unwrap();
    assert_eq!(doc.url, "https://example.com/ai-2026");
    assert_eq!(doc.title, "AI Revolution in 2026 - TechNews");
    assert_eq!(doc.author.as_deref(), Some("John Doe"));
    assert!(doc.text.contains("Artificial intelligence is transforming"));
    assert!(doc.text.contains("\"quoted text\""));
    assert!(!doc.text.contains("console.log"));
    assert!(!doc.text.contains("font-family"));
  }
}
