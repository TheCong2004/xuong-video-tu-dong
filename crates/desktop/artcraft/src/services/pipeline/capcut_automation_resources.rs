//! ArtCraft-owned resource resolution for CapCut Automation.
//!
//! The resolver intentionally accepts only a relative resource identifier and
//! searches ArtCraft-owned roots.  It has no fallback to the legacy CapCap
//! tree, environment variables, or arbitrary absolute paths.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};

fn hex_encode(bytes: &[u8]) -> String {
  const HEX: &[u8; 16] = b"0123456789abcdef";
  bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0f) as usize] as char);
    out
  })
}

#[derive(Clone, Debug)]
pub struct CapcutResourceResolver {
  roots: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResourceError {
  InvalidRelativePath,
  Missing(String),
  OutsideOwnedRoot(String),
  SymlinkNotAllowed(String),
  SizeMismatch { expected: u64, actual: u64 },
  HashMismatch { expected: String, actual: String },
}

impl std::fmt::Display for ResourceError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::InvalidRelativePath => write!(f, "CAPCUT_RESOURCE_INVALID_PATH"),
      Self::Missing(id) => write!(f, "CAPCUT_RESOURCE_MISSING:{id}"),
      Self::OutsideOwnedRoot(id) => write!(f, "CAPCUT_RESOURCE_OUTSIDE_ARTCRAFT:{id}"),
      Self::SymlinkNotAllowed(id) => write!(f, "CAPCUT_RESOURCE_SYMLINK_NOT_ALLOWED:{id}"),
      Self::SizeMismatch { expected, actual } => write!(f, "CAPCUT_RESOURCE_SIZE_MISMATCH:{expected}:{actual}"),
      Self::HashMismatch { expected, actual } => write!(f, "CAPCUT_RESOURCE_HASH_MISMATCH:{expected}:{actual}"),
    }
  }
}

impl std::error::Error for ResourceError {}

impl CapcutResourceResolver {
  pub fn new(packaged_root: Option<PathBuf>, development_root: Option<PathBuf>, appdata_root: Option<PathBuf>) -> Self {
    // Prefer the verified per-user runtime for executable dependencies.  A
    // development/package resource tree may contain the worker and ONNX model
    // while intentionally omitting Python site-packages; selecting its bare
    // interpreter first makes RapidOCR fail with ModuleNotFoundError.  The
    // app-data runtime is the install root whose interpreter has the staged
    // offline modules, while every artifact is still hash/size checked below.
    Self { roots: [appdata_root, packaged_root, development_root].into_iter().flatten().filter_map(|root| fs::canonicalize(root).ok()).collect() }
  }

  pub fn resolve(&self, relative_path: &str, expected_size: Option<u64>, expected_sha256: Option<&str>) -> Result<PathBuf, ResourceError> {
    let relative = Path::new(relative_path);
    if relative.is_absolute() || relative_path.trim().is_empty() || relative.components().any(|component| matches!(component, Component::ParentDir | Component::Prefix(_))) {
      return Err(ResourceError::InvalidRelativePath);
    }
    // A stale artifact in one owned root must not mask a verified artifact in
    // another owned root (for example an older AppData copy ahead of the
    // development/resource tree).  Keep the last verification error for a
    // useful final diagnostic, but continue searching all ArtCraft roots.
    let mut last_error: Option<ResourceError> = None;
    for root in &self.roots {
      let candidate = root.join(relative);
      if !candidate.is_file() {
        continue;
      }
      let canonical = match fs::canonicalize(&candidate) {
        Ok(path) => path,
        Err(_) => {
          last_error = Some(ResourceError::Missing(relative_path.to_string()));
          continue;
        },
      };
      if !canonical.starts_with(root) {
        return Err(ResourceError::OutsideOwnedRoot(relative_path.to_string()));
      }
      let metadata = match fs::symlink_metadata(&candidate) {
        Ok(metadata) => metadata,
        Err(_) => {
          last_error = Some(ResourceError::Missing(relative_path.to_string()));
          continue;
        },
      };
      if metadata.file_type().is_symlink() {
        last_error = Some(ResourceError::SymlinkNotAllowed(relative_path.to_string()));
        continue;
      }
      if let Some(expected) = expected_size {
        let actual = metadata.len();
        if actual != expected {
          last_error = Some(ResourceError::SizeMismatch { expected, actual });
          continue;
        }
      }
      if let Some(expected) = expected_sha256 {
        let bytes = match fs::read(&canonical) {
          Ok(bytes) => bytes,
          Err(_) => {
            last_error = Some(ResourceError::Missing(relative_path.to_string()));
            continue;
          },
        };
        let actual = hex_encode(&Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(expected) {
          last_error = Some(ResourceError::HashMismatch { expected: expected.to_string(), actual });
          continue;
        }
      }
      return Ok(canonical);
    }
    Err(last_error.unwrap_or_else(|| ResourceError::Missing(relative_path.to_string())))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn resolver_rejects_absolute_and_parent_paths() {
    let resolver = CapcutResourceResolver::new(None, None, None);
    assert_eq!(resolver.resolve(r"D:\\CapCap\\models\\x", None, None), Err(ResourceError::InvalidRelativePath));
    assert_eq!(resolver.resolve("../legacy/models/x", None, None), Err(ResourceError::InvalidRelativePath));
  }

  #[test]
  fn resolver_validates_hash_and_size_inside_owned_root() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("models").join("demo.bin");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, b"demo").unwrap();
    let resolver = CapcutResourceResolver::new(Some(dir.path().to_path_buf()), None, None);
    let hash = hex_encode(&Sha256::digest(b"demo"));
    assert!(resolver.resolve("models/demo.bin", Some(4), Some(&hash)).is_ok());
    assert!(matches!(resolver.resolve("models/demo.bin", Some(5), Some(&hash)), Err(ResourceError::SizeMismatch { .. })));
  }

  #[test]
  fn resolver_skips_stale_owned_root_and_uses_verified_fallback() {
    let stale = tempfile::tempdir().unwrap();
    let verified = tempfile::tempdir().unwrap();
    let stale_file = stale.path().join("scripts").join("worker.py");
    let verified_file = verified.path().join("scripts").join("worker.py");
    fs::create_dir_all(stale_file.parent().unwrap()).unwrap();
    fs::create_dir_all(verified_file.parent().unwrap()).unwrap();
    fs::write(&stale_file, b"old-worker").unwrap();
    fs::write(&verified_file, b"new-worker").unwrap();
    let hash = hex_encode(&Sha256::digest(b"new-worker"));
    let resolver = CapcutResourceResolver::new(Some(stale.path().to_path_buf()), Some(verified.path().to_path_buf()), None);
    assert_eq!(resolver.resolve("scripts/worker.py", Some(10), Some(&hash)).unwrap(), fs::canonicalize(verified_file).unwrap());
  }
}
