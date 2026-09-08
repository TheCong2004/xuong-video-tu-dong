//! ArtCraft-owned engine/resource catalog for CapCut Automation.
//!
//! This module is deliberately a small control-plane boundary.  It never
//! imports or executes CapCap, Python, a cloud provider, or a legacy backend.
//! Engines are usable only after their executable/model files have been
//! installed under an ArtCraft-owned resource root and verified by hash.

use super::capcut_automation_resources::{CapcutResourceResolver, ResourceError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapcutEngineKind {
  Transcription,
  Translation,
  Ocr,
  TextToSpeech,
  SpeakerDiarization,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapcutEngineStatus {
  NotInstalled,
  Ready,
  Unverified,
  LicenseReview,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapcutEngineSpec {
  pub id: String,
  pub kind: CapcutEngineKind,
  pub display_name: String,
  pub executable: Option<String>,
  pub executable_sha256: Option<String>,
  pub executable_size: Option<u64>,
  pub worker_script: Option<String>,
  pub worker_script_sha256: Option<String>,
  pub worker_script_size: Option<u64>,
  pub resource_path: String,
  pub sha256: Option<String>,
  pub size: Option<u64>,
  pub license: String,
  pub status: CapcutEngineStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapcutEngineHealth {
  pub spec: CapcutEngineSpec,
  pub resolved_resource: Option<String>,
  pub resolved_executable: Option<String>,
  pub resolved_worker_script: Option<String>,
  pub error_code: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ResolvedCapcutEngine {
  pub spec: CapcutEngineSpec,
  pub resource_path: PathBuf,
  pub executable_path: Option<PathBuf>,
  pub worker_script_path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapcutModelStatus {
  NotInstalled,
  Downloading,
  Verifying,
  Ready,
  Corrupted,
  LicenseRequired,
  Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapcutModelRecord {
  pub id: String,
  pub capability: CapcutEngineKind,
  pub engine: String,
  pub version: String,
  pub relative_path: String,
  pub download_url: String,
  pub sha256: String,
  pub size: Option<u64>,
  pub license: String,
  pub license_url: String,
  pub required: bool,
  pub status: CapcutModelStatus,
  pub progress: u8,
  pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CapcutAutomationEngineManager {
  install_root: PathBuf,
  resolver: CapcutResourceResolver,
  specs: Vec<CapcutEngineSpec>,
}

impl CapcutAutomationEngineManager {
  pub fn new(packaged_root: Option<PathBuf>, development_root: Option<PathBuf>, appdata_root: PathBuf) -> Self {
    let install_root = appdata_root.join("capcut-automation");
    let model_root = install_root.join("models");
    let _ = fs::create_dir_all(&model_root);
    let specs = default_engine_specs();
    let resolver = CapcutResourceResolver::new(packaged_root, development_root, Some(install_root.clone()));
    Self { install_root, resolver, specs }
  }

  pub fn install_root(&self) -> &Path {
    &self.install_root
  }

  pub fn specs(&self) -> &[CapcutEngineSpec] {
    &self.specs
  }

  pub fn list_models(&self) -> Vec<CapcutModelRecord> {
    default_model_records()
      .into_iter()
      .map(|mut record| {
        let path = self.install_root.join(&record.relative_path);
        record.status = if path.is_file() {
          match Self::verify_artifact(&path, &record.sha256, record.size) {
            Ok(()) => CapcutModelStatus::Ready,
            Err(_) => CapcutModelStatus::Corrupted,
          }
        } else if partial_path(&path).is_file() {
          CapcutModelStatus::Downloading
        } else {
          CapcutModelStatus::NotInstalled
        };
        record
      })
      .collect()
  }

  pub fn model(&self, id: &str) -> Result<CapcutModelRecord, String> {
    self.list_models().into_iter().find(|record| record.id == id).ok_or_else(|| "CAPCUT_MODEL_UNKNOWN".to_string())
  }

  /// Resolve a model through the same packaged/development/AppData resource
  /// resolver used by engine binaries.  Callers must use this boundary rather
  /// than constructing paths or trusting a filename; the digest and size are
  /// checked before the path is returned.
  pub fn resolve_model_path(&self, id: &str) -> Result<(CapcutModelRecord, PathBuf), String> {
    let record = default_model_records().into_iter().find(|record| record.id == id).ok_or_else(|| "CAPCUT_MODEL_UNKNOWN".to_string())?;
    let path = self.resolver.resolve(&record.relative_path, record.size, Some(&record.sha256)).map_err(|_| format!("CAPCUT_MODEL_NOT_READY:{id}"))?;
    Ok((record, path))
  }

  pub async fn download_model(&self, id: &str) -> Result<CapcutModelRecord, String> {
    let record = self.model(id)?;
    let url = reqwest::Url::parse(&record.download_url).map_err(|_| "CAPCUT_MODEL_URL_INVALID".to_string())?;
    if url.scheme() != "https" || !matches!(url.host_str(), Some("huggingface.co" | "argos-net.com" | "argosopentech.com" | "argosopentech.nyc3.digitaloceanspaces.com")) {
      return Err("CAPCUT_MODEL_SOURCE_NOT_ALLOWED".to_string());
    }
    let destination = self.install_root.join(&record.relative_path);
    if let Some(parent) = destination.parent() {
      fs::create_dir_all(parent).map_err(|_| "CAPCUT_MODEL_DIRECTORY_FAILED".to_string())?;
    }
    let partial = partial_path(&destination);
    let response = reqwest::Client::new().get(url).send().await.map_err(|_| "CAPCUT_MODEL_DOWNLOAD_FAILED".to_string())?.error_for_status().map_err(|_| "CAPCUT_MODEL_DOWNLOAD_FAILED".to_string())?;
    let mut file = tokio::fs::File::create(&partial).await.map_err(|_| "CAPCUT_MODEL_PARTIAL_CREATE_FAILED".to_string())?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
      let chunk = chunk.map_err(|_| "CAPCUT_MODEL_DOWNLOAD_FAILED".to_string())?;
      file.write_all(&chunk).await.map_err(|_| "CAPCUT_MODEL_PARTIAL_WRITE_FAILED".to_string())?;
    }
    file.flush().await.map_err(|_| "CAPCUT_MODEL_PARTIAL_WRITE_FAILED".to_string())?;
    drop(file);
    Self::verify_artifact(&partial, &record.sha256, record.size).map_err(|_| "CAPCUT_MODEL_VERIFY_FAILED".to_string())?;
    fs::rename(&partial, &destination).map_err(|_| "CAPCUT_MODEL_ACTIVATE_FAILED".to_string())?;
    self.model(id)
  }

  /// Activate an already downloaded artifact through the same verification
  /// and atomic rename boundary as the network downloader. This is used by
  /// staging tools so a checked download cannot be copied around the runtime
  /// without digest/size verification.
  pub fn stage_verified_model(&self, id: &str, source: &Path) -> Result<CapcutModelRecord, String> {
    let record = default_model_records().into_iter().find(|record| record.id == id).ok_or_else(|| "CAPCUT_MODEL_UNKNOWN".to_string())?;
    Self::verify_artifact(source, &record.sha256, record.size).map_err(|_| "CAPCUT_MODEL_VERIFY_FAILED".to_string())?;
    let destination = self.install_root.join(&record.relative_path);
    if let Some(parent) = destination.parent() {
      fs::create_dir_all(parent).map_err(|_| "CAPCUT_MODEL_DIRECTORY_FAILED".to_string())?;
    }
    let partial = partial_path(&destination);
    fs::copy(source, &partial).map_err(|_| "CAPCUT_MODEL_PARTIAL_WRITE_FAILED".to_string())?;
    Self::verify_artifact(&partial, &record.sha256, record.size).map_err(|_| "CAPCUT_MODEL_VERIFY_FAILED".to_string())?;
    fs::rename(&partial, &destination).map_err(|_| "CAPCUT_MODEL_ACTIVATE_FAILED".to_string())?;
    self.model(id)
  }

  pub fn verify_model(&self, id: &str) -> Result<CapcutModelRecord, String> {
    let record = self.model(id)?;
    let path = self.install_root.join(&record.relative_path);
    Self::verify_artifact(&path, &record.sha256, record.size).map_err(|_| "CAPCUT_MODEL_VERIFY_FAILED".to_string())?;
    Ok(CapcutModelRecord { status: CapcutModelStatus::Ready, error: None, progress: 100, ..record })
  }

  pub fn remove_model(&self, id: &str) -> Result<(), String> {
    let record = self.model(id)?;
    let path = self.install_root.join(&record.relative_path);
    if path.exists() {
      fs::remove_file(path).map_err(|_| "CAPCUT_MODEL_REMOVE_FAILED".to_string())?;
    }
    Ok(())
  }

  pub fn health(&self) -> Vec<CapcutEngineHealth> {
    self.specs.iter().map(|spec| self.check_spec(spec)).collect()
  }

  pub fn check(&self, kind: CapcutEngineKind) -> Vec<CapcutEngineHealth> {
    self.specs.iter().filter(|spec| spec.kind == kind).map(|spec| self.check_spec(spec)).collect()
  }

  pub fn resolve_first(&self, kind: CapcutEngineKind) -> Result<ResolvedCapcutEngine, String> {
    for spec in self.specs.iter().filter(|spec| spec.kind == kind) {
      let resource = self.resolver.resolve(&spec.resource_path, spec.size, spec.sha256.as_deref());
      let executable = match spec.executable.as_deref() {
        Some(path) => self.resolver.resolve(path, spec.executable_size, spec.executable_sha256.as_deref()).map(Some),
        None => Ok(None),
      };
      let worker_script = match spec.worker_script.as_deref() {
        Some(path) => match self.resolver.resolve(path, spec.worker_script_size, spec.worker_script_sha256.as_deref()) {
          Ok(path) => Ok(Some(path)),
          Err(error) if spec.id == "argos-translate" || spec.id == "rapidocr-onnx" => self.resolve_tracked_worker(spec).map(Some).or(Err(error)),
          Err(error) => Err(error),
        },
        None => Ok(None),
      };
      if let (Ok(resource_path), Ok(executable_path), Ok(worker_script_path)) = (resource, executable, worker_script) {
        if spec.id == "sherpa-onnx-diarization" && self.resolver.resolve("models/diarization/embedding.onnx", None, None).is_err() {
          continue;
        }
        return Ok(ResolvedCapcutEngine { spec: spec.clone(), resource_path, executable_path, worker_script_path });
      }
    }
    Err(format!("CAPCUT_ENGINE_NOT_READY:{kind:?}"))
  }

  /// In a source checkout, prefer the tracked worker when an ignored/stale
  /// resource copy is present.  Packaged/AppData roots remain authoritative
  /// when they contain the same verified digest; this fallback only prevents
  /// `cargo tauri dev` from silently running an older ignored copy.
  fn resolve_tracked_worker(&self, spec: &CapcutEngineSpec) -> Result<PathBuf, ResourceError> {
    let worker_name = spec.worker_script.as_deref().and_then(|path| Path::new(path).file_name()).ok_or_else(|| ResourceError::Missing(spec.worker_script.clone().unwrap_or_default()))?;
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tools/capcut-automation").join(worker_name);
    let canonical = fs::canonicalize(&source).map_err(|_| ResourceError::Missing(spec.worker_script.clone().unwrap_or_default()))?;
    if !canonical.is_file() {
      return Err(ResourceError::Missing(canonical.display().to_string()));
    }
    let metadata = fs::metadata(&canonical).map_err(|_| ResourceError::Missing(canonical.display().to_string()))?;
    if spec.worker_script_size.is_some_and(|expected| expected != metadata.len()) {
      return Err(ResourceError::SizeMismatch { expected: spec.worker_script_size.unwrap_or_default(), actual: metadata.len() });
    }
    let bytes = fs::read(&canonical).map_err(|_| ResourceError::Missing(canonical.display().to_string()))?;
    let actual = hex_encode(&Sha256::digest(bytes));
    if spec.worker_script_sha256.as_deref().is_some_and(|expected| !expected.eq_ignore_ascii_case(&actual)) {
      return Err(ResourceError::HashMismatch { expected: spec.worker_script_sha256.clone().unwrap_or_default(), actual });
    }
    Ok(canonical)
  }

  fn check_spec(&self, spec: &CapcutEngineSpec) -> CapcutEngineHealth {
    let resolved_resource = self.resolver.resolve(&spec.resource_path, spec.size, spec.sha256.as_deref()).ok();
    let resolved_executable = spec.executable.as_deref().and_then(|path| self.resolver.resolve(path, spec.executable_size, spec.executable_sha256.as_deref()).ok()).map(|path| path.display().to_string());
    let resolved_worker_script = spec.worker_script.as_deref().and_then(|path| self.resolver.resolve(path, spec.worker_script_size, spec.worker_script_sha256.as_deref()).ok().or_else(|| (spec.id == "argos-translate" || spec.id == "rapidocr-onnx").then(|| self.resolve_tracked_worker(spec).ok()).flatten())).map(|path| path.display().to_string());
    let error_code = if resolved_resource.is_none() {
      Some("CAPCUT_ENGINE_RESOURCE_MISSING_OR_UNVERIFIED".to_string())
    } else if spec.executable.is_some() && resolved_executable.is_none() {
      Some("CAPCUT_ENGINE_EXECUTABLE_MISSING".to_string())
    } else if spec.worker_script.is_some() && resolved_worker_script.is_none() {
      Some("CAPCUT_ENGINE_WORKER_SCRIPT_MISSING".to_string())
    } else if spec.id == "sherpa-onnx-diarization" && self.resolver.resolve("models/diarization/embedding.onnx", None, None).is_err() {
      Some("CAPCUT_SPEAKER_EMBEDDING_MODEL_MISSING".to_string())
    } else if spec.status == CapcutEngineStatus::LicenseReview {
      Some("CAPCUT_ENGINE_LICENSE_REVIEW_REQUIRED".to_string())
    } else {
      None
    };
    let mut effective_spec = spec.clone();
    if error_code.is_none() && effective_spec.status == CapcutEngineStatus::NotInstalled {
      effective_spec.status = CapcutEngineStatus::Ready;
    }
    CapcutEngineHealth { spec: effective_spec, resolved_resource: resolved_resource.map(|path| path.display().to_string()), resolved_executable, resolved_worker_script, error_code }
  }

  /// Registers a downloaded artifact only after it has been copied into the
  /// ArtCraft-owned directory and its digest is known.  The caller still has
  /// to obtain and record the upstream license separately.
  pub fn verify_artifact(path: &Path, expected_sha256: &str, expected_size: Option<u64>) -> Result<(), ResourceError> {
    let metadata = fs::metadata(path).map_err(|_| ResourceError::Missing(path.display().to_string()))?;
    if let Some(expected) = expected_size {
      if metadata.len() != expected {
        return Err(ResourceError::SizeMismatch { expected, actual: metadata.len() });
      }
    }
    let bytes = fs::read(path).map_err(|_| ResourceError::Missing(path.display().to_string()))?;
    let actual = hex_encode(&Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected_sha256) {
      return Err(ResourceError::HashMismatch { expected: expected_sha256.to_string(), actual });
    }
    Ok(())
  }
}

fn partial_path(path: &Path) -> PathBuf {
  PathBuf::from(format!("{}.partial", path.display()))
}

fn default_engine_specs() -> Vec<CapcutEngineSpec> {
  vec![
    CapcutEngineSpec { id: "whisper-cpp".into(), kind: CapcutEngineKind::Transcription, display_name: "Whisper.cpp (local CPU)".into(), executable: Some("bin/whisper-cli.exe".into()), executable_sha256: Some("31513eef3a9721d377544e10edfb7b27f4533ca84b71c8b87764da4cdf36da18".into()), executable_size: Some(489_984), worker_script: None, worker_script_sha256: None, worker_script_size: None, resource_path: "models/whisper/ggml-tiny.bin".into(), sha256: Some("be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21".into()), size: Some(77_691_713), license: "MIT + model card terms".into(), status: CapcutEngineStatus::NotInstalled },
    CapcutEngineSpec { id: "argos-translate".into(), kind: CapcutEngineKind::Translation, display_name: "Argos Translate (offline)".into(), executable: Some("runtime/python314/python.exe".into()), executable_sha256: Some("03168c01b7b7491423350e82c26fee71f35b43694d1319d3c668bda6903a0c38".into()), executable_size: Some(106_208), worker_script: Some("scripts/translation_worker.py".into()), worker_script_sha256: Some("f9b6dd09bdb75957718b293dbca95cf43463fa4d3f33c8879c0c8c3bd74ecb57".into()), worker_script_size: Some(8_588), resource_path: "models/argos/translate-en_vi-1_9.argosmodel".into(), sha256: Some("86957101aa4099aa9a1a7492e41987d938d3cf0fdaf4fb684c0797a9d567dd16".into()), size: Some(67_770_159), license: "MIT engine; OPUS-MT model CC-BY 4.0".into(), status: CapcutEngineStatus::NotInstalled },
    CapcutEngineSpec { id: "rapidocr-onnx".into(), kind: CapcutEngineKind::Ocr, display_name: "RapidOCR ONNX (local)".into(), executable: Some("runtime/python314/python.exe".into()), executable_sha256: Some("03168c01b7b7491423350e82c26fee71f35b43694d1319d3c668bda6903a0c38".into()), executable_size: Some(106_208), worker_script: Some("scripts/ocr_rapid_worker.py".into()), worker_script_sha256: Some("af40f798a214d311c19f8b48a62fe7a9658d895496654c0434374d7e5b0c57e0".into()), worker_script_size: Some(4_376), resource_path: "python/rapidocr_onnxruntime/models/ch_PP-OCRv3_det_infer.onnx".into(), sha256: Some("3439588c030faea393a54515f51e983d8e155b19a2e8aba7891934c1cf0de526".into()), size: Some(2_432_880), license: "Apache-2.0 engine/models".into(), status: CapcutEngineStatus::NotInstalled },
    CapcutEngineSpec { id: "piper".into(), kind: CapcutEngineKind::TextToSpeech, display_name: "Piper (giọng Việt ngoại tuyến)".into(), executable: Some("bin/piper.exe".into()), executable_sha256: Some("96f3da3811151580073e40bb4dd20eb0fb8115f5f5f76e2fb54282b3edfa5c1f".into()), executable_size: Some(509_952), worker_script: None, worker_script_sha256: None, worker_script_size: None, resource_path: "voices/piper/vi_VN-vivos-x_low/vi_VN-vivos-x_low.onnx".into(), sha256: Some("6ab13374eb0862021a545befe7727aef59e16117f1c075aa9e0362237ecc98ae".into()), size: Some(27_789_413), license: "Piper MIT; VIVOS voice model terms".into(), status: CapcutEngineStatus::NotInstalled },
    CapcutEngineSpec { id: "sherpa-onnx-diarization".into(), kind: CapcutEngineKind::SpeakerDiarization, display_name: "Sherpa-ONNX diarization (offline)".into(), executable: Some("runtime/python314/python.exe".into()), executable_sha256: Some("03168c01b7b7491423350e82c26fee71f35b43694d1319d3c668bda6903a0c38".into()), executable_size: Some(106_208), worker_script: Some("scripts/speaker_diarization_worker.py".into()), worker_script_sha256: Some("94ea11fb192cea7300faf4bef91edd6f3891ef97eef2d05bdf88b4f33c5dbfd5".into()), worker_script_size: Some(2_481), resource_path: "models/diarization/segmentation/model.onnx".into(), sha256: Some("220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079".into()), size: Some(5_992_913), license: "Sherpa-ONNX Apache-2.0; model-specific terms".into(), status: CapcutEngineStatus::NotInstalled },
  ]
}

fn default_model_records() -> Vec<CapcutModelRecord> {
  vec![
    CapcutModelRecord { id: "whisper-cpp-ggml-tiny".into(), capability: CapcutEngineKind::Transcription, engine: "whisper.cpp".into(), version: "main-ggml-tiny".into(), relative_path: "models/whisper/ggml-tiny.bin".into(), download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin?download=true".into(), sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21".into(), size: Some(77_691_713), license: "MIT source; OpenAI Whisper model card terms".into(), license_url: "https://huggingface.co/ggerganov/whisper.cpp".into(), required: false, status: CapcutModelStatus::NotInstalled, progress: 0, error: None },
    CapcutModelRecord { id: "argos-en-vi-1-9".into(), capability: CapcutEngineKind::Translation, engine: "Argos Translate".into(), version: "1.9".into(), relative_path: "models/argos/translate-en_vi-1_9.argosmodel".into(), download_url: "https://argos-net.com/v1/translate-en_vi-1_9.argosmodel".into(), sha256: "86957101aa4099aa9a1a7492e41987d938d3cf0fdaf4fb684c0797a9d567dd16".into(), size: Some(67_770_159), license: "OPUS-MT model CC-BY 4.0".into(), license_url: "https://github.com/argosopentech/argospm-index".into(), required: false, status: CapcutModelStatus::NotInstalled, progress: 0, error: None },
    CapcutModelRecord { id: "argos-zt-en-1-9".into(), capability: CapcutEngineKind::Translation, engine: "Argos Translate".into(), version: "1.9".into(), relative_path: "models/argos/translate-zt_en-1_9.argosmodel".into(), download_url: "https://argos-net.com/v1/translate-zt_en-1_9.argosmodel".into(), sha256: "a9ea826d801f059dd47f1f47b6c1dc9d762519a80ea3afd2a982130a42a25486".into(), size: Some(74_498_034), license: "OPUS-MT model CC-BY 4.0".into(), license_url: "https://github.com/argosopentech/argospm-index".into(), required: false, status: CapcutModelStatus::NotInstalled, progress: 0, error: None },
    CapcutModelRecord { id: "argos-zh-en-1-9".into(), capability: CapcutEngineKind::Translation, engine: "Argos Translate".into(), version: "1.9".into(), relative_path: "models/argos/translate-zh_en-1_9.argosmodel".into(), download_url: "https://argos-net.com/v1/translate-zh_en-1_9.argosmodel".into(), sha256: "62e7af5a3a48b530e47b7b3e5c78c2de79073ecd815750d2bf3ab35b4a67da2d".into(), size: Some(74_481_402), license: "OPUS-MT model CC-BY 4.0".into(), license_url: "https://github.com/argosopentech/argospm-index".into(), required: false, status: CapcutModelStatus::NotInstalled, progress: 0, error: None },
    CapcutModelRecord { id: "tesseract-eng-fast".into(), capability: CapcutEngineKind::Ocr, engine: "Tesseract OCR".into(), version: "tessdata_fast-eng".into(), relative_path: "models/tesseract/eng.traineddata".into(), download_url: "https://github.com/tesseract-ocr/tessdata_fast/raw/main/eng.traineddata".into(), sha256: "7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2".into(), size: Some(4_113_088), license: "Apache-2.0 + language data terms".into(), license_url: "https://github.com/tesseract-ocr/tessdata_fast".into(), required: false, status: CapcutModelStatus::NotInstalled, progress: 0, error: None },
    CapcutModelRecord { id: "tesseract-chi-tra-fast".into(), capability: CapcutEngineKind::Ocr, engine: "Tesseract OCR".into(), version: "tessdata_fast-chi_tra".into(), relative_path: "models/tesseract/chi_tra.traineddata".into(), download_url: "https://github.com/tesseract-ocr/tessdata_fast/raw/main/chi_tra.traineddata".into(), sha256: "529c5b5797d64b126065cd55f2bb4c7fd7b15790798091b1ff259941a829330b".into(), size: Some(2_366_642), license: "Apache-2.0 + language data terms".into(), license_url: "https://github.com/tesseract-ocr/tessdata_fast".into(), required: false, status: CapcutModelStatus::NotInstalled, progress: 0, error: None },
    CapcutModelRecord { id: "tesseract-chi-sim-fast".into(), capability: CapcutEngineKind::Ocr, engine: "Tesseract OCR".into(), version: "tessdata_fast-chi_sim".into(), relative_path: "models/tesseract/chi_sim.traineddata".into(), download_url: "https://github.com/tesseract-ocr/tessdata_fast/raw/main/chi_sim.traineddata".into(), sha256: "a5fcb6f0db1e1d6d8522f39db4e848f05984669172e584e8d76b6b3141e1f730".into(), size: Some(2_469_156), license: "Apache-2.0 + language data terms".into(), license_url: "https://github.com/tesseract-ocr/tessdata_fast".into(), required: false, status: CapcutModelStatus::NotInstalled, progress: 0, error: None },
  ]
}

fn hex_encode(bytes: &[u8]) -> String {
  const HEX: &[u8; 16] = b"0123456789abcdef";
  bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0f) as usize] as char);
    out
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_catalog_is_local_and_does_not_reference_capcap() {
    let specs = default_engine_specs();
    assert_eq!(specs.len(), 5);
    assert!(specs.iter().all(|spec| !spec.resource_path.to_ascii_lowercase().contains("capcap")));
    assert!(specs.iter().any(|spec| spec.id == "whisper-cpp"));
  }

  #[test]
  fn artifact_verification_rejects_wrong_digest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("artifact.bin");
    fs::write(&path, b"local").unwrap();
    assert!(matches!(CapcutAutomationEngineManager::verify_artifact(&path, "00", Some(5)), Err(ResourceError::HashMismatch { .. })));
  }

  #[test]
  fn tracked_translation_worker_matches_catalog_digest_and_size() {
    let spec = default_engine_specs().into_iter().find(|spec| spec.id == "argos-translate").unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tools/capcut-automation/translation_worker.py");
    assert!(CapcutAutomationEngineManager::verify_artifact(&path, spec.worker_script_sha256.as_deref().unwrap(), spec.worker_script_size).is_ok());
  }
}
