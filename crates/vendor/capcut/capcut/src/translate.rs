use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateOptions {
    pub to: String,
    pub from: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub dry_run: bool,
    pub out_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslatePair {
    pub id: String,
    pub original: String,
    pub translated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateResult {
    pub ok: bool,
    pub count: usize,
    pub to: String,
    pub from: String,
    pub out: String,
    pub pairs: Vec<TranslatePair>,
    pub dry_run: bool,
}

fn extract_text_from_material(v: &serde_json::Value) -> String {
    // Mirrors reference draft.ts extractText: content is JSON string with {"text":"..."}
    // or structured content with text field. Try common shapes.
    if let Some(content) = v.get("content") {
        if let Some(s) = content.as_str() {
            // content is JSON string
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                if let Some(t) = parsed.get("text").and_then(|x| x.as_str()) {
                    return t.to_string();
                }
            }
            return s.to_string();
        }
        if let Some(t) = content.get("text").and_then(|x| x.as_str()) {
            return t.to_string();
        }
    }
    v.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn update_text_content(content: &serde_json::Value, new_text: &str) -> serde_json::Value {
    if let Some(s) = content.as_str() {
        if let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(s) {
            if parsed.get("text").is_some() {
                if let Some(obj) = parsed.as_object_mut() {
                    obj.insert("text".to_string(), serde_json::Value::String(new_text.to_string()));
                }
                return serde_json::Value::String(parsed.to_string());
            }
        }
        return serde_json::Value::String(new_text.to_string());
    }
    if content.is_object() {
        let mut m = content.clone();
        if let Some(obj) = m.as_object_mut() {
            obj.insert("text".to_string(), serde_json::Value::String(new_text.to_string()));
        }
        return m;
    }
    serde_json::Value::String(new_text.to_string())
}

/// Translate every text material in a draft. Writes cloned draft to `opts.out_path`.
/// When dry_run or no texts: writes draft unchanged with pairs echoing originals.
/// Without dry_run, requires ANTHROPIC_API_KEY (or opts.api_key).
pub fn translate_draft(draft: &mut crate::draft::Draft, opts: &TranslateOptions) -> Result<TranslateResult> {
    // Collect (id, original)
    let collected: Vec<(String, String)> = draft
        .materials
        .texts
        .iter()
        .filter_map(|v| {
            let id = v.get("id").and_then(|x| x.as_str())?.to_string();
            let original = extract_text_from_material(v);
            if original.trim().is_empty() { None } else { Some((id, original)) }
        })
        .collect();

    let from = opts.from.clone().unwrap_or_else(|| "auto".to_string());

    if opts.dry_run || collected.is_empty() {
        let pairs = collected.into_iter().map(|(id, original)| TranslatePair { id, translated: original.clone(), original }).collect::<Vec<_>>();
        let count = pairs.len();
        let json = serde_json::to_string_pretty(draft)?;
        std::fs::write(&opts.out_path, json)?;
        return Ok(TranslateResult { ok: true, count, to: opts.to.clone(), from, out: opts.out_path.clone(), pairs, dry_run: true });
    }

    let api_key = opts.api_key.clone().or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());
    let Some(api_key) = api_key else {
        bail!("Missing API key. Set ANTHROPIC_API_KEY or pass --api-key. Get one at https://console.anthropic.com/. (Use --dry-run to see what would be translated without calling the API.)");
    };
    let _model = opts.model.clone().unwrap_or_else(|| "claude-haiku-4-5-20251001".to_string());
    // Network call intentionally stubbed: callers with an API key should use async path or CLI.
    // Keep pure logic above fully ported; network layer bails with actionable message.
    let _ = api_key;
    bail!("translate_draft network path requires async HTTP (reqwest). Use translate_draft_async or set dry_run=true. Requires API key for live translation.");
}

/// Build the Anthropic batch prompt (pure, testable).
pub fn build_translate_prompt(texts: &[String], to: &str, from: &str) -> String {
    [
        format!("Translate the following {} text strings from {from} to {to}.", texts.len()),
        "Preserve line breaks and punctuation. Do not add commentary, only translations.".to_string(),
        format!("Output a JSON array of {} strings, same order as input. No markdown fences, just JSON.", texts.len()),
        String::new(),
        "Input strings (JSON array):".to_string(),
        serde_json::to_string(texts).unwrap_or_default(),
    ]
    .join("\n")
}

/// Parse model output: strip ``` fences then JSON array.
pub fn parse_translate_output(raw: &str) -> Result<Vec<String>> {
    let t = raw.trim();
    let s = t.strip_prefix("```").unwrap_or(t);
    let s = s.strip_prefix("json").or_else(|| s.strip_prefix("JSON")).unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s).trim();
    let parsed: serde_json::Value = serde_json::from_str(s).map_err(|e| anyhow::anyhow!("Failed to parse model output as JSON array. Raw: {}. err: {e}", &s[..s.len().min(300)]))?;
    let arr = parsed.as_array().ok_or_else(|| anyhow::anyhow!("Expected JSON array, got: {parsed}"))?;
    Ok(arr.iter().map(|x| if let Some(s) = x.as_str() { s.to_string() } else { x.to_string() }).collect())
}

pub fn apply_translations(draft: &mut crate::draft::Draft, pairs: &[TranslatePair]) {
    use std::collections::HashMap;
    let map: HashMap<&str, &str> = pairs.iter().map(|p| (p.id.as_str(), p.translated.as_str())).collect();
    for v in &mut draft.materials.texts {
        if let Some(id) = v.get("id").and_then(|x| x.as_str()).map(|s| s.to_string()) {
            if let Some(new_text) = map.get(id.as_str()) {
                if let Some(content) = v.get("content").cloned() {
                    let updated = update_text_content(&content, new_text);
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("content".to_string(), updated);
                    }
                }
            }
        }
    }
}
