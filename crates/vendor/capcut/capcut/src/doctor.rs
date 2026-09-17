use std::path::{Path, PathBuf};
use serde::Serialize;

/// Mirrors `reference/src/doctor.ts` checks in Rust.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    /// Whether a CapCut/JianYing draft store appears to exist on this host.
    pub capcut_installed: bool,
    /// Resolved draft store path (CAPCUT_DRAFT_DIR override or default location).
    pub draft_store_path: Option<PathBuf>,
    pub ffmpeg_available: bool,
    pub ffprobe_available: bool,
    /// Human-readable issues (warnings). Empty means healthy.
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnoseReport {
    pub doctor: DoctorReport,
    pub draft: String,
    pub discover: DiagnoseDiscover,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnoseDiscover {
    pub ok: bool,
    pub layout: Option<String>,
    pub version: Option<String>,
    pub diverged: Option<bool>,
    pub error: Option<String>,
}

fn on_path(cmd: &str) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    #[cfg(windows)]
    let exts: &[&str] = &["", ".exe", ".cmd", ".bat"];
    #[cfg(not(windows))]
    let exts: &[&str] = &[""];
    for dir in std::env::split_paths(&path_var) {
        for ext in exts {
            let candidate = dir.join(format!("{}{}", cmd, ext));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn diagnose_impl() -> DoctorReport {
    let mut issues = Vec::new();

    let draft_store_path = crate::store::default_drafts_dir();
    let capcut_installed = draft_store_path
        .as_ref()
        .map(|p| p.exists())
        .unwrap_or(false);

    if let Some(p) = &draft_store_path {
        if !p.exists() {
            issues.push(format!(
                "draft store not found at {} — open a project in CapCut/JianYing once, or set CAPCUT_DRAFT_DIR",
                p.display()
            ));
        } else {
            let candidates = crate::store::draft_dir_candidates();
            let env_override = std::env::var("CAPCUT_DRAFT_DIR")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if env_override.is_none() && !candidates.is_empty() {
                let any_exists = candidates.iter().any(|c| c.path.exists());
                if !any_exists {
                    issues.push(format!(
                        "none of the default draft dirs exist (checked {} candidates)",
                        candidates.len()
                    ));
                }
            }
            if p.is_dir() {
                let has_candidate = crate::store::STANDARD_FILES
                    .iter()
                    .any(|f| p.join(f).exists());
                if has_candidate {
                    if let Err(e) = crate::store::discover_draft_store(p) {
                        issues.push(format!("draft store layout issue at {}: {}", p.display(), e));
                    }
                }
            }
        }
    } else {
        issues.push("no default draft dir for this OS — pass draft path explicitly".to_string());
    }

    if std::env::var("CAPCUT_DRAFT_DIR").ok().map(|s| s.trim().is_empty()).unwrap_or(false) {
        issues.push("CAPCUT_DRAFT_DIR is set but empty".to_string());
    }

    let ffmpeg = on_path("ffmpeg");
    let ffprobe = on_path("ffprobe");
    let ffmpeg_available = ffmpeg.is_some();
    let ffprobe_available = ffprobe.is_some();

    if ffmpeg.is_none() {
        issues.push("ffmpeg not found on PATH — rendering/probing will fail; install ffmpeg or pass --ffmpeg-cmd".to_string());
    }
    if ffprobe.is_none() {
        issues.push("ffprobe not found on PATH — media duration/dimensions cannot be auto-detected; install ffmpeg (includes ffprobe)".to_string());
    }

    DoctorReport {
        capcut_installed,
        draft_store_path,
        ffmpeg_available,
        ffprobe_available,
        issues,
    }
}

/// No-arg doctor check (used by `capcut doctor`).
pub fn check() -> DoctorReport {
    diagnose_impl()
}

/// Alias for task spec: `diagnose() -> DoctorReport` (spec requires this name).
pub fn diagnose() -> DoctorReport {
    diagnose_impl()
}

/// Draft-specific diagnose expected by CLI `diagnose <draft>` command.
pub fn diagnose_with_draft(draft: &Path) -> DiagnoseReport {
    let doctor = diagnose_impl();
    let discover = match crate::store::discover_draft_store(draft) {
        Ok(store) => DiagnoseDiscover {
            ok: true,
            layout: Some(store.layout.to_string()),
            version: store.version.clone(),
            diverged: Some(store.diverged),
            error: None,
        },
        Err(e) => DiagnoseDiscover {
            ok: false,
            layout: None,
            version: None,
            diverged: None,
            error: Some(format!("{:#}", e)),
        },
    };
    DiagnoseReport {
        doctor,
        draft: draft.display().to_string(),
        discover,
    }
}
