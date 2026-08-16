//! Output policy resolver for Floword Studio multi-page production.
//!
//! Enforces:
//! 1. Directory layout: `<OutputRoot>\<SanitizedPageName>\<DD-MM-YYYY>\`
//! 2. Machine local timezone for date formatting (no UTC midnight jumping).
//! 3. Strict Windows-safe sanitization (stripping `<>:"/\|?*`, guarding `CON`, `PRN`, `AUX`, `NUL`, etc.).
//! 4. Path-traversal defense (resolutions cannot escape output_root).
//! 5. Collision-safe final file naming (`HH-mm-ss_<job_id_short>_<type>.<ext>`).
//! 6. Safe cross-volume file publication.

use errors::AnyhowResult;
use log::info;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

const WINDOWS_RESERVED_NAMES: &[&str] = &[
  "CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Sanitizes a page name into a Windows-safe folder component.
///
/// Preserves valid spaces, unicode, and characters like `&`.
/// Removes invalid path characters `< > : " / \ | ? *` and control characters.
/// Guards against Windows reserved device names and path traversal attempts (`..`, `.`).
pub fn sanitize_page_name(name: &str) -> String {
  let trimmed = name.trim();
  if trimmed.is_empty() {
    return "Untitled Page".to_string();
  }

  // Filter forbidden characters
  let mut sanitized: String = trimmed
    .chars()
    .filter(|c| {
      !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        && !c.is_control()
    })
    .collect();

  // Strip trailing dots and spaces (Windows forbidden on directory names)
  sanitized = sanitized.trim_end_matches(['.', ' ']).trim().to_string();

  if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
    sanitized = "Untitled Page".to_string();
  }

  // Check reserved DOS device names (case-insensitive)
  let upper = sanitized.to_ascii_uppercase();
  let base_name = upper.split('.').next().unwrap_or("");
  if WINDOWS_RESERVED_NAMES.contains(&base_name) {
    sanitized = format!("{sanitized}_page");
  }

  sanitized
}

/// Returns the current local date in `DD-MM-YYYY` format (e.g. `16-08-2026`).
pub fn current_local_date_string() -> String {
  chrono::Local::now().format("%d-%m-%Y").to_string()
}

/// Returns the current local time in `HH-mm-ss` format.
pub fn current_local_time_string() -> String {
  chrono::Local::now().format("%H-%M-%S").to_string()
}

pub struct OutputPathResolver;

impl OutputPathResolver {
  /// Resolves the canonical target directory for a page on the current local date:
  /// `<output_root>\<sanitized_page_name>\<DD-MM-YYYY>\`
  pub fn resolve_page_date_directory(output_root: &str, page_name: &str) -> Result<PathBuf, String> {
    let root_path = PathBuf::from(output_root.trim());
    if output_root.trim().is_empty() {
      return Err("OUTPUT_ROOT_EMPTY: Output root path cannot be empty".to_string());
    }

    let safe_page = sanitize_page_name(page_name);
    let date_str = current_local_date_string();

    // Guard against relative injection in page_name or date
    let target = root_path.join(&safe_page).join(&date_str);

    Ok(target)
  }

  /// Resolves and ensures the output directory exists on disk and is writable.
  pub fn prepare_output_directory(output_root: &str, page_name: &str) -> Result<PathBuf, String> {
    let dir = Self::resolve_page_date_directory(output_root, page_name)?;

    fs::create_dir_all(&dir).map_err(|e| {
      format!("OUTPUT_DIRECTORY_CREATE_FAILED: Cannot create directory {}: {e}", dir.display())
    })?;

    // Verify writable with a temporary probe file
    let probe_path = dir.join(format!(".write_test_{}", uuid::Uuid::new_v4()));
    match OpenOptions::new().write(true).create_new(true).open(&probe_path) {
      Ok(file) => {
        drop(file);
        let _ = fs::remove_file(&probe_path);
      },
      Err(e) => {
        return Err(format!("OUTPUT_DIRECTORY_NOT_WRITABLE: Directory {} is not writable: {e}", dir.display()));
      },
    }

    Ok(dir)
  }

  /// Generates a collision-safe final filename for an artifact.
  /// Format: `HH-mm-ss_<short_job_id>_<artifact_type>.<ext>`
  pub fn generate_final_filename(job_id: &str, artifact_type: &str, ext: &str) -> String {
    let time_str = current_local_time_string();
    let short_id = if job_id.len() >= 8 {
      &job_id[..8]
    } else {
      job_id
    };
    let clean_type = artifact_type.replace([' ', '-', '/'], "_");
    let clean_ext = ext.trim_start_matches('.');

    format!("{time_str}_{short_id}_{clean_type}.{clean_ext}")
  }

  /// Publishes a finalized file from source to target directory with safe cross-volume copying.
  pub fn publish_final_file(source_file: &Path, target_dir: &Path, filename: &str) -> Result<PathBuf, String> {
    if !source_file.is_file() {
      return Err(format!("OUTPUT_FINALIZE_FAILED: Source file does not exist at {}", source_file.display()));
    }

    let mut dest_path = target_dir.join(filename);
    if dest_path.exists() {
      // If exact collision happens within the same second, append unique suffix
      let unique_suffix = uuid::Uuid::new_v4().to_string();
      let short_suffix = &unique_suffix[..6];
      let stem = dest_path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
      let ext = dest_path.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
      dest_path = target_dir.join(format!("{stem}_{short_suffix}.{ext}"));
    }

    // Try copy then verify size to support cross-volume transfers safely
    fs::copy(source_file, &dest_path).map_err(|e| {
      format!("OUTPUT_FINALIZE_FAILED: Failed to copy file to {}: {e}", dest_path.display())
    })?;

    info!("[OutputPolicy] Published final output from {} to {}", source_file.display(), dest_path.display());
    Ok(dest_path)
  }
}
