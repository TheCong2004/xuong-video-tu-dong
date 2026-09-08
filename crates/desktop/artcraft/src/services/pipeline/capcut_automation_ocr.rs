//! ArtCraft-owned OCR boundary.
//!
//! The adapter consumes output from a verified ArtCraft-owned OCR worker. It
//! never shells out to CapCap or a cloud OCR service. RapidOCR's ONNX worker
//! is the selected Windows engine; the TSV parser remains for compatibility
//! with previously captured Tesseract evidence.

use super::capcut_automation_engine_manager::{CapcutAutomationEngineManager, CapcutEngineKind};
use super::capcut_automation_translation::{translate_local, LocalTranslationRequest, TranslationSegment};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Deserialize)]
struct RapidWorkerResponse {
  regions: Vec<RapidWorkerRegion>,
}

#[derive(Deserialize)]
struct RapidWorkerRegion {
  id: String,
  text: String,
  x: f32,
  y: f32,
  width: f32,
  height: f32,
  confidence: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LocalOcrRequest {
  pub image_path: String,
  pub language: Option<String>,
  pub image_width: u32,
  pub image_height: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OcrRegion {
  pub id: String,
  pub text: String,
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
  pub confidence: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LocalOcrTranslationRequest {
  pub regions: Vec<OcrRegion>,
  pub source_language: String,
  pub target_language: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct OcrTranslatedRegion {
  pub id: String,
  pub text: String,
  pub translated_text: String,
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
  pub confidence: Option<f32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalOcrTranslationResponse {
  pub provider: String,
  pub model: String,
  pub regions: Vec<OcrTranslatedRegion>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalOcrResponse {
  pub engine: String,
  pub language: String,
  pub regions: Vec<OcrRegion>,
}

pub fn recognize_local(manager: &CapcutAutomationEngineManager, request: LocalOcrRequest) -> Result<LocalOcrResponse, String> {
  let image = Path::new(&request.image_path);
  if !image.is_file() || request.image_width == 0 || request.image_height == 0 {
    return Err("CAPCUT_OCR_INPUT_INVALID".to_string());
  }
  let engine = manager.resolve_first(CapcutEngineKind::Ocr)?;
  let executable = engine.executable_path.ok_or_else(|| "CAPCUT_OCR_EXECUTABLE_MISSING".to_string())?;
  let requested_language = request.language.clone().filter(|value| !value.trim().is_empty());
  let rapid_worker = engine.worker_script_path.as_ref().filter(|_| engine.spec.id == "rapidocr-onnx");
  let mut command = Command::new(&executable);
  if let Some(worker) = rapid_worker {
    let python_path = executable.parent().and_then(|parent| parent.parent()).and_then(|runtime| runtime.parent()).map(|root| root.join("python")).filter(|path| path.is_dir()).unwrap_or_else(|| manager.install_root().join("python"));
    command.arg(worker).arg("--image").arg(image).env("PYTHONPATH", python_path);
  } else {
    let language = requested_language.as_deref().unwrap_or("eng");
    command.args([image.to_string_lossy().as_ref(), "stdout", "--psm", "6", "-l", language, "tsv"]);
  }
  command.stdout(Stdio::piped()).stderr(Stdio::null());
  let mut child = command.spawn().map_err(|_| "CAPCUT_OCR_PROCESS_START_FAILED".to_string())?;
  let deadline = Instant::now() + Duration::from_secs(120);
  loop {
    match child.try_wait().map_err(|_| "CAPCUT_OCR_PROCESS_FAILED".to_string())? {
      Some(_) => break,
      None if Instant::now() >= deadline => {
        let _ = child.kill();
        let _ = child.wait();
        return Err("CAPCUT_OCR_TIMEOUT".to_string());
      },
      None => std::thread::sleep(Duration::from_millis(25)),
    }
  }
  let output = child.wait_with_output().map_err(|_| "CAPCUT_OCR_PROCESS_FAILED".to_string())?;
  if !output.status.success() {
    return Err(format!("CAPCUT_OCR_FAILED:{}", output.status.code().unwrap_or(-1)));
  }
  let regions = if rapid_worker.is_some() {
    let worker: RapidWorkerResponse = serde_json::from_slice(&output.stdout).map_err(|_| "CAPCUT_OCR_OUTPUT_INVALID".to_string())?;
    worker.regions.into_iter().filter(|region| !region.text.trim().is_empty()).map(|region| OcrRegion { id: region.id, text: region.text, x: region.x.clamp(0.0, 1.0), y: region.y.clamp(0.0, 1.0), width: region.width.clamp(0.0, 1.0), height: region.height.clamp(0.0, 1.0), confidence: region.confidence.filter(|value| value.is_finite() && *value >= 0.0) }).collect()
  } else {
    parse_tsv(&String::from_utf8_lossy(&output.stdout), request.image_width, request.image_height)
  };
  let language = if rapid_worker.is_some() {
    // RapidOCR is multilingual and has no Tesseract language-pack contract.
    // Do not report a fabricated `eng`/`chi_tra` language for this engine.
    "rapidocr-multilingual".to_string()
  } else {
    requested_language.unwrap_or_else(|| "eng".to_string())
  };
  Ok(LocalOcrResponse { engine: engine.spec.id, language, regions })
}

pub fn translate_regions_local(manager: &CapcutAutomationEngineManager, request: LocalOcrTranslationRequest) -> Result<LocalOcrTranslationResponse, String> {
  if request.regions.is_empty() {
    return Err("CAPCUT_OCR_TRANSLATION_REGIONS_EMPTY".to_string());
  }
  let translation = translate_local(manager, LocalTranslationRequest { segments: request.regions.iter().map(|region| TranslationSegment { id: region.id.clone(), start_ms: 0, end_ms: 1, text: region.text.clone(), translated_text: None }).collect(), source_language: request.source_language, target_language: request.target_language })?;
  let translated_by_id = translation.segments.into_iter().map(|segment| (segment.id, segment.translated_text.unwrap_or_default())).collect::<std::collections::HashMap<_, _>>();
  let regions = request.regions.into_iter().map(|region| OcrTranslatedRegion { translated_text: translated_by_id.get(&region.id).cloned().unwrap_or_default(), id: region.id, text: region.text, x: region.x, y: region.y, width: region.width, height: region.height, confidence: region.confidence }).collect();
  Ok(LocalOcrTranslationResponse { provider: translation.provider, model: translation.model, regions })
}

/// Infer the normalized translation language for OCR text when the user chose
/// automatic source detection. OCR engines can return Chinese text even when
/// no `ocrLanguages` hint was supplied, so falling back to English would send
/// that text to the wrong model. Keep this conservative: explicit UI language
/// settings still take precedence in the job manager.
pub fn infer_ocr_source_language(regions: &[OcrRegion]) -> String {
  let text = regions.iter().map(|region| region.text.as_str()).collect::<Vec<_>>().join(" ");
  let traditional_markers = ['國', '氣', '為', '與', '現', '臺', '觀', '園', '應', '這', '來', '對', '後', '發', '會', '說', '還', '華', '風', '電', '體', '畫', '號', '開', '關', '過', '點', '長', '門', '見', '從', '讓', '變', '聽', '樣', '實', '時', '麼', '機', '種', '個', '兩', '無', '國'];
  let simplified_markers = ['国', '气', '为', '与', '现', '台', '观', '园', '应', '这', '来', '对', '后', '发', '会', '说', '还', '华', '风', '电', '体', '画', '号', '开', '关', '过', '点', '长', '门', '见', '从', '让', '变', '听', '样', '实', '时', '么', '机', '种', '个', '两', '无'];
  let traditional = text.chars().filter(|character| traditional_markers.contains(character)).count() + ["國", "氣", "為", "與", "現", "臺", "觀", "園", "應", "這", "來", "對", "後", "發", "會", "說", "還", "華", "風", "電", "體", "畫", "號", "開", "關", "過", "點", "長", "門", "見", "從", "讓", "變", "聽", "樣", "實", "時", "麼", "機", "種", "個", "兩", "無"].iter().map(|marker| text.matches(marker).count()).sum::<usize>();
  let simplified = text.chars().filter(|character| simplified_markers.contains(character)).count() + ["国", "气", "为", "与", "现", "台", "观", "园", "应", "这", "来", "对", "后", "发", "会", "说", "还", "华", "风", "电", "体", "画", "号", "开", "关", "过", "点", "长", "门", "见", "从", "让", "变", "听", "样", "实", "时", "么", "机", "种", "个", "两", "无"].iter().map(|marker| text.matches(marker).count()).sum::<usize>();
  if traditional > simplified {
    return "zt".to_string();
  }
  if simplified > traditional {
    return "zh".to_string();
  }
  if text.chars().any(|character| ('\u{3400}'..='\u{9fff}').contains(&character)) {
    // Ambiguous Han text is safer through the simplified Chinese pivot. A
    // user-provided `chi_tra`/`chi_sim` hint remains authoritative.
    return "zh".to_string();
  }
  if text.chars().any(|character| character.is_alphabetic()) {
    let vietnamese = ['ă', 'â', 'đ', 'ê', 'ô', 'ơ', 'ư', 'Ă', 'Â', 'Đ', 'Ê', 'Ô', 'Ơ', 'Ư'];
    if text.chars().any(|character| vietnamese.contains(&character)) {
      return "vi".to_string();
    }
    return "en".to_string();
  }
  "unknown".to_string()
}

pub fn infer_text_language(text: &str) -> String {
  infer_ocr_source_language(&[OcrRegion { id: "language-evidence".into(), text: text.to_string(), x: 0.0, y: 0.0, width: 1.0, height: 1.0, confidence: Some(1.0) }])
}

fn parse_tsv(tsv: &str, image_width: u32, image_height: u32) -> Vec<OcrRegion> {
  tsv
    .lines()
    .skip(1)
    .enumerate()
    .filter_map(|(index, line)| {
      let fields = line.split('\t').collect::<Vec<_>>();
      if fields.len() < 12 || fields[11].trim().is_empty() {
        return None;
      }
      let left = fields[6].parse::<u32>().ok()?;
      let top = fields[7].parse::<u32>().ok()?;
      let width = fields[8].parse::<u32>().ok()?;
      let height = fields[9].parse::<u32>().ok()?;
      if width == 0 || height == 0 {
        return None;
      }
      let confidence = fields[10].parse::<f32>().ok().filter(|value| value.is_finite() && *value >= 0.0);
      Some(OcrRegion { id: format!("ocr-{}", index + 1), text: fields[11].trim().to_string(), x: left as f32 / image_width as f32, y: top as f32 / image_height as f32, width: width as f32 / image_width as f32, height: height as f32 / image_height as f32, confidence })
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_tsv_to_normalized_regions() {
    let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t100\t50\t200\t80\t92.5\tHello\n";
    let regions = parse_tsv(tsv, 1000, 500);
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].id, "ocr-1");
    assert!((regions[0].x - 0.1).abs() < f32::EPSILON);
    assert!((regions[0].width - 0.2).abs() < f32::EPSILON);
  }

  #[test]
  fn infers_traditional_chinese_without_filename_or_ui_hint() {
    let regions = vec![OcrRegion { id: "r1".into(), text: "陽明山國家公園冷水坑觀景台".into(), x: 0.0, y: 0.0, width: 1.0, height: 0.2, confidence: None }];
    assert_eq!(infer_ocr_source_language(&regions), "zt");
  }

  #[test]
  fn infers_simplified_chinese_without_filename_or_ui_hint() {
    let regions = vec![OcrRegion { id: "r1".into(), text: "因为地理和气候因素".into(), x: 0.0, y: 0.0, width: 1.0, height: 0.2, confidence: None }];
    assert_eq!(infer_ocr_source_language(&regions), "zh");
  }

  #[test]
  fn defaults_latin_ocr_to_english() {
    let regions = vec![OcrRegion { id: "r1".into(), text: "Hello world".into(), x: 0.0, y: 0.0, width: 1.0, height: 0.2, confidence: None }];
    assert_eq!(infer_ocr_source_language(&regions), "en");
  }
}
