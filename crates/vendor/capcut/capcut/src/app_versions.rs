use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::draft::Draft;
use crate::version::{AppSource, assess_write_safety};

// ---------------------------------------------------------------------------
// Constants / types
// ---------------------------------------------------------------------------

pub const APP_VERSIONS_ENV: &str = "CAPCUT_CLI_APP_VERSIONS";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppVersionEvidence {
    pub app_source: AppSource,
    pub app_version: Option<String>,
    pub schema_int: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppVersionRecord {
    pub app_source: AppSource,
    pub app_version: Option<String>,
    pub schema_int: Option<i64>,
    pub seen_at: String,
}

impl From<AppVersionRecord> for AppVersionEvidence {
    fn from(r: AppVersionRecord) -> Self {
        AppVersionEvidence {
            app_source: r.app_source,
            app_version: r.app_version,
            schema_int: r.schema_int,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppVersionDrift {
    pub store_dir: String,
    pub from: AppVersionRecord,
    pub to: AppVersionEvidence,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppVersionsFile {
    version: u32,
    stores: HashMap<String, AppVersionRecord>,
}

// pending drift global
static PENDING_DRIFT: Mutex<Option<AppVersionDrift>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Path
// ---------------------------------------------------------------------------

pub fn app_versions_path(override_path: Option<&str>) -> PathBuf {
    if let Some(p) = override_path {
        return PathBuf::from(p);
    }
    if let Ok(env) = std::env::var(APP_VERSIONS_ENV) {
        return PathBuf::from(env);
    }
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config")
        });
    config_home.join("capcut-cli").join("app-versions.json")
}

// ---------------------------------------------------------------------------
// Load
// ---------------------------------------------------------------------------

fn is_record(value: &serde_json::Value) -> bool {
    if let Some(obj) = value.as_object() {
        obj.get("app_source").and_then(|v| v.as_str()).is_some()
            && obj.get("seen_at").and_then(|v| v.as_str()).is_some()
    } else {
        false
    }
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

pub fn load_app_versions(path: &Path) -> (HashMap<String, AppVersionRecord>, Option<String>) {
    if !path.exists() {
        return (HashMap::new(), None);
    }
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return (
                HashMap::new(),
                Some(format!("app-version state did not parse: {} ({})", e, path.display())),
            )
        }
    };
    let content = strip_bom(&content);
    let parsed: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            return (
                HashMap::new(),
                Some(format!("app-version state did not parse: {} ({})", e, path.display())),
            )
        }
    };
    let stores_val = match parsed.get("stores") {
        Some(v) if v.is_object() => v,
        _ => {
            return (
                HashMap::new(),
                Some(format!("app-version state has no stores{{}} object ({})", path.display())),
            )
        }
    };
    let mut stores = HashMap::new();
    if let Some(map) = stores_val.as_object() {
        for (dir, record_val) in map {
            if !is_record(record_val) {
                continue;
            }
            if let Ok(rec) = serde_json::from_value::<AppVersionRecord>(record_val.clone()) {
                stores.insert(dir.clone(), rec);
            }
        }
    }
    (stores, None)
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

pub fn app_version_evidence(draft: &Draft, store_version: Option<&str>) -> Option<AppVersionEvidence> {
    let detected = assess_write_safety(draft, store_version).detected;
    if detected.app_version.is_none() && detected.schema_int.is_none() {
        return None;
    }
    Some(AppVersionEvidence {
        app_source: detected.app_source,
        app_version: detected.app_version,
        schema_int: detected.schema_int,
    })
}

fn describe_changes(last: &AppVersionEvidence, current: &AppVersionEvidence) -> Vec<String> {
    let mut changes = Vec::new();
    let label = |v: Option<&String>| -> String {
        match v {
            Some(s) => s.clone(),
            None => "(none)".to_string(),
        }
    };
    let label_int = |v: Option<i64>| -> String {
        match v {
            Some(n) => n.to_string(),
            None => "(none)".to_string(),
        }
    };
    if last.app_version != current.app_version {
        changes.push(format!(
            "app version {} -> {}",
            label(last.app_version.as_ref()),
            label(current.app_version.as_ref())
        ));
    }
    if last.app_source != current.app_source {
        changes.push(format!("app source {} -> {}", last.app_source, current.app_source));
    }
    if last.schema_int != current.schema_int {
        changes.push(format!(
            "schema generation {} -> {}",
            label_int(last.schema_int),
            label_int(current.schema_int)
        ));
    }
    changes
}

fn drift_between(
    store_dir: &str,
    last: Option<&AppVersionRecord>,
    current: Option<&AppVersionEvidence>,
) -> Option<AppVersionDrift> {
    let last = last?;
    let current = current?;
    let last_evidence = AppVersionEvidence {
        app_source: last.app_source,
        app_version: last.app_version.clone(),
        schema_int: last.schema_int,
    };
    let changes = describe_changes(&last_evidence, current);
    if changes.is_empty() {
        return None;
    }
    Some(AppVersionDrift {
        store_dir: store_dir.to_string(),
        from: last.clone(),
        to: current.clone(),
        changes,
    })
}

// ---------------------------------------------------------------------------
// assess (read-only)
// ---------------------------------------------------------------------------

pub fn assess_app_version_drift(
    project_dir: &Path,
    evidence: Option<&AppVersionEvidence>,
    state_path: Option<&Path>,
) -> (Option<AppVersionDrift>, Option<String>) {
    let default_path;
    let path: &Path = match state_path {
        Some(p) => p,
        None => {
            default_path = app_versions_path(None);
            &default_path
        }
    };
    let (stores, error) = load_app_versions(path);
    let store_dir = dunce_canonical(project_dir);
    let drift = drift_between(&store_dir, stores.get(&store_dir), evidence);
    (drift, error)
}

fn dunce_canonical(p: &Path) -> String {
    // Use canonical where possible, else absolute via current_dir join
    if let Ok(c) = p.canonicalize() {
        return c.to_string_lossy().to_string();
    }
    // fallback: join with current_dir
    if p.is_absolute() {
        return p.to_string_lossy().to_string();
    }
    if let Ok(cwd) = std::env::current_dir() {
        return cwd.join(p).to_string_lossy().to_string();
    }
    p.to_string_lossy().to_string()
}

// ---------------------------------------------------------------------------
// Atomic write helper
// ---------------------------------------------------------------------------

fn write_atomic_local(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!(
        "capcut-cli-{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    // set 0o600 on unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&temp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&temp, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// pending
// ---------------------------------------------------------------------------

pub fn take_app_version_drift() -> Option<AppVersionDrift> {
    PENDING_DRIFT.lock().ok()?.take()
}

// ---------------------------------------------------------------------------
// track (mutating)
// ---------------------------------------------------------------------------

pub fn track_app_version(
    project_dir: &Path,
    evidence: Option<&AppVersionEvidence>,
    state_path: Option<&Path>,
) -> (Option<AppVersionDrift>, Option<String>) {
    let default_path;
    let path: &Path = match state_path {
        Some(p) => p,
        None => {
            default_path = app_versions_path(None);
            &default_path
        }
    };
    let (mut stores, error) = load_app_versions(path);
    let Some(ev) = evidence else {
        return (None, error);
    };
    let store_dir = dunce_canonical(project_dir);
    let last = stores.get(&store_dir).cloned();
    let drift = drift_between(&store_dir, last.as_ref(), Some(ev));
    if let Some(ref d) = drift {
        if let Ok(mut guard) = PENDING_DRIFT.lock() {
            *guard = Some(d.clone());
        }
    }
    let needs_write = last.is_none() || drift.is_some();
    if needs_write {
        // build ISO8601 timestamp
        let seen_at = chrono_timestamp();
        let record = AppVersionRecord {
            app_source: ev.app_source,
            app_version: ev.app_version.clone(),
            schema_int: ev.schema_int,
            seen_at,
        };
        stores.insert(store_dir.clone(), record);
        let file = AppVersionsFile { version: 1, stores };
        let content = match serde_json::to_string_pretty(&file) {
            Ok(mut s) => {
                s.push('\n');
                s
            }
            Err(e) => {
                return (
                    drift,
                    Some(error.unwrap_or_else(|| format!("serialize app-versions: {}", e))),
                )
            }
        };
        if let Err(e) = write_atomic_local(path, &content) {
            let msg = format!("app-version state could not be written: {} ({})", e, path.display());
            return (drift, Some(error.unwrap_or(msg)));
        }
    }
    (drift, error)
}

fn chrono_timestamp() -> String {
    // Use std time to produce ISO8601 UTC without adding chrono dep
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    // Format as RFC3339-like: 2026-09-12T00:00:00Z — second precision is enough
    // Compute calendar from secs via simple algorithm using time crate would need dep.
    // Instead format as seconds-since-epoch string that still sorts; but spec expects ISO8601.
    // Produce "1970-01-01T00:00:00Z" style via manual conversion.
    format_rfc3339(secs)
}

fn format_rfc3339(secs: i64) -> String {
    // Howard Hinnant days algorithm
    let days = secs.div_euclid(86400);
    let secs_of_day = secs.rem_euclid(86400);
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;

    // civil_from_days
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mon: i64 = mp + if mp < 10 { 3 } else { -9 };
    y += if mon <= 2 { 1 } else { 0 };
    let year = y;
    let month = mon;
    let day = d;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, h, m, s)
}

pub fn format_app_version_drift_warning(drift: &AppVersionDrift) -> String {
    let app = match drift.to.app_source {
        AppSource::Lv => "JianYing",
        AppSource::Cc => "CapCut",
        AppSource::Unknown => "The app",
    };
    format!(
        "App version drift in this draft store: {} (recorded {}). {} may have auto-updated since the CLI last wrote here and drafts may have crossed a support boundary. Warning only — writes stay gated by the version guard. See \"Pinning app updates\" in docs/version-support.md.",
        drift.changes.join(", "),
        drift.from.seen_at,
        app
    )
}

// ---------------------------------------------------------------------------
// scanTrackedStores — simplified without store discovery (no draft read)
// ---------------------------------------------------------------------------

/// Scan every tracked store dir; if a drift check closure is provided, use it.
/// Without closure, just returns drifts based on re-reading current evidence via caller.
pub fn scan_tracked_stores(state_path: Option<&Path>) -> (usize, Vec<AppVersionDrift>, Option<String>) {
    let default_path;
    let path: &Path = match state_path {
        Some(p) => p,
        None => {
            default_path = app_versions_path(None);
            &default_path
        }
    };
    let (stores, error) = load_app_versions(path);
    // Without draft discovery we cannot compute new evidence; return no drifts but count tracked.
    let tracked = stores.len();
    (tracked, Vec::new(), error)
}

/// Scan with evidence provider closure (for callers that can load drafts).
pub fn scan_tracked_stores_with<F>(state_path: Option<&Path>, mut evidence_fn: F) -> (usize, Vec<AppVersionDrift>, Option<String>)
where
    F: FnMut(&str) -> Option<AppVersionEvidence>,
{
    let default_path;
    let path: &Path = match state_path {
        Some(p) => p,
        None => {
            default_path = app_versions_path(None);
            &default_path
        }
    };
    let (stores, error) = load_app_versions(path);
    let tracked = stores.len();
    let mut drifts = Vec::new();
    for (dir, last) in &stores {
        if !Path::new(dir).exists() {
            continue;
        }
        let Some(evidence) = evidence_fn(dir) else { continue };
        if let Some(d) = drift_between(dir, Some(last), Some(&evidence)) {
            drifts.push(d);
        }
    }
    (tracked, drifts, error)
}
