use crate::services::pipeline::clients::omniroute_client::{generate_video, list_video_models, OmniRouteVideoError, OmniRouteVideoResult, StructuredScript};
use crate::services::pipeline::artifact_store::ArtifactStore;
use crate::services::pipeline::contracts::{ArtifactKind, ArtifactRef, StageId};
use log::{info, warn};
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use vynaro_detect::Ffmpeg;

/// Timeout for one provider/model attempt. A transient failure may then fall
/// through to the next connected provider without blocking the workflow for
/// fifteen minutes on a single account.
const VIDEO_GENERATION_TIMEOUT_SECS: u64 = 300;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenePlan {
  pub version: u8,
  pub target_duration_seconds: f64,
  pub scenes: Vec<ScenePlanItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenePlanItem {
  pub scene_id: String,
  pub order: u32,
  pub start_seconds: f64,
  pub end_seconds: f64,
  pub duration_seconds: f64,
  pub visual_prompt: String,
  pub asset_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualAssetError {
  pub code: &'static str,
  pub message: String,
}

impl VisualAssetError {
  fn new(code: &'static str, message: impl Into<String>) -> Self {
    Self { code, message: message.into() }
  }
}

pub fn build_scene_plan(script: &StructuredScript, target_duration_seconds: u32) -> Result<ScenePlan, VisualAssetError> {
  if target_duration_seconds == 0 || script.scenes.is_empty() {
    return Err(VisualAssetError { code: "SCENE_PLAN_INVALID", message: "positive duration and at least one scripted scene are required".into() });
  }
  let weights = script.scenes.iter().map(|scene| scene.duration_ms.max(1) as f64).collect::<Vec<_>>();
  let total_weight: f64 = weights.iter().sum();
  let target = target_duration_seconds as f64;
  let mut cursor = 0.0;
  let mut scenes = Vec::with_capacity(script.scenes.len());
  for (position, (scene, weight)) in script.scenes.iter().zip(weights).enumerate() {
    let prompt = [&scene.visual_instruction, &scene.caption, &scene.narration].into_iter().map(|value| value.trim()).find(|value| !value.is_empty()).ok_or_else(|| VisualAssetError { code: "SCENE_PLAN_VISUAL_PROMPT_REQUIRED", message: format!("scene {} has no visual prompt", scene.id) })?;
    let end = if position + 1 == script.scenes.len() { target } else { cursor + target * weight / total_weight };
    scenes.push(ScenePlanItem { scene_id: scene.id.clone(), order: position as u32, start_seconds: cursor, end_seconds: end, duration_seconds: end - cursor, visual_prompt: prompt.to_string(), asset_type: "video".into() });
    cursor = end;
  }
  Ok(ScenePlan { version: 1, target_duration_seconds: target, scenes })
}

fn supports_prompt_only_text_to_video(is_disabled: Option<bool>, text_to_video_supported: Option<bool>, text_prompt_supported: Option<bool>, starting_keyframe_required: Option<bool>) -> bool {
  is_disabled != Some(true) && text_to_video_supported != Some(false) && text_prompt_supported == Some(true) && starting_keyframe_required != Some(true)
}

fn router_aspect_ratio_str(value: &str) -> &str {
  match value {
    "16:9" => "16:9",
    "1:1" => "1:1",
    _ => "9:16",
  }
}

fn map_omniroute_error_to_visual(error: &OmniRouteVideoError) -> VisualAssetError {
  VisualAssetError::new(
    match error {
      OmniRouteVideoError::NoVideoProvider => "VISUAL_PROVIDER_UNAVAILABLE",
      OmniRouteVideoError::Unauthorized => "VISUAL_AUTH_REQUIRED",
      OmniRouteVideoError::PaymentRequired => "VISUAL_PAYMENT_REQUIRED",
      OmniRouteVideoError::RateLimited => "VISUAL_RATE_LIMITED",
      OmniRouteVideoError::ProviderTimeout => "VISUAL_PROVIDER_TIMEOUT",
      OmniRouteVideoError::Unavailable(_) => "VISUAL_PROVIDER_UNAVAILABLE",
      OmniRouteVideoError::InvalidRequest(_) => "VISUAL_INVALID_REQUEST",
      OmniRouteVideoError::GenerationFailed(_) => "VISUAL_GENERATION_FAILED",
    },
    error.message(),
  )
}

fn should_fallback_to_next_video_model(error: &OmniRouteVideoError, has_next: bool) -> bool {
  has_next && error.permits_fallback()
}

/// Generate real visual assets for all scenes in the plan via OmniRoute.
///
/// OmniRoute is the SINGLE AI gateway — it owns provider selection, credential
/// resolution, model routing, fallback, and polling. Floword only sends the
/// normalized request and receives a video URL.
pub async fn generate_visual_assets(app: &AppHandle, work_dir: &Path, job_id: &str, plan: &ScenePlan, aspect_ratio: &str, cancel_flag: Arc<AtomicBool>) -> Result<Vec<ArtifactRef>, VisualAssetError> {
  // Discover video models available from OmniRoute. This must not fall back to any
  // direct provider call — if OmniRoute has no models, we report VISUAL_PROVIDER_UNAVAILABLE.
  let video_models = match list_video_models().await {
    Ok(models) if !models.is_empty() => {
      info!("OMNIROUTE_VIDEO_MODELS available={} first={}", models.len(), models[0].id);
      models
    },
    Ok(_) => {
      warn!("NO_OMNIROUTE_VIDEO_PROVIDER OmniRoute returned no video-capable models");
      return Err(VisualAssetError::new("VISUAL_PROVIDER_UNAVAILABLE", "OmniRoute has no configured video generation provider (NO_OMNIROUTE_VIDEO_PROVIDER)"));
    },
    Err(error) => {
      warn!("OMNIROUTE_UNAVAILABLE cannot fetch video model catalog: {error}");
      return Err(VisualAssetError::new("VISUAL_PROVIDER_UNAVAILABLE", format!("OmniRoute unavailable when fetching video models: {error}")));
    },
  };

  let runtime = app_lib::services::resolve_ffmpeg_runtime(app).await.map_err(|error| VisualAssetError::new("VISUAL_ARTIFACT_INVALID", error.to_string()))?;
  let ffmpeg = Ffmpeg::with_bins(runtime.ffmpeg_path, runtime.ffprobe_path);
  let asset_dir = work_dir.join("media_assets");
  std::fs::create_dir_all(&asset_dir).map_err(|error| VisualAssetError::new("VISUAL_ARTIFACT_INVALID", error.to_string()))?;
  let client = reqwest::Client::builder().timeout(Duration::from_secs(300)).build().map_err(|error| VisualAssetError::new("VISUAL_GENERATION_UNAVAILABLE", error.to_string()))?;
  let mut artifacts = Vec::with_capacity(plan.scenes.len());

  for scene in &plan.scenes {
    if cancel_flag.load(Ordering::SeqCst) {
      return Err(VisualAssetError::new("VISUAL_GENERATION_CANCELLED", "Visual generation cancelled"));
    }

    let mut generated = None;
    let mut last_error = None;
    for (model_index, model) in video_models.iter().enumerate() {
      let model_id = &model.id;
      info!("OMNIROUTE_REQUEST capability=video_generation model={model_id} attempt={}/{} job_id={job_id} scene_id={}", model_index + 1, video_models.len(), scene.scene_id);

      let attempt = tokio::select! {
        result = generate_video(
          model_id,
          &scene.visual_prompt,
          Some(scene.duration_seconds.ceil() as u16),
          Some(router_aspect_ratio_str(aspect_ratio)),
          VIDEO_GENERATION_TIMEOUT_SECS,
        ) => result,
        _ = wait_for_cancel(&cancel_flag) => {
          return Err(VisualAssetError::new("VISUAL_GENERATION_CANCELLED", "Visual generation cancelled"));
        }
      };

      match attempt {
        Ok(result) => {
          generated = Some(result);
          break;
        },
        Err(error) => {
          warn!("AI_REQUEST_FAILED capability=video_generation model={model_id} scene_id={} code={} detail={}", scene.scene_id, error.code(), error.message());
          let has_next = model_index + 1 < video_models.len();
          if should_fallback_to_next_video_model(&error, has_next) {
            warn!("OMNIROUTE_VIDEO_FALLBACK failed_model={model_id} next_model={} scene_id={}", video_models[model_index + 1].id, scene.scene_id);
            last_error = Some(error);
            continue;
          }
          return Err(map_omniroute_error_to_visual(&error));
        },
      }
    }
    let result: OmniRouteVideoResult = generated.ok_or_else(|| {
      let error = last_error.unwrap_or_else(|| OmniRouteVideoError::NoVideoProvider);
      map_omniroute_error_to_visual(&error)
    })?;

    info!("OMNIROUTE_GENERATION_COMPLETED capability=video_generation model={} provider={} scene_id={}", result.model_used, result.provider_used, scene.scene_id);

    // Download the generated video from the URL provided by OmniRoute.
    let response = client.get(&result.video_url).send().await.map_err(|error| VisualAssetError::new("VISUAL_ARTIFACT_INVALID", error.to_string()))?;
    if !response.status().is_success() {
      return Err(VisualAssetError::new("VISUAL_ARTIFACT_INVALID", format!("Generated media download returned HTTP {}", response.status().as_u16())));
    }
    let content_type = response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).unwrap_or("").to_ascii_lowercase();
    if !content_type.starts_with("video/") && !content_type.contains("octet-stream") {
      return Err(VisualAssetError::new("VISUAL_ARTIFACT_INVALID", format!("Generated media has unsupported content type {content_type}")));
    }
    let extension = if content_type.contains("webm") {
      "webm"
    } else if content_type.contains("quicktime") {
      "mov"
    } else {
      "mp4"
    };
    let bytes = response.bytes().await.map_err(|error| VisualAssetError::new("VISUAL_ARTIFACT_INVALID", error.to_string()))?;
    if bytes.is_empty() {
      return Err(VisualAssetError::new("VISUAL_ARTIFACT_INVALID", "Generated media download was empty"));
    }
    let path = asset_dir.join(format!("scene-{:03}.{extension}", scene.order + 1));
    std::fs::write(&path, &bytes).map_err(|error| VisualAssetError::new("VISUAL_ARTIFACT_INVALID", error.to_string()))?;
    let probe = tokio::time::timeout(Duration::from_secs(120), ffmpeg.probe(&path)).await.map_err(|_| VisualAssetError::new("VISUAL_ARTIFACT_INVALID", "ffprobe timed out for generated video"))?.map_err(|error| VisualAssetError::new("VISUAL_ARTIFACT_INVALID", error.to_string()))?;
    if !probe.duration_seconds.is_finite() || probe.duration_seconds <= 0.0 || probe.video_codec.is_none() {
      return Err(VisualAssetError::new("VISUAL_ARTIFACT_INVALID", "Generated video has no positive duration or decodable video stream"));
    }
    let stored = ArtifactStore::register_typed_artifact(
      work_dir,
      job_id,
      StageId::StoryScript,
      "omniroute",
      ArtifactKind::GeneratedVideo,
      &path,
      json!({
        "scene_id": scene.scene_id,
        "order": scene.order,
        "duration_seconds": probe.duration_seconds,
        "provider": result.provider_used,
        "model": result.model_used,
        "asset_type": "video",
        "service": "omniroute"
      }),
    )
    .map_err(|error| VisualAssetError::new("VISUAL_ARTIFACT_INVALID", error.to_string()))?;
    info!("AI_ARTIFACT_CREATED capability=video_generation job_id={job_id} scene_id={} model={} provider={} artifact_id={}", scene.scene_id, result.model_used, result.provider_used, stored.id);
    artifacts.push(stored.to_artifact_ref(StageId::StoryScript).map_err(|error| VisualAssetError::new("VISUAL_ARTIFACT_INVALID", error.to_string()))?);
  }
  Ok(artifacts)
}

async fn wait_for_cancel(flag: &Arc<AtomicBool>) {
  while !flag.load(Ordering::SeqCst) {
    tokio::time::sleep(Duration::from_millis(100)).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::services::pipeline::clients::omniroute_client::ScriptScene;

  fn scene(id: &str, duration_ms: u64, visual: &str) -> ScriptScene {
    ScriptScene { id: id.into(), index: 0, narration: format!("narration {id}"), caption: String::new(), visual_instruction: visual.into(), search_keywords: vec![], emotion: String::new(), duration_ms }
  }

  #[test]
  fn scene_plan_preserves_dynamic_scene_count_and_exact_target_duration() {
    let script = StructuredScript { title: "t".into(), hook: "h".into(), cta: "c".into(), language: "vi".into(), target_duration_seconds: 11, scenes: vec![scene("one", 2_000, "first visual"), scene("two", 3_000, "second visual"), scene("three", 6_000, "third visual")] };
    let plan = build_scene_plan(&script, 11).unwrap();
    assert_eq!(plan.scenes.len(), 3);
    assert_eq!(plan.scenes.last().unwrap().end_seconds, 11.0);
    assert_eq!(plan.scenes[0].visual_prompt, "first visual");
  }

  #[test]
  fn scene_plan_rejects_scenes_without_any_visual_description() {
    let mut item = scene("empty", 1_000, "");
    item.narration.clear();
    let script = StructuredScript { title: "t".into(), hook: "h".into(), cta: "c".into(), language: "vi".into(), target_duration_seconds: 1, scenes: vec![item] };
    assert_eq!(build_scene_plan(&script, 1).unwrap_err().code, "SCENE_PLAN_VISUAL_PROMPT_REQUIRED");
  }

  #[test]
  fn omniroute_payment_required_maps_to_visual_payment_required() {
    let error = OmniRouteVideoError::PaymentRequired;
    let visual = map_omniroute_error_to_visual(&error);
    assert_eq!(visual.code, "VISUAL_PAYMENT_REQUIRED");
  }

  #[test]
  fn omniroute_no_provider_maps_to_visual_provider_unavailable() {
    let error = OmniRouteVideoError::NoVideoProvider;
    let visual = map_omniroute_error_to_visual(&error);
    assert_eq!(visual.code, "VISUAL_PROVIDER_UNAVAILABLE");
  }

  #[test]
  fn omniroute_unavailable_maps_to_visual_provider_unavailable() {
    let error = OmniRouteVideoError::Unavailable("connection refused".to_string());
    let visual = map_omniroute_error_to_visual(&error);
    assert_eq!(visual.code, "VISUAL_PROVIDER_UNAVAILABLE");
  }

  #[test]
  fn omniroute_rate_limited_maps_to_visual_rate_limited() {
    let error = OmniRouteVideoError::RateLimited;
    let visual = map_omniroute_error_to_visual(&error);
    assert_eq!(visual.code, "VISUAL_RATE_LIMITED");
  }

  #[test]
  fn quota_and_transient_errors_fall_back_to_the_next_video_provider() {
    for error in [OmniRouteVideoError::RateLimited, OmniRouteVideoError::PaymentRequired, OmniRouteVideoError::ProviderTimeout, OmniRouteVideoError::Unavailable("offline".into())] {
      assert!(should_fallback_to_next_video_model(&error, true));
    }
  }

  #[test]
  fn invalid_requests_and_last_provider_errors_do_not_loop() {
    assert!(!should_fallback_to_next_video_model(&OmniRouteVideoError::InvalidRequest("bad prompt".into()), true));
    assert!(!should_fallback_to_next_video_model(&OmniRouteVideoError::RateLimited, false));
  }

  #[test]
  fn omniroute_unauthorized_maps_to_visual_auth_required() {
    let error = OmniRouteVideoError::Unauthorized;
    let visual = map_omniroute_error_to_visual(&error);
    assert_eq!(visual.code, "VISUAL_AUTH_REQUIRED");
  }

  #[test]
  fn video_catalog_can_infer_prompt_only_capability_when_legacy_flag_is_omitted() {
    assert!(supports_prompt_only_text_to_video(None, None, Some(true), None));
    assert!(!supports_prompt_only_text_to_video(None, Some(false), Some(true), None));
    assert!(!supports_prompt_only_text_to_video(None, None, Some(true), Some(true)));
  }

  #[test]
  fn aspect_ratio_string_maps_correctly() {
    assert_eq!(router_aspect_ratio_str("16:9"), "16:9");
    assert_eq!(router_aspect_ratio_str("1:1"), "1:1");
    assert_eq!(router_aspect_ratio_str("9:16"), "9:16");
    assert_eq!(router_aspect_ratio_str("unknown"), "9:16");
  }
}
