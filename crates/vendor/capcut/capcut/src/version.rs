use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::draft::Draft;

// ---------------------------------------------------------------------------
// AppSource / enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppSource {
    #[serde(rename = "cc")]
    Cc,
    #[serde(rename = "lv")]
    Lv,
    #[serde(rename = "unknown")]
    Unknown,
}

impl AppSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AppSource::Cc => "cc",
            AppSource::Lv => "lv",
            AppSource::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "cc" => AppSource::Cc,
            "lv" => AppSource::Lv,
            _ => AppSource::Unknown,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            AppSource::Cc => "CapCut",
            AppSource::Lv => "JianYing",
            AppSource::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for AppSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportEvidence {
    #[serde(rename = "fixture-tested")]
    FixtureTested,
    #[serde(rename = "synthetic-tested")]
    SyntheticTested,
    #[serde(rename = "reported")]
    Reported,
    #[serde(rename = "expected-compatible")]
    ExpectedCompatible,
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportStatus {
    #[serde(rename = "supported")]
    Supported,
    #[serde(rename = "untested")]
    Untested,
    #[serde(rename = "known-broken")]
    KnownBroken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WriteGuard {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "warn")]
    Warn,
    #[serde(rename = "refuse")]
    Refuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskField {
    #[serde(rename = "mask")]
    Mask,
    #[serde(rename = "common_mask")]
    CommonMask,
    #[serde(rename = "common_masks")]
    CommonMasks,
    #[serde(rename = "both")]
    Both,
    #[serde(rename = "none")]
    None,
}

// ---------------------------------------------------------------------------
// VersionInfo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub app: String,
    pub app_source: AppSource,
    pub app_version: Option<String>,
    pub os: Option<String>,
    pub schema: SchemaInfo,
    pub support: SupportInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaInfo {
    pub mask_field: MaskField,
    pub has_text_ranges: bool,
    pub has_audio_fades: bool,
    pub new_version_field: Value,
    pub last_modified_platform: Value,
    pub schema_int: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportInfo {
    pub status: SupportStatus,
    pub notes: Vec<String>,
    pub evidence: SupportEvidence,
    pub beyond_known_range: bool,
    pub write_guard: WriteGuard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteSafety {
    pub action: WriteGuard,
    pub reasons: Vec<String>,
    pub detected: DetectedVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedVersion {
    pub app_source: AppSource,
    pub app_version: Option<String>,
    pub schema_int: Option<i64>,
    pub beyond_known_range: bool,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SupportedEntry {
    pub version: &'static str,
    pub evidence: SupportEvidence,
}

#[derive(Debug, Clone)]
pub struct BrokenEntry {
    pub min: &'static str,
    pub reason: &'static str,
}

#[derive(Debug, Clone)]
pub struct VersionRegistryEntry {
    pub range: &'static str,
    pub ceiling: Option<&'static str>,
    pub supported: &'static [SupportedEntry],
    pub expected_compatible: &'static [&'static str],
    pub broken: &'static [BrokenEntry],
}

pub const KNOWN_SCHEMA_INT_MAX: i64 = 360_000;

pub fn supported_app_versions(source: AppSource) -> VersionRegistryEntry {
    match source {
        AppSource::Cc => VersionRegistryEntry {
            range: "6.x — 9.x",
            ceiling: Some("9.99"),
            supported: &[
                SupportedEntry {
                    version: "6.2.8",
                    evidence: SupportEvidence::FixtureTested,
                },
                SupportedEntry {
                    version: "8.7.0",
                    evidence: SupportEvidence::SyntheticTested,
                },
            ],
            expected_compatible: &["6.5.0", "7.0.0", "8.0.0", "9.0.0"],
            broken: &[],
        },
        AppSource::Lv => VersionRegistryEntry {
            range: "5.9.x (auto-update destroys pinning — see docs/version-support.md)",
            ceiling: Some("5.99"),
            supported: &[SupportedEntry {
                version: "5.9.0",
                evidence: SupportEvidence::Reported,
            }],
            expected_compatible: &[],
            broken: &[BrokenEntry {
                min: "6.0",
                reason: "encrypted draft_content.json era — see `capcut decrypt`",
            }],
        },
        AppSource::Unknown => VersionRegistryEntry {
            range: "unknown",
            ceiling: None,
            supported: &[],
            expected_compatible: &[],
            broken: &[],
        },
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Parse a version string into numeric tuple, mirroring TS versionTuple.
pub fn version_tuple(version: Option<&str>) -> Vec<i64> {
    let s = version.unwrap_or("");
    s.split('.')
        .filter_map(|part| {
            // parseInt behaviour: leading numeric prefix; empty -> NaN filtered.
            // Extract leading optional sign + digits prefix.
            let trimmed = part.trim();
            if trimmed.is_empty() {
                return None;
            }
            // Find numeric prefix like parseInt
            let mut prefix = String::new();
            for (idx, ch) in trimmed.chars().enumerate() {
                if idx == 0 && (ch == '-' || ch == '+') {
                    prefix.push(ch);
                } else if ch.is_ascii_digit() {
                    prefix.push(ch);
                } else {
                    break;
                }
            }
            if prefix.is_empty() || prefix == "-" || prefix == "+" {
                return None;
            }
            prefix.parse::<i64>().ok()
        })
        .collect()
}

pub fn at_least(version: Option<&str>, wanted: &str) -> bool {
    let a = version_tuple(version);
    let b = version_tuple(Some(wanted));
    let max = a.len().max(b.len());
    for i in 0..max {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        let diff = av - bv;
        if diff != 0 {
            return diff > 0;
        }
    }
    true
}

pub fn is_pre_release(version: Option<&str>) -> bool {
    let Some(v) = version else { return false };
    let trimmed = v.trim();
    if trimmed.is_empty() {
        return false;
    }
    let first = trimmed.chars().next().unwrap();
    if !first.is_ascii_digit() {
        return false;
    }
    // must match /^\d+(?:\.\d+)*$/
    let re_ok = trimmed
        .split('.')
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    // But check that splitting on '.' covers whole string without extra chars.
    // The above already ensures each segment is all digits, but we also need to ensure
    // no extra characters like '-' remain. Since we split on '.', any '-' stays inside a segment
    // and causes the segment check to fail.
    !re_ok
}

// ---------------------------------------------------------------------------
// Draft helpers (internal)
// ---------------------------------------------------------------------------

fn platform_field(draft: &Draft) -> Option<serde_json::Map<String, Value>> {
    if draft.platform.is_null() || draft.platform == Value::Object(Default::default()) {
        return None;
    }
    if let Value::Object(map) = &draft.platform {
        if map.is_empty() {
            return None;
        }
        return Some(map.clone());
    }
    None
}

fn get_platform_str(draft: &Draft, key: &str) -> Option<String> {
    let map = platform_field(draft)?;
    map.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn schema_int(draft: &Draft) -> Option<i64> {
    let raw = draft.extra.get("version")?;
    match raw {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            // parseInt behaviour: leading integer prefix
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            let mut prefix = String::new();
            for (idx, ch) in trimmed.chars().enumerate() {
                if idx == 0 && (ch == '-' || ch == '+') {
                    prefix.push(ch);
                } else if ch.is_ascii_digit() {
                    prefix.push(ch);
                } else {
                    break;
                }
            }
            if prefix.is_empty() || prefix == "-" || prefix == "+" {
                return None;
            }
            prefix.parse::<i64>().ok()
        }
        _ => None,
    }
}

const FIXTURE_CTA: &str = "If this project opens fine in your app, run `capcut fixture <project> --out <dir>` to build a redacted bundle and attach it to an issue so this version can move to fixture-tested; or pass --force-write to write anyway.";

// ---------------------------------------------------------------------------
// assessWriteSafety
// ---------------------------------------------------------------------------

pub fn assess_write_safety(draft: &Draft, store_version: Option<&str>) -> WriteSafety {
    let raw_source = get_platform_str(draft, "app_source");
    let app_source = match raw_source.as_deref() {
        Some("cc") => AppSource::Cc,
        Some("lv") => AppSource::Lv,
        _ => AppSource::Unknown,
    };

    // last_modified_platform.app_version
    let last_version: Option<String> = draft
        .extra
        .get("last_modified_platform")
        .and_then(|v| v.as_object())
        .and_then(|m| m.get("app_version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let si = schema_int(draft);

    // candidates: max version among platform, last, store
    let platform_version = get_platform_str(draft, "app_version");
    let mut candidates: Vec<String> = Vec::new();
    for cand in [&platform_version, &last_version, &store_version.map(|s| s.to_string())] {
        if let Some(v) = cand {
            if !version_tuple(Some(v)).is_empty() {
                candidates.push(v.clone());
            }
        }
    }
    // sort descending by version (max first)
    candidates.sort_by(|a, b| {
        if at_least(Some(a), b) && at_least(Some(b), a) {
            std::cmp::Ordering::Equal
        } else if at_least(Some(a), b) {
            std::cmp::Ordering::Less // a >= b => a comes first
        } else {
            std::cmp::Ordering::Greater
        }
    });
    // The above comparator is not total but sufficient for max selection.
    // Simpler: find max via iterative comparison
    let app_version: Option<String> = {
        let mut best: Option<String> = None;
        for c in &candidates {
            match &best {
                None => best = Some(c.clone()),
                Some(b) => {
                    if at_least(Some(c), b) && !at_least(Some(b), c) {
                        best = Some(c.clone());
                    }
                }
            }
        }
        best
    };

    let registry = supported_app_versions(app_source);
    let beyond_ceiling = match (&app_version, registry.ceiling) {
        (Some(v), Some(ceil)) => !at_least(Some(ceil), v),
        _ => false,
    };
    let beyond_known_range = beyond_ceiling || si.map(|n| n > KNOWN_SCHEMA_INT_MAX).unwrap_or(false);
    let detected = DetectedVersion {
        app_source,
        app_version: app_version.clone(),
        schema_int: si,
        beyond_known_range,
    };

    // broken check
    if let Some(ref av) = app_version {
        if let Some(broken) = registry.broken.iter().find(|e| at_least(Some(av), e.min)) {
            let label = if app_source == AppSource::Lv { "JianYing" } else { "CapCut" };
            return WriteSafety {
                action: WriteGuard::Refuse,
                reasons: vec![
                    format!(
                        "{} {} is known-broken (>= {}: {}). A plaintext write may be ignored or shown as corrupted (\"内容已损坏\") by the app. See docs/version-support.md.",
                        label, av, broken.min, broken.reason
                    ),
                    FIXTURE_CTA.to_string(),
                ],
                detected,
            };
        }
    }
    if beyond_ceiling {
        let av = app_version.clone().unwrap();
        let label = if app_source == AppSource::Cc { "CapCut" } else { "JianYing" };
        return WriteSafety {
            action: WriteGuard::Refuse,
            reasons: vec![
                format!(
                    "{} {} is newer than any version this CLI has evidence for (supported range {}). New builds are reported to reject tool-written drafts as corrupted.",
                    label, av, registry.range
                ),
                FIXTURE_CTA.to_string(),
            ],
            detected,
        };
    }
    if let Some(n) = si {
        if n > KNOWN_SCHEMA_INT_MAX {
            return WriteSafety {
                action: WriteGuard::Refuse,
                reasons: vec![
                    format!(
                        "Draft schema integer {} is newer than the newest known generation ({}), so a round-trip through this CLI is unverified and the app may reject the written draft.",
                        n, KNOWN_SCHEMA_INT_MAX
                    ),
                    FIXTURE_CTA.to_string(),
                ],
                detected,
            };
        }
    }

    let mut reasons: Vec<String> = Vec::new();
    if app_source == AppSource::Unknown
        && raw_source.is_some()
        && (app_version.is_some() || si.is_some())
    {
        reasons.push(format!(
            "Unrecognized app source \"{}\" — proceeding, but round-trip is unverified.",
            raw_source.unwrap()
        ));
    } else if app_source == AppSource::Unknown && app_version.is_some() {
        reasons.push(format!(
            "Draft has no app-source marker but carries app version {} (from last_modified_platform or a sibling file) — proceeding, but round-trip is unverified.",
            app_version.clone().unwrap()
        ));
    }
    if let Some(n) = si {
        if n < KNOWN_SCHEMA_INT_MAX {
            reasons.push(format!("Older schema generation {} — round-trip untested.", n));
        }
    }
    if !reasons.is_empty() {
        reasons.push(FIXTURE_CTA.to_string());
        return WriteSafety {
            action: WriteGuard::Warn,
            reasons,
            detected,
        };
    }
    WriteSafety {
        action: WriteGuard::Ok,
        reasons: vec![],
        detected,
    }
}

// ---------------------------------------------------------------------------
// detectVersion
// ---------------------------------------------------------------------------

fn detect_mask_field(draft: &Draft) -> MaskField {
    let mats = &draft.materials;
    let populated = |key: &str| -> bool {
        if let Some(Value::Array(arr)) = mats.extra.get(key) {
            !arr.is_empty()
        } else {
            match key {
                "masks" => !mats.masks.is_empty(),
                "common_mask" => !mats.common_mask.is_empty(),
                _ => false,
            }
        }
    };
    // also check common_masks via extra
    let check_common_masks = populated("common_masks");
    let mut present: Vec<MaskField> = Vec::new();
    if populated("masks") {
        present.push(MaskField::Mask);
    }
    if populated("common_mask") {
        present.push(MaskField::CommonMask);
    }
    if check_common_masks {
        present.push(MaskField::CommonMasks);
    }
    if present.len() > 1 {
        return MaskField::Both;
    }
    present.into_iter().next().unwrap_or(MaskField::None)
}

fn detect_text_ranges(draft: &Draft) -> bool {
    for val in &draft.materials.texts {
        // texts are Vec<Value> — each entry may be object with content field
        let content = match val {
            Value::Object(map) => map.get("content").and_then(|v| v.as_str()),
            _ => None,
        };
        let Some(content) = content else { continue };
        if let Ok(parsed) = serde_json::from_str::<Value>(content) {
            if let Some(arr) = parsed.get("styles").and_then(|v| v.as_array()) {
                if arr.len() > 1 {
                    return true;
                }
            }
        }
    }
    false
}

fn assess_support(
    app_source: AppSource,
    app_version: Option<&str>,
) -> (SupportStatus, Vec<String>, SupportEvidence) {
    let matrix = supported_app_versions(app_source);
    let mut notes = vec![format!(
        "{} supported range: {}",
        match app_source {
            AppSource::Cc => "CapCut",
            AppSource::Lv => "JianYing",
            AppSource::Unknown => "Unknown app",
        },
        matrix.range
    )];
    let Some(av) = app_version else {
        notes.push("No `platform.app_version` field — cannot assess compatibility".to_string());
        return (SupportStatus::Untested, notes, SupportEvidence::None);
    };
    if let Some(broken) = matrix.broken.iter().find(|e| at_least(Some(av), e.min)) {
        notes.push(format!(
            "Version {} listed as known-broken (>= {}: {})",
            av, broken.min, broken.reason
        ));
        return (SupportStatus::KnownBroken, notes, SupportEvidence::Reported);
    }
    if let Some(m) = matrix.supported.iter().find(|e| e.version == av) {
        return (SupportStatus::Supported, notes, m.evidence);
    }
    if matrix.expected_compatible.contains(&av) {
        notes.push(format!(
            "Version {} is expected-compatible (schema inspection only; no committed fixture — see docs/version-support.md)",
            av
        ));
        return (SupportStatus::Untested, notes, SupportEvidence::ExpectedCompatible);
    }
    if let Some(ceil) = matrix.ceiling {
        if !at_least(Some(ceil), av) {
            notes.push(format!(
                "Version {} is beyond the known range ({}) — no evidence this CLI can round-trip it",
                av, matrix.range
            ));
            return (SupportStatus::Untested, notes, SupportEvidence::None);
        }
    }
    let list = matrix
        .supported
        .iter()
        .map(|e| e.version)
        .collect::<Vec<_>>()
        .join(", ");
    let list = if list.is_empty() { "none yet".to_string() } else { list };
    notes.push(format!(
        "Version {} not in the evidence-backed list ({}) — may work",
        av, list
    ));
    (SupportStatus::Untested, notes, SupportEvidence::None)
}

pub fn detect_version(draft: &Draft) -> VersionInfo {
    let app_source = match get_platform_str(draft, "app_source").as_deref() {
        Some("cc") => AppSource::Cc,
        Some("lv") => AppSource::Lv,
        _ => AppSource::Unknown,
    };
    let app = match app_source {
        AppSource::Cc => "CapCut".to_string(),
        AppSource::Lv => "JianYing".to_string(),
        AppSource::Unknown => "unknown".to_string(),
    };
    let app_version = get_platform_str(draft, "app_version");
    let os = get_platform_str(draft, "os");

    let mask_field = detect_mask_field(draft);
    let has_text_ranges = detect_text_ranges(draft);
    let has_audio_fades = !draft.materials.audio_fades.is_empty();
    let new_version_field = draft.extra.get("new_version").cloned().unwrap_or(Value::Null);
    let last_modified_platform = draft
        .extra
        .get("last_modified_platform")
        .cloned()
        .unwrap_or(Value::Null);
    let si = schema_int(draft);

    let (mut status, mut notes, evidence) = assess_support(app_source, app_version.as_deref());

    if mask_field == MaskField::CommonMasks {
        notes.push(
            "Draft uses `common_masks` (JianYing 9.6+ era) — `mask` follows the populated variant, so new masks land there too".to_string(),
        );
    }
    if mask_field == MaskField::Both {
        notes.push(
            "Mask materials are split across variant arrays (`masks`/`common_mask`/`common_masks`) — the app reads only one; consolidate with `capcut migrate --from <ver> --to <ver>` (lint reports which arrays are populated)".to_string(),
        );
    }
    if is_pre_release(app_version.as_deref()) {
        let tuple = version_tuple(app_version.as_deref());
        let tuple_str = tuple.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(".");
        notes.push(format!(
            "{} is a pre-release build — version comparisons treat it as {}, and guidance derived from release-build evidence is withheld rather than extended to it",
            app_version.as_deref().unwrap_or(""),
            tuple_str
        ));
    }
    if app_source == AppSource::Lv {
        if let Some(ref av) = app_version {
            if !av.starts_with("5.9") {
                if status != SupportStatus::KnownBroken {
                    status = SupportStatus::Untested;
                }
                notes.push(format!(
                    "JianYing {} is post-5.9 — likely encrypted; pinning to 5.9 strongly recommended",
                    av
                ));
            }
        }
    }

    let safety = assess_write_safety(draft, None);
    for reason in &safety.reasons {
        notes.push(reason.clone());
    }

    VersionInfo {
        app,
        app_source,
        app_version,
        os,
        schema: SchemaInfo {
            mask_field,
            has_text_ranges,
            has_audio_fades,
            new_version_field,
            last_modified_platform,
            schema_int: si,
        },
        support: SupportInfo {
            status,
            notes,
            evidence,
            beyond_known_range: safety.detected.beyond_known_range,
            write_guard: safety.action,
        },
    }
}
