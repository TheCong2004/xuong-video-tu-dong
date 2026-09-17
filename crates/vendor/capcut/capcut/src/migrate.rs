use capcut_core::error::Result as CoreResult;
use serde_json::Value;

// ---------------------------------------------------------------------------
// migrate — mirrors reference/src/migrate.ts
// ---------------------------------------------------------------------------

pub const RESTAMP_FIELDS: &[&str] = &[
    "version",
    "new_version",
    "platform",
    "last_modified_platform",
    "color_space",
    "render_index_track_mode_on",
    "free_render_index_mode_on",
    "source",
];

const RESTAMP_FILL_FIELDS: &[&str] = &["config"];

#[derive(Debug, Clone)]
pub struct RestampResult {
    pub copied: Vec<String>,
    pub filled: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub changed: bool,
    pub applied: Vec<String>,
    pub warnings: Vec<String>,
}

/// Rewrite schema markers from a donor draft into `draft`.
/// Only the fields in RESTAMP_FIELDS are copied; RESTAMP_FILL_FIELDS only if absent.
pub fn restamp_draft(
    draft: &mut serde_json::Map<String, Value>,
    donor: &serde_json::Map<String, Value>,
    donor_label: &str,
) -> RestampResult {
    let mut copied = Vec::new();
    let mut filled = Vec::new();
    for &field in RESTAMP_FIELDS {
        if let Some(v) = donor.get(field) {
            draft.insert(field.to_string(), v.clone());
            copied.push(field.to_string());
        }
    }
    for &field in RESTAMP_FILL_FIELDS {
        if !draft.contains_key(field) {
            if let Some(v) = donor.get(field) {
                draft.insert(field.to_string(), v.clone());
                filled.push(field.to_string());
            }
        }
    }
    let warnings = if copied.is_empty() {
        vec![format!("donor '{donor_label}' has no restamp fields to copy")]
    } else {
        Vec::new()
    };
    RestampResult { copied, filled, warnings }
}

fn parse_ver(s: &str) -> Option<i64> {
    // simple: major * 1_000_000 + minor * 1_000 + patch
    let parts: Vec<&str> = s.split('.').collect();
    let maj: i64 = parts.first()?.parse().ok()?;
    let min: i64 = parts.get(1).and_then(|x| x.parse().ok()).unwrap_or(0);
    let pat: i64 = parts.get(2).and_then(|x| x.parse().ok()).unwrap_or(0);
    Some(maj * 1_000_000 + min * 1_000 + pat)
}

fn is_jump_across_mask_rename(from: &str, to: &str) -> &'static str {
    let f = parse_ver(from).unwrap_or(0);
    let t = parse_ver(to).unwrap_or(0);
    // threshold ~ 5.8.0 where masks -> common_masks
    let threshold = 5_008_000;
    if f < threshold && t >= threshold {
        "legacy-to-new"
    } else if f >= threshold && t < threshold {
        "new-to-legacy"
    } else {
        "none"
    }
}

fn move_mask_entries(
    draft: &mut serde_json::Map<String, Value>,
    from_key: &str,
    to_key: &str,
) -> usize {
    let Some(materials) = draft
        .get_mut("materials")
        .and_then(|v| v.as_object_mut())
    else {
        return 0;
    };
    let Some(arr) = materials.remove(from_key).and_then(|v| {
        if let Value::Array(a) = v { Some(a) } else { None }
    }) else {
        return 0;
    };
    let n = arr.len();
    if n == 0 {
        return 0;
    }
    let dest = materials
        .entry(to_key.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Value::Array(dest_arr) = dest {
        dest_arr.extend(arr);
    }
    n
}

/// Migrate a draft map between schema versions.
/// Uses `assess_write_safety` style judgement to warn on breaking jumps.
pub fn migrate_draft(
    draft: &mut serde_json::Map<String, Value>,
    from: &str,
    to: &str,
) -> CoreResult<MigrationResult> {
    if from == to {
        return Ok(MigrationResult {
            changed: false,
            applied: Vec::new(),
            warnings: vec!["from == to — nothing to migrate".to_string()],
        });
    }
    let mut applied = Vec::new();
    let mut warnings = Vec::new();

    let direction = is_jump_across_mask_rename(from, to);
    match direction {
        "legacy-to-new" => {
            let n = move_mask_entries(draft, "masks", "common_masks");
            if n > 0 {
                applied.push(format!("masks -> common_masks ({n} entries)"));
            }
            // also migrate common_mask singular -> common_masks
            let n2 = move_mask_entries(draft, "common_mask", "common_masks");
            if n2 > 0 {
                applied.push(format!("common_mask -> common_masks ({n2} entries)"));
            }
        }
        "new-to-legacy" => {
            let n = move_mask_entries(draft, "common_masks", "masks");
            if n > 0 {
                applied.push(format!("common_masks -> masks ({n} entries)"));
            }
        }
        _ => {}
    }

    // Version stamp update
    if let Some(v) = draft.get_mut("version") {
        *v = Value::String(to.to_string());
        applied.push(format!("version {from} -> {to}"));
    }
    if applied.is_empty() {
        warnings.push(format!("no structural changes for {from} -> {to}"));
    }

    // Safety note using version helpers when available
    let from_v = parse_ver(from);
    let to_v = parse_ver(to);
    if let (Some(f), Some(t)) = (from_v, to_v) {
        if (t - f).abs() > 1_000_000 {
            warnings.push("large version jump — verify in CapCut app".to_string());
        }
    }

    Ok(MigrationResult {
        changed: !applied.is_empty(),
        applied,
        warnings,
    })
}

/// Assess whether writing `draft` is safe given a store version string.
/// Thin wrapper around version::assess_write_safety that works on a JSON map.
pub fn assess_migration_safety(
    draft: &serde_json::Map<String, Value>,
    store_version: Option<&str>,
) -> Vec<String> {
    let draft_version = draft
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let mut notes = Vec::new();
    if store_version.is_none() {
        notes.push(format!(
            "no store version known — draft version is {draft_version}; use --force-write to override"
        ));
    } else if let Some(sv) = store_version {
        if draft_version != sv {
            notes.push(format!("draft version {draft_version} differs from store {sv}"));
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(v: Value) -> serde_json::Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn restamp_copies_fields() {
        let donor = map(json!({"version":"9.3","platform":{"app_version":"9.3"},"extra":"keep"}));
        let mut draft = map(json!({"name":"old"}));
        let r = restamp_draft(&mut draft, &donor, "donor.json");
        assert!(r.copied.contains(&"version".to_string()));
        assert_eq!(draft["version"], json!("9.3"));
    }

    #[test]
    fn migrate_legacy_to_new() {
        let mut draft = map(json!({
            "version":"5.0.0",
            "materials":{"masks":[{"id":"m1"}]}
        }));
        let res = migrate_draft(&mut draft, "5.0.0", "6.0.0").unwrap();
        assert!(res.changed);
        let mats = draft["materials"].as_object().unwrap();
        assert!(mats.contains_key("common_masks"));
    }

    #[test]
    fn migrate_noop_same_version() {
        let mut draft = map(json!({"version":"6.0.0","materials":{}}));
        let res = migrate_draft(&mut draft, "6.0.0", "6.0.0").unwrap();
        assert!(!res.changed);
    }

    #[test]
    fn assess_safety_no_store() {
        let draft = map(json!({"version":"9.3"}));
        let notes = assess_migration_safety(&draft, None);
        assert!(!notes.is_empty());
    }
}
