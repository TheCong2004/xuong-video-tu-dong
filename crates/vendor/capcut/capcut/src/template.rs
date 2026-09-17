use crate::draft::Draft;
use anyhow::{Context, Result};
use std::path::Path;
use std::fs;

/// Clone a template directory (containing draft_content.json + draft_info.json)
/// into a new draft directory. Returns the new draft_content.json path.
pub fn clone_from_template(template_dir: &Path, dest_dir: &Path, name: &str) -> Result<std::path::PathBuf> {
    fs::create_dir_all(dest_dir).with_context(|| format!("mkdir {}", dest_dir.display()))?;

    for entry in fs::read_dir(template_dir).with_context(|| format!("read {}", template_dir.display()))? {
        let entry = entry?;
        let src = entry.path();
        let dst = dest_dir.join(entry.file_name());
        if src.is_file() {
            fs::copy(&src, &dst).with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
        }
    }

    // patch name + id
    let draft_path = dest_dir.join("draft_content.json");
    if draft_path.exists() {
        let raw = fs::read_to_string(&draft_path)?;
        let mut draft: Draft = serde_json::from_str(raw.strip_prefix('\u{FEFF}').unwrap_or(&raw))?;
        draft.name = name.to_string();
        draft.id = uuid::Uuid::new_v4().to_string();
        let json = serde_json::to_string(&draft)?;
        fs::write(&draft_path, json)?;
    }

    // patch draft_info.json if present
    let info_path = dest_dir.join("draft_info.json");
    if info_path.exists() {
        if let Ok(raw) = fs::read_to_string(&info_path) {
            if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("id".to_string(), serde_json::json!(uuid::Uuid::new_v4().to_string()));
                    if let Some(draft_name) = obj.get_mut("draft_name") { *draft_name = serde_json::json!(name); }
                }
                let _ = fs::write(&info_path, serde_json::to_string(&v).unwrap());
            }
        }
    }

    Ok(draft_path)
}

pub fn builtin_template_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/_init")
}

/// List available template names from the templates vendored with this crate.
/// Includes `_init` (the draft skeleton) plus any sibling JSON templates.
pub fn list_templates() -> Vec<String> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(name) = p.file_name().and_then(|x| x.to_str()) {
                    out.push(name.to_string());
                }
            } else if p.extension().and_then(|x| x.to_str()) == Some("json") {
                if let Some(stem) = p.file_stem().and_then(|x| x.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}
