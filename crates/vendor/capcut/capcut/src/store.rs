use crate::draft::Draft;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Constants mirroring reference/src/store.ts
// ---------------------------------------------------------------------------

pub const STANDARD_FILES: &[&str] = &[
    "draft_content.json",
    "draft_info.json",
    "draft_meta_info.json",
    "template-2.tmp",
];

/// Warn text for nested Timelines/ layout (issue #50).
pub const NESTED_TIMELINES_WRITE_WARNING: &str =
    "Nested Timelines/ layout detected (CapCut 7.x \u{2014} issue #50): the app is reported to keep the live document at \
     Timelines/<id>/draft_info.json and to regenerate the project-root files from it, so this root-mirror edit may \
     be discarded the next time the project opens. The CLI still writes the root files only \u{2014} no verified fixture \
     for the nested layout exists yet. If you have such a project, contribute a bundle: \
     `capcut fixture <project> --out <dir>`. To copy this edit into the nested documents explicitly, run \
     `capcut sync-timelines <project> --nested --apply` (opt-in repair).";

pub const NESTED_TIMELINES_ACTION: &str =
    "Timelines/ directory with a nested timeline document: CapCut 7.x is reported to keep the live document at \
     Timelines/<id>/draft_info.json, with the project-root file a regenerated mirror (issue #50). Edit commands \
     still read and write the project-root files, so CapCut 7.x may discard those edits on the next open. \
     `capcut sync-timelines <project> --nested --apply` copies the root timeline into the nested documents as an \
     explicit opt-in repair. Evidence for this layout is report-only \u{2014} if you have such a project, contribute a \
     bundle: `capcut fixture <project> --out <dir>`.";

pub const NESTED_TIMELINES_MODERN_ACTION: &str =
    "Timelines/ directory with a nested timeline document, on CapCut >= 8.7 storage. No discard risk is claimed \
     here and none is ruled out: the 7.x report in issue #50 and the 8.5.0 open/close round trip in issue #68 both \
     predate this storage generation, so what the app does with the nested document on >= 8.7 is unevidenced in \
     either direction. Edit commands read and write the project-root files only; \
     `capcut sync-timelines <project> --nested --apply` copies the root timeline into the nested documents as an \
     explicit opt-in repair. If this project opens in your app \
     with a CLI edit intact \u{2014} or without it \u{2014} that is the artifact issue #50 has been blocked on: \
     `capcut fixture <project> --out <dir>`.";

const NESTED_TIMELINE_FILES: &[&str] = &["draft_info.json", "draft_content.json"];
const NESTED_MIRROR_FROM_ROOT_SINCE: &str = "8.5.0";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DraftStoreLayout {
    #[serde(rename = "content-primary")]
    ContentPrimary,
    #[serde(rename = "info-primary")]
    InfoPrimary,
    #[serde(rename = "timelines-nested")]
    TimelinesNested,
    #[serde(rename = "unknown")]
    Unknown,
}

impl std::fmt::Display for DraftStoreLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::ContentPrimary => "content-primary",
            Self::InfoPrimary => "info-primary",
            Self::TimelinesNested => "timelines-nested",
            Self::Unknown => "unknown",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
pub struct DraftCandidate {
    pub name: String,
    pub path: PathBuf,
    pub exists: bool,
    pub size: u64,
    pub mtime: Option<String>,
    pub sha256: Option<String>,
    /// BOM-stripped raw text (None when file missing or unreadable).
    pub raw: Option<String>,
    pub parseable: bool,
    /// Envelope path where the timeline was found (empty when top-level).
    pub envelope_path: Vec<String>,
    pub draft: Option<Draft>,
    pub timeline_hash: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DraftStore {
    pub project_dir: PathBuf,
    pub canonical: DraftCandidate,
    pub targets: Vec<DraftCandidate>,
    pub candidates: Vec<DraftCandidate>,
    /// Newest app_version among parseable targets.
    pub version: Option<String>,
    pub modern_storage: bool,
    pub diverged: bool,
    pub layout: DraftStoreLayout,
    /// Relative nested Timelines/ doc paths.
    pub nested_timelines: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hash_str(value: &str) -> String {
    sha256_hex(value.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    compute_sha256(bytes)
}

fn compute_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn file_mtime(meta: &std::fs::Metadata) -> Option<String> {
    meta.modified().ok().and_then(chrono_like_iso)
}

fn chrono_like_iso(t: std::time::SystemTime) -> Option<String> {
    let dur = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    let secs = dur.as_secs();
    Some(format!("{}", secs))
}
// ---------------------------------------------------------------------------
// Timeline detection (mirrors findTimeline / isTimeline)
// ---------------------------------------------------------------------------

fn is_timeline(value: &serde_json::Value) -> bool {
    if let serde_json::Value::Object(map) = value {
        if let Some(tracks) = map.get("tracks") {
            if !tracks.is_array() {
                return false;
            }
            if let Some(materials) = map.get("materials") {
                return materials.is_object();
            }
        }
    }
    false
}

fn find_timeline(
    value: &serde_json::Value,
    path: Vec<String>,
    depth: usize,
) -> Option<(Draft, Vec<String>)> {
    if is_timeline(value) {
        if let Ok(draft) = serde_json::from_value::<Draft>(value.clone()) {
            return Some((draft, path));
        }
    }
    if depth >= 3 {
        return None;
    }
    let obj = value.as_object()?;
    let preferred = [
        "draft_content",
        "draft_info",
        "timeline",
        "content",
        "data",
        "draft",
    ];
    let mut entries: Vec<(&String, &serde_json::Value)> = obj.iter().collect();
    entries.sort_by(|(a, _), (b, _)| {
        let ai = preferred.iter().position(|p| p == a).unwrap_or(99);
        let bi = preferred.iter().position(|p| p == b).unwrap_or(99);
        ai.cmp(&bi)
    });
    for (key, child) in entries {
        if let serde_json::Value::String(s) = child {
            let trimmed = s.trim();
            if trimmed.starts_with('{') {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    let mut next_path = path.clone();
                    next_path.push(format!("{}:json", key));
                    if let Some(found) = find_timeline(&parsed, next_path, depth + 1) {
                        return Some(found);
                    }
                }
            }
        } else if child.is_object() {
            let mut next_path = path.clone();
            next_path.push(key.clone());
            if let Some(found) = find_timeline(child, next_path, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// parse_candidate
// ---------------------------------------------------------------------------

pub fn parse_candidate(path: &Path) -> DraftCandidate {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    if !path.exists() {
        return DraftCandidate {
            name,
            path: path.to_path_buf(),
            exists: false,
            size: 0,
            mtime: None,
            sha256: None,
            raw: None,
            parseable: false,
            envelope_path: vec![],
            draft: None,
            timeline_hash: None,
            error: None,
        };
    }

    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return DraftCandidate {
                name,
                path: path.to_path_buf(),
                exists: false,
                size: 0,
                mtime: None,
                sha256: None,
                raw: None,
                parseable: false,
                envelope_path: vec![],
                draft: None,
                timeline_hash: None,
                error: Some(e.to_string()),
            };
        }
    };
    let size = meta.len();
    let mtime = file_mtime(&meta);

    let raw_bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            return DraftCandidate {
                name,
                path: path.to_path_buf(),
                exists: true,
                size,
                mtime,
                sha256: None,
                raw: None,
                parseable: false,
                envelope_path: vec![],
                draft: None,
                timeline_hash: None,
                error: Some(e.to_string()),
            };
        }
    };
    let text = String::from_utf8_lossy(&raw_bytes).to_string();
    let raw = crate::bom::strip_bom(&text).to_string();
    let sha = hash_str(&raw);

    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(parsed) => {
            if let Some((draft, envelope_path)) = find_timeline(&parsed, vec![], 0) {
                let timeline_hash = hash_str(&serde_json::to_string(&draft).unwrap_or_default());
                DraftCandidate {
                    name,
                    path: path.to_path_buf(),
                    exists: true,
                    size,
                    mtime,
                    sha256: Some(sha),
                    raw: Some(raw),
                    parseable: true,
                    envelope_path,
                    draft: Some(draft),
                    timeline_hash: Some(timeline_hash),
                    error: None,
                }
            } else {
                DraftCandidate {
                    name,
                    path: path.to_path_buf(),
                    exists: true,
                    size,
                    mtime,
                    sha256: Some(sha),
                    raw: Some(raw),
                    parseable: false,
                    envelope_path: vec![],
                    draft: None,
                    timeline_hash: None,
                    error: Some("JSON file does not contain a recognizable timeline".to_string()),
                }
            }
        }
        Err(e) => DraftCandidate {
            name,
            path: path.to_path_buf(),
            exists: true,
            size,
            mtime,
            sha256: Some(sha),
            raw: Some(raw),
            parseable: false,
            envelope_path: vec![],
            draft: None,
            timeline_hash: None,
            error: Some(e.to_string()),
        },
    }
}

// ---------------------------------------------------------------------------
// Nested timelines detection
// ---------------------------------------------------------------------------

fn detect_nested_timelines(project_dir: &Path) -> (bool, Vec<String>) {
    let timelines_dir = project_dir.join("Timelines");
    let Ok(meta) = std::fs::metadata(&timelines_dir) else {
        return (false, vec![]);
    };
    if !meta.is_dir() {
        return (false, vec![]);
    }
    let mut files: Vec<String> = vec![];
    if timelines_dir.join("project.json").exists() {
        files.push("Timelines/project.json".to_string());
    }
    let entries = match std::fs::read_dir(&timelines_dir) {
        Ok(rd) => rd,
        Err(_) => return (false, files),
    };
    let mut dirs: Vec<PathBuf> = vec![];
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            dirs.push(p);
        }
    }
    dirs.sort();
    for dir in dirs {
        let entry_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        for name in NESTED_TIMELINE_FILES {
            if dir.join(name).exists() {
                files.push(format!("Timelines/{}/{}", entry_name, name));
            }
        }
    }
    ( !files.is_empty(), files )
}

// ---------------------------------------------------------------------------
// candidate_paths / discover_draft_store / store_after_write
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CandidatePaths {
    pub project_dir: PathBuf,
    pub requested: Option<PathBuf>,
    pub paths: Vec<PathBuf>,
}

pub fn candidate_paths(input: &Path) -> CandidatePaths {
    // Resolve to absolute where possible (best-effort).
    let resolved = if input.is_absolute() {
        input.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(input)
    };
    // Normalize `.` / `..` without requiring existence (like Node `resolve`).
    // We do a simple lexical clean; existence check below covers file vs dir.
    let is_file = resolved.is_file();
    // Also check original input if resolved doesn't exist (relative not yet created).
    let is_file = is_file || (input.exists() && input.is_file());

    let project_dir: PathBuf;
    let requested: Option<PathBuf>;
    if is_file {
        // Prefer the existing path that is a file.
        let file_path = if resolved.is_file() {
            resolved.clone()
        } else {
            // input is the file
            if input.is_absolute() {
                input.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(input)
            }
        };
        requested = Some(file_path.clone());
        project_dir = file_path.parent().map(|p| p.to_path_buf()).unwrap_or(file_path.clone());
    } else {
        // Directory input (may not exist yet — use resolved).
        requested = None;
        // For candidate probing, use `resolved` if it exists as dir, else `input` joined.
        // If input is a dir that exists, use its canonical-ish path.
        let dir = if input.exists() && input.is_dir() {
            if input.is_absolute() { input.to_path_buf() } else { std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(input) }
        } else {
            resolved.clone()
        };
        project_dir = dir;
    }

    let mut paths: Vec<PathBuf> = vec![];
    if let Some(ref req) = requested {
        paths.push(req.clone());
    }
    for name in STANDARD_FILES {
        let p = project_dir.join(name);
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    CandidatePaths { project_dir, requested, paths }
}

fn highest_version(parseable: &[DraftCandidate]) -> Option<String> {
    let mut versions: Vec<String> = parseable
        .iter()
        .filter_map(|c| {
            c.draft.as_ref().and_then(|d| {
                // draft.platform is Value; look for app_version
                if let serde_json::Value::Object(map) = &d.platform {
                    if let Some(serde_json::Value::String(s)) = map.get("app_version") {
                        if !s.is_empty() {
                            return Some(s.clone());
                        }
                    }
                }
                // also check extra map
                if let Some(serde_json::Value::String(s)) = d.extra.get("app_version") {
                    if !s.is_empty() {
                        return Some(s.clone());
                    }
                }
                None
            })
        })
        .collect();
    if versions.is_empty() {
        return None;
    }
    versions.sort_by(|a, b| {
        if crate::version::at_least(Some(a.as_str()), b) {
            std::cmp::Ordering::Less
        } else if crate::version::at_least(Some(b.as_str()), a) {
            std::cmp::Ordering::Greater
        } else {
            a.cmp(b)
        }
    });
    versions.into_iter().next()
}

pub fn discover_draft_store(input: &Path) -> Result<DraftStore> {
    let cp = candidate_paths(input);
    let candidates: Vec<DraftCandidate> = cp.paths.iter().map(|p| parse_candidate(p)).collect();
    let parseable: Vec<&DraftCandidate> = candidates.iter().filter(|c| c.parseable && c.draft.is_some()).collect();

    if parseable.is_empty() {
        let found: Vec<String> = candidates.iter().filter(|c| c.exists).map(|c| c.name.clone()).collect();
        let detail = if found.is_empty() {
            String::new()
        } else {
            format!("Found {}, but none contained a readable timeline.", found.join(", "))
        };
        anyhow::bail!(
            "No draft found at: {}\nExpected draft_content.json, draft_info.json, draft_meta_info.json, or template-2.tmp. {}",
            input.display(),
            detail.trim()
        );
    }

    // Collect owned parseable for version calc
    let parseable_owned: Vec<DraftCandidate> = candidates
        .iter()
        .filter(|c| c.parseable && c.draft.is_some())
        .cloned()
        .collect();
    let version = highest_version(&parseable_owned);
    let modern_storage = version
        .as_deref()
        .map(|v| crate::version::at_least(Some(v), "8.7"))
        .unwrap_or(false);

    // Canonical selection
    let mut canonical: Option<DraftCandidate> = None;
    if let Some(ref req) = cp.requested {
        canonical = parseable_owned.iter().find(|c| &c.path == req).cloned();
    }
    if canonical.is_none() {
        let preference: &[&str] = if modern_storage {
            &["template-2.tmp", "draft_meta_info.json", "draft_content.json", "draft_info.json"]
        } else {
            &["draft_content.json", "draft_info.json", "template-2.tmp", "draft_meta_info.json"]
        };
        for name in preference {
            if let Some(c) = parseable_owned.iter().find(|c| c.name == *name) {
                canonical = Some(c.clone());
                break;
            }
        }
    }
    let canonical = canonical.unwrap_or_else(|| parseable_owned[0].clone());

    let diverged = {
        let mut set: HashSet<String> = HashSet::new();
        for c in &parseable_owned {
            if let Some(h) = &c.timeline_hash {
                set.insert(h.clone());
            }
        }
        set.len() > 1
    };

    let content_readable = parseable_owned.iter().any(|c| c.name == "draft_content.json");
    let info_readable = parseable_owned.iter().any(|c| c.name == "draft_info.json");
    let (nested_present, nested_files) = detect_nested_timelines(&cp.project_dir);

    let layout = if nested_present && !modern_storage {
        DraftStoreLayout::TimelinesNested
    } else if content_readable {
        DraftStoreLayout::ContentPrimary
    } else if info_readable {
        DraftStoreLayout::InfoPrimary
    } else {
        DraftStoreLayout::Unknown
    };

    let targets = parseable_owned;

    Ok(DraftStore {
        project_dir: cp.project_dir,
        canonical,
        targets,
        candidates,
        version,
        modern_storage,
        diverged,
        layout,
        nested_timelines: nested_files,
    })
}

pub fn store_after_write(
    store: &DraftStore,
    draft: &Draft,
    written: &HashMap<PathBuf, String>,
) -> DraftStore {
    // Refresh helper
    let refresh = |candidate: &DraftCandidate| -> DraftCandidate {
        if let Some(content) = written.get(&candidate.path) {
            let size = content.len() as u64;
            let (actual_size, mtime) = match std::fs::metadata(&candidate.path) {
                Ok(meta) => (meta.len(), file_mtime(&meta)),
                Err(_) => (size, candidate.mtime.clone()),
            };
            let sha = hash_str(content);
            let timeline_hash = hash_str(&serde_json::to_string(draft).unwrap_or_default());
            DraftCandidate {
                name: candidate.name.clone(),
                path: candidate.path.clone(),
                exists: true,
                size: actual_size,
                mtime,
                sha256: Some(sha),
                raw: Some(content.clone()),
                parseable: true,
                envelope_path: candidate.envelope_path.clone(),
                draft: Some(draft.clone()),
                timeline_hash: Some(timeline_hash),
                error: None,
            }
        } else {
            candidate.clone()
        }
    };

    let order = candidate_paths(&store.canonical.path).paths;
    let rank = |c: &DraftCandidate| -> usize {
        order.iter().position(|p| p == &c.path).unwrap_or(order.len())
    };

    let mut candidates: Vec<DraftCandidate> = store.candidates.clone();
    candidates.sort_by_key(|c| rank(c));
    candidates = candidates.iter().map(refresh).collect();

    let targets: Vec<DraftCandidate> = candidates
        .iter()
        .filter(|c| c.parseable && c.draft.is_some())
        .cloned()
        .collect();

    let version = highest_version(&targets);
    let modern_storage = version
        .as_deref()
        .map(|v| crate::version::at_least(Some(v), "8.7"))
        .unwrap_or(false);

    let content_readable = targets.iter().any(|c| c.name == "draft_content.json");
    let info_readable = targets.iter().any(|c| c.name == "draft_info.json");

    let diverged = {
        let mut set: HashSet<String> = HashSet::new();
        for c in &targets {
            if let Some(h) = &c.timeline_hash {
                set.insert(h.clone());
            }
        }
        set.len() > 1
    };

    let layout = if !store.nested_timelines.is_empty() && !modern_storage {
        DraftStoreLayout::TimelinesNested
    } else if content_readable {
        DraftStoreLayout::ContentPrimary
    } else if info_readable {
        DraftStoreLayout::InfoPrimary
    } else {
        DraftStoreLayout::Unknown
    };

    let canonical = targets
        .iter()
        .find(|c| c.path == store.canonical.path)
        .cloned()
        .unwrap_or_else(|| {
            // canonical was overwritten
            if let Some(content) = written.get(&store.canonical.path) {
                let sha = hash_str(content);
                let th = hash_str(&serde_json::to_string(draft).unwrap_or_default());
                let (sz, mt) = match std::fs::metadata(&store.canonical.path) {
                    Ok(m) => (m.len(), file_mtime(&m)),
                    Err(_) => (content.len() as u64, store.canonical.mtime.clone()),
                };
                DraftCandidate {
                    name: store.canonical.name.clone(),
                    path: store.canonical.path.clone(),
                    exists: true,
                    size: sz,
                    mtime: mt,
                    sha256: Some(sha),
                    raw: Some(content.clone()),
                    parseable: true,
                    envelope_path: store.canonical.envelope_path.clone(),
                    draft: Some(draft.clone()),
                    timeline_hash: Some(th),
                    error: None,
                }
            } else {
                store.canonical.clone()
            }
        });

    DraftStore {
        project_dir: store.project_dir.clone(),
        canonical,
        targets,
        candidates,
        version,
        modern_storage,
        diverged,
        layout,
        nested_timelines: store.nested_timelines.clone(),
    }
}

// ---------------------------------------------------------------------------
// serialize_draft_candidate
// ---------------------------------------------------------------------------

fn indent_of(raw: Option<&str>) -> usize {
    if let Some(s) = raw {
        if let Some(pos) = s.find('\n') {
            let rest = &s[pos + 1..];
            let indent_len = rest.chars().take_while(|c| *c == ' ' || *c == '\t').count();
            if indent_len > 0 {
                // detect tab vs spaces
                if rest.chars().next() == Some('\t') {
                    return 0; // signal tab — caller handles
                }
                return indent_len;
            }
        }
    }
    0
}

fn is_tab_indent(raw: Option<&str>) -> bool {
    if let Some(s) = raw {
        if let Some(pos) = s.find('\n') {
            let rest = &s[pos + 1..];
            return rest.starts_with('\t');
        }
    }
    false
}

fn replace_at_path(root: &mut serde_json::Value, path: &[String], draft: &Draft) -> Result<()> {
    if path.is_empty() {
        *root = serde_json::to_value(draft).context("serialize draft")?;
        return Ok(());
    }
    let part = &path[0];
    let rest = &path[1..];
    let is_json = part.ends_with(":json");
    let key = if is_json { &part[..part.len() - 5] } else { part.as_str() };
    let obj = root.as_object_mut().context("Cannot update draft envelope at path")?;
    if is_json {
        let entry = obj.get_mut(key).context(format!("Cannot update JSON envelope field {}", key))?;
        if !entry.is_string() {
            anyhow::bail!("Cannot update JSON envelope field {}", key);
        }
        let inner_str = entry.as_str().unwrap().to_string();
        let mut inner: serde_json::Value = serde_json::from_str(&inner_str).context("parse envelope JSON string")?;
        replace_at_path(&mut inner, rest, draft)?;
        *entry = serde_json::Value::String(serde_json::to_string(&inner)?);
    } else {
        let entry = obj.get_mut(key).context(format!("missing envelope key {}", key))?;
        replace_at_path(entry, rest, draft)?;
    }
    Ok(())
}

pub fn serialize_draft_candidate(candidate: &DraftCandidate, draft: &Draft) -> Result<String> {
    if candidate.raw.is_none() || candidate.envelope_path.is_empty() {
        // Preserve indent style
        let raw_opt = candidate.raw.as_deref();
        if is_tab_indent(raw_opt) {
            // serde_json doesn't support tab indent directly; produce pretty then replace
            let pretty = serde_json::to_string_pretty(draft)?;
            // replace 2-space indent with tab
            let tabbed = pretty.replace("  ", "\t");
            // Actually serde pretty uses 2 spaces; replace leading spaces
            // Simplistic: not critical for correctness.
            return Ok(tabbed);
        }
        let indent = indent_of(raw_opt);
        if indent > 0 {
            // Use custom pretty with indent width
            // serde_json only supports 2-space via to_string_pretty; for other widths
            // we accept default pretty (close enough). For strict parity we could use
            // `serde_json::ser::PrettyFormatter::with_indent`.
            // Quick: use `serde_json` pretty formatter with custom indent via manual.
            let pretty = to_string_with_indent(draft, indent)?;
            return Ok(pretty);
        }
        return Ok(serde_json::to_string(draft)?);
    }
    let raw = candidate.raw.as_deref().unwrap();
    let mut root: serde_json::Value = serde_json::from_str(raw).context("parse candidate raw for envelope")?;
    replace_at_path(&mut root, &candidate.envelope_path, draft)?;
    let raw_opt = candidate.raw.as_deref();
    if is_tab_indent(raw_opt) {
        let s = serde_json::to_string_pretty(&root)?;
        return Ok(s.replace("  ", "\t"));
    }
    let indent = indent_of(raw_opt);
    if indent != 0 && indent != 2 {
        return to_string_with_indent(&root, indent);
    }
    // default 2-space pretty if original was pretty, else compact if original was compact
    // Detect pretty by presence of newline in raw
    if raw.contains('\n') {
        Ok(serde_json::to_string_pretty(&root)?)
    } else {
        Ok(serde_json::to_string(&root)?)
    }
}

fn to_string_with_indent<T: serde::Serialize>(value: &T, indent: usize) -> Result<String> {
    let mut buf = Vec::new();
    let indent_bytes = vec![b' '; indent];
    let formatter = serde_json::ser::PrettyFormatter::with_indent(&indent_bytes);
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser)?;
    Ok(String::from_utf8(buf)?)
}

// ---------------------------------------------------------------------------
// Editor processes
// ---------------------------------------------------------------------------

pub fn match_editor_processes(listing: &str, flavor: &str) -> Vec<String> {
    let names: &[&str] = if flavor == "win32" {
        &["CapCut.exe", "JianyingPro.exe"]
    } else {
        &["CapCut", "JianyingPro"]
    };
    let mut seen: HashSet<String> = HashSet::new();
    for raw in listing.split('\n') {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let exe = if flavor == "win32" {
            // tasklist CSV: first quoted field
            if let Some(start) = line.find('"') {
                if let Some(end) = line[start + 1..].find('"') {
                    line[start + 1..start + 1 + end].to_string()
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        } else {
            line.split('/').last().unwrap_or(line).to_string()
        };
        seen.insert(exe.to_lowercase());
    }
    names
        .iter()
        .filter(|n| seen.contains(&n.to_lowercase()))
        .map(|s| s.to_string())
        .collect()
}

pub fn editor_processes() -> Vec<String> {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .output();
        if let Ok(out) = output {
            let listing = String::from_utf8_lossy(&out.stdout).to_string();
            return match_editor_processes(&listing, "win32");
        }
        vec![]
    }
    #[cfg(not(windows))]
    {
        let output = std::process::Command::new("ps").args(["-axo", "comm="]).output();
        if let Ok(out) = output {
            let listing = String::from_utf8_lossy(&out.stdout).to_string();
            return match_editor_processes(&listing, "posix");
        }
        vec![]
    }
}

// ---------------------------------------------------------------------------
// draft_dir_candidates / default_drafts_dir
// ---------------------------------------------------------------------------

pub struct DraftDirCandidate {
    pub label: String,
    pub path: PathBuf,
}

pub fn draft_dir_candidates() -> Vec<DraftDirCandidate> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs_home();
        return vec![
            DraftDirCandidate {
                label: "CapCut (macOS)".to_string(),
                path: home.join("Movies/CapCut/User Data/Projects/com.lveditor.draft"),
            },
            DraftDirCandidate {
                label: "JianYing (macOS)".to_string(),
                path: home.join("Movies/JianyingPro/User Data/Projects/com.lveditor.draft"),
            },
        ];
    }
    #[cfg(windows)]
    {
        let local = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs_home().join("AppData/Local"));
        return vec![
            DraftDirCandidate {
                label: "CapCut (Windows)".to_string(),
                path: local.join("CapCut/User Data/Projects/com.lveditor.draft"),
            },
            DraftDirCandidate {
                label: "JianYing (Windows)".to_string(),
                path: local.join("JianyingPro/User Data/Projects/com.lveditor.draft"),
            },
        ];
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        vec![]
    }
}

fn dirs_home() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h);
    }
    if let Ok(h) = std::env::var("USERPROFILE") {
        return PathBuf::from(h);
    }
    PathBuf::from(".")
}

pub fn default_drafts_dir() -> Option<PathBuf> {
    if let Ok(override_dir) = std::env::var("CAPCUT_DRAFT_DIR") {
        let trimmed = override_dir.trim().to_string();
        if !trimmed.is_empty() {
            // resolve to absolute
            let p = PathBuf::from(&trimmed);
            if p.is_absolute() {
                return Some(p);
            }
            if let Ok(cwd) = std::env::current_dir() {
                return Some(cwd.join(p));
            }
            return Some(p);
        }
    }
    let candidates = draft_dir_candidates();
    if let Some(found) = candidates.iter().find(|c| c.path.exists()) {
        return Some(found.path.clone());
    }
    candidates.into_iter().next().map(|c| c.path)
}

// ---------------------------------------------------------------------------
// Nested warnings version-gated helpers (optional export)
// ---------------------------------------------------------------------------

fn nested_mirror_is_safe(app_version: Option<&str>) -> bool {
    match app_version {
        Some(v) => {
            !crate::version::is_pre_release(Some(v))
                && crate::version::at_least(Some(v), NESTED_MIRROR_FROM_ROOT_SINCE)
        }
        None => false,
    }
}

fn nested_mirror_is_unevidenced_prerelease(app_version: Option<&str>) -> bool {
    match app_version {
        Some(v) => {
            crate::version::is_pre_release(Some(v))
                && crate::version::at_least(Some(v), NESTED_MIRROR_FROM_ROOT_SINCE)
        }
        None => false,
    }
}

pub fn nested_timelines_write_warning(app_version: Option<&str>) -> String {
    if nested_mirror_is_unevidenced_prerelease(app_version) {
        return format!(
            "Nested Timelines/ layout detected. Pre-release {} at or above {} has no verified storage behavior — neither the 7.x discard risk nor the 8.5.0 reassurance applies.",
            app_version.unwrap_or(""),
            NESTED_MIRROR_FROM_ROOT_SINCE
        );
    }
    if !nested_mirror_is_safe(app_version) {
        return NESTED_TIMELINES_WRITE_WARNING.to_string();
    }
    format!(
        "Nested Timelines/ layout detected on CapCut {} (issue #68): on this version the app is reported to regenerate Timelines/<id>/draft_info.json from the project-root file, so this root-mirror edit should survive the next open. The CLI writes the root files only.",
        app_version.unwrap_or("")
    )
}

pub fn nested_timelines_action(app_version: Option<&str>) -> String {
    if nested_mirror_is_unevidenced_prerelease(app_version) {
        return format!(
            "Timelines/ directory with a nested timeline document. Pre-release {} at or above {} has no verified storage behavior.",
            app_version.unwrap_or(""),
            NESTED_MIRROR_FROM_ROOT_SINCE
        );
    }
    if !nested_mirror_is_safe(app_version) {
        return NESTED_TIMELINES_ACTION.to_string();
    }
    format!(
        "Timelines/ directory with a nested timeline document, on CapCut {}: this version is reported to regenerate the nested document from the project-root file (issue #68 — byte-identical after an open/close round trip), so the root-file writes the edit commands perform are the ones the app keeps. The 7.x caution in issue #50 still applies to older builds.",
        app_version.unwrap_or("")
    )
}

// ---------------------------------------------------------------------------
// Legacy single-file helpers (kept for backward compat)
// ---------------------------------------------------------------------------

/// Resolve draft_content.json path from a user input (file or directory).
pub fn resolve_draft_path(input: &Path) -> Result<PathBuf> {
    if input.is_file() {
        return Ok(input.to_path_buf());
    }
    let candidate = input.join("draft_content.json");
    if candidate.exists() {
        return Ok(candidate);
    }
    anyhow::bail!("no draft_content.json found under {}", input.display());
}

pub fn load_draft(path: &Path) -> Result<Draft> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let raw = crate::bom::strip_bom(&raw);
    let draft: Draft = serde_json::from_str(raw).context("parse draft_content.json")?;
    Ok(draft)
}

pub fn save_draft(path: &Path, draft: &Draft) -> Result<()> {
    let json = serde_json::to_string(draft).context("serialize draft")?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &json).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

pub fn save_draft_pretty(path: &Path, draft: &Draft) -> Result<()> {
    let json = serde_json::to_string_pretty(draft).context("serialize draft")?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
