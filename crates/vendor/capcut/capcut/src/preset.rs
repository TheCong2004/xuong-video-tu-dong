use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::draft::Draft;
use crate::text_offsets::{from_stored_offset, repair_doubled_ranges, stored_text_length};

pub const PRESET_VERSION: i64 = 1;

pub const STYLE_FIELDS: &[&str] = &[
    "alignment",
    "font_size",
    "text_color",
    "typesetting",
    "letter_spacing",
    "line_spacing",
    "line_feed",
    "line_max_width",
    "force_apply_line_max_width",
    "fixed_width",
    "fixed_height",
    "text_alpha",
    "has_shadow",
    "shadow_alpha",
    "shadow_angle",
    "shadow_color",
    "shadow_distance",
    "shadow_smoothing",
    "has_border",
    "border_width",
    "border_color",
    "border_alpha",
    "has_text_shadow_config",
    "background_color",
    "background_alpha",
    "background_style",
    "background_round_radius",
    "background_width",
    "background_height",
    "background_horizontal_offset",
    "background_vertical_offset",
    "font_id",
    "font_name",
    "font_path",
    "font_resource_id",
    "bold",
    "italic",
    "underline",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextStylePreset {
    #[serde(rename = "capcutCliPreset")]
    pub capcut_cli_preset: i64,
    pub style: serde_json::Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bubble: Option<BubbleRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_ranges: Option<Vec<TextRangePreset>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BubbleRef {
    pub effect_id: String,
    pub resource_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRangePreset {
    pub start: i64,
    pub end: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_alpha: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
}

fn hex_to_rgb01(hex: &str) -> [f64; 3] {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h.get(0..2).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0;
    let g = u8::from_str_radix(&h.get(2..4).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0;
    let b = u8::from_str_radix(&h.get(4..6).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0;
    [r, g, b]
}

fn rgb01_to_hex(rgb: [f64; 3]) -> String {
    let ch = |v: f64| {
        let n = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("{n:02X}")
    };
    format!("#{}{}{}", ch(rgb[0]), ch(rgb[1]), ch(rgb[2]))
}

fn parse_content(mat: &Value) -> Option<Value> {
    let content = mat.get("content")?.as_str()?;
    serde_json::from_str(content).ok()
}

pub fn make_preset(draft: &Draft, segment_id: &str) -> Result<(TextStylePreset, String), String> {
    let seg = draft
        .tracks
        .iter()
        .flat_map(|t| &t.segments)
        .find(|s| s.id == segment_id)
        .ok_or_else(|| format!("Segment not found: {segment_id}"))?;
    let mat = draft
        .materials
        .texts
        .iter()
        .find(|v| v.get("id").and_then(|x| x.as_str()) == Some(&seg.material_id))
        .ok_or_else(|| format!("Text material not found for segment {segment_id}"))?;

    let mut style = serde_json::Map::new();
    if let Some(obj) = mat.as_object() {
        for f in STYLE_FIELDS {
            if let Some(v) = obj.get(*f) {
                style.insert(f.to_string(), v.clone());
            }
        }
    }
    // fill gaps from content styles[0]
    if let Some(content) = parse_content(mat) {
        if let Some(styles) = content.get("styles").and_then(|v| v.as_array()) {
            if let Some(base) = styles.first().and_then(|v| v.as_object()) {
                if !style.contains_key("font_size") {
                    if let Some(sz) = base.get("size") { style.insert("font_size".to_string(), sz.clone()); }
                }
                for k in &["bold", "italic", "underline"] {
                    if !style.contains_key(*k) {
                        if let Some(v) = base.get(*k) { style.insert(k.to_string(), v.clone()); }
                    }
                }
                if !style.contains_key("text_color") {
                    if let Some(color) = base.get("fill").and_then(|f| f.get("content")).and_then(|c| c.get("solid")).and_then(|s| s.get("color")).and_then(|c| c.as_array()) {
                        if color.len() == 3 {
                            let rgb = [color[0].as_f64().unwrap_or(0.0), color[1].as_f64().unwrap_or(0.0), color[2].as_f64().unwrap_or(0.0)];
                            style.insert("text_color".to_string(), Value::String(rgb01_to_hex(rgb)));
                        }
                    }
                }
                if !style.contains_key("font_path") {
                    if let Some(p) = base.get("font").and_then(|f| f.get("path")).and_then(|x| x.as_str()) {
                        style.insert("font_path".to_string(), Value::String(p.to_string()));
                    }
                }
                if !style.contains_key("font_id") {
                    if let Some(id) = base.get("font").and_then(|f| f.get("id")).and_then(|x| x.as_str()) {
                        style.insert("font_id".to_string(), Value::String(id.to_string()));
                    }
                }
            }
        }
    }

    let mut preset = TextStylePreset {
        capcut_cli_preset: PRESET_VERSION,
        style,
        transform: None,
        bubble: None,
        text_ranges: None,
    };

    // transform from segment clip
    if let Some(clip) = seg.extra.get("clip").and_then(|v| v.as_object()) {
        if let Some(tr) = clip.get("transform").and_then(|v| v.as_object()) {
            if let (Some(x), Some(y)) = (tr.get("x").and_then(|v| v.as_f64()), tr.get("y").and_then(|v| v.as_f64())) {
                preset.transform = Some(Transform { x, y });
            }
        }
    }

    if let Some(eid) = mat.get("bubble_effect_id").and_then(|v| v.as_str()) {
        if !eid.is_empty() {
            if let Some(rid) = mat.get("bubble_resource_id").and_then(|v| v.as_str()) {
                preset.bubble = Some(BubbleRef { effect_id: eid.to_string(), resource_id: rid.to_string() });
            }
        }
    }

    // text ranges
    if let Some(content) = parse_content(mat) {
        if let Some(styles) = content.get("styles").and_then(|v| v.as_array()) {
            if styles.len() > 1 {
                let text = content.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let mut stored: Vec<[i64; 2]> = Vec::new();
                for s in styles {
                    if let Some(r) = s.get("range").and_then(|v| v.as_array()) {
                        if r.len() == 2 {
                            stored.push([r[0].as_i64().unwrap_or(0), r[1].as_i64().unwrap_or(0)]);
                        }
                    }
                }
                let repaired = repair_doubled_ranges(text, &stored).unwrap_or(stored.clone());
                let mut ranges = Vec::new();
                for (idx, s) in styles.iter().enumerate() {
                    if idx >= repaired.len() { break; }
                    let [rs, re] = repaired[idx];
                    let start = from_stored_offset(text, rs as usize) as i64;
                    let end = from_stored_offset(text, re as usize) as i64;
                    let solid = s.get("fill").and_then(|f| f.get("content")).and_then(|c| c.get("solid"));
                    let font_color = solid.and_then(|sol| sol.get("color")).and_then(|c| c.as_array()).map(|a| {
                        let rgb = [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0), a[2].as_f64().unwrap_or(0.0)];
                        rgb01_to_hex(rgb)
                    });
                    ranges.push(TextRangePreset {
                        start, end,
                        font_color,
                        font_size: s.get("size").and_then(|v| v.as_f64()),
                        font_alpha: solid.and_then(|sol| sol.get("alpha")).and_then(|v| v.as_f64()),
                        bold: s.get("bold").and_then(|v| v.as_bool()),
                        italic: s.get("italic").and_then(|v| v.as_bool()),
                        underline: s.get("underline").and_then(|v| v.as_bool()),
                    });
                }
                if !ranges.is_empty() {
                    preset.text_ranges = Some(ranges);
                }
            }
        }
    }

    let mid = mat.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    Ok((preset, mid))
}

pub fn apply_preset(
    draft: &mut Draft,
    segment_id: &str,
    preset: &TextStylePreset,
) -> Result<String, String> {
    if preset.capcut_cli_preset != PRESET_VERSION {
        return Err(format!("Unsupported preset version: {}", preset.capcut_cli_preset));
    }
    // validate style types
    let numeric = ["font_size","alignment","typesetting","letter_spacing","line_spacing","text_alpha","shadow_alpha","shadow_angle","shadow_distance","shadow_smoothing","border_width","border_alpha","background_alpha","background_style","background_round_radius","background_width","background_height","background_horizontal_offset","background_vertical_offset"];
    for f in &numeric {
        if let Some(v) = preset.style.get(*f) {
            if !v.is_number() { return Err(format!("Preset style.{f} must be a number")); }
        }
    }

    let seg_idx = draft.tracks.iter().position(|t| t.segments.iter().any(|s| s.id == segment_id))
        .ok_or_else(|| format!("Segment not found: {segment_id}"))?;
    let seg_pos = draft.tracks[seg_idx].segments.iter().position(|s| s.id == segment_id).unwrap();
    let mat_id = draft.tracks[seg_idx].segments[seg_pos].material_id.clone();
    let mat_pos = draft.materials.texts.iter().position(|v| v.get("id").and_then(|x| x.as_str()) == Some(&mat_id))
        .ok_or_else(|| format!("Text material not found for segment {segment_id}"))?;

    for f in STYLE_FIELDS {
        if let Some(v) = preset.style.get(*f) {
            draft.materials.texts[mat_pos].as_object_mut().unwrap().insert(f.to_string(), v.clone());
        }
    }
    // mirror into content styles[0]
    if let Some(content_str) = draft.materials.texts[mat_pos].get("content").and_then(|v| v.as_str()).map(|s| s.to_string()) {
        if let Ok(mut content) = serde_json::from_str::<Value>(&content_str) {
            let text_for_len = content.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let text_len_n = stored_text_length(&text_for_len) as i64;
            let has_ranges = preset.text_ranges.as_ref().map(|v| !v.is_empty()).unwrap_or(false);
            // capture len before mutable borrow of styles
            let styles_len = content.get("styles").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            let needs_collapse = !has_ranges && styles_len > 1;
            if let Some(styles) = content.get_mut("styles").and_then(|v| v.as_array_mut()) {
                if let Some(base) = styles.first_mut().and_then(|v| v.as_object_mut()) {
                    if let Some(sz) = preset.style.get("font_size").and_then(|v| v.as_f64()) {
                        base.insert("size".to_string(), serde_json::json!(sz));
                    }
                    for k in &["bold","italic","underline"] {
                        if let Some(v) = preset.style.get(*k).and_then(|x| x.as_bool()) {
                            base.insert(k.to_string(), serde_json::json!(v));
                        }
                    }
                    if let Some(color) = preset.style.get("text_color").and_then(|v| v.as_str()) {
                        let rgb = hex_to_rgb01(color);
                        base.insert("fill".to_string(), serde_json::json!({
                            "alpha": 1,
                            "content": { "render_type": "solid", "solid": { "alpha": 1, "color": rgb }}
                        }));
                    }
                    if let Some(p) = preset.style.get("font_path").and_then(|v| v.as_str()) {
                        let fid = preset.style.get("font_id").and_then(|v| v.as_str()).unwrap_or("");
                        base.insert("font".to_string(), serde_json::json!({"id": fid, "path": p}));
                    }
                    if needs_collapse {
                        base.insert("range".to_string(), serde_json::json!([0, text_len_n]));
                    }
                }
                if needs_collapse {
                    if let Some(first) = styles.first().cloned() {
                        *styles = vec![first];
                    }
                }
            }
            if content.get("styles").is_some() {
                draft.materials.texts[mat_pos].as_object_mut().unwrap().insert("content".to_string(), Value::String(serde_json::to_string(&content).unwrap()));
            }
        }
    }

    if let Some(tr) = &preset.transform {
        let seg = &mut draft.tracks[seg_idx].segments[seg_pos];
        if seg.extra.get("clip").is_some() {
            if let Some(clip) = seg.extra.get_mut("clip").and_then(|v| v.as_object_mut()) {
                clip.insert("transform".to_string(), serde_json::json!({"x": tr.x, "y": tr.y}));
            }
        }
    }

    // text_ranges: delegate to inline logic - simplified: just store ranges via content rewrite
    if let Some(ranges) = &preset.text_ranges {
        if !ranges.is_empty() {
            // Build text ranges directly - simplified implementation
            // We rewrite content.styles to include ranges
            if let Some(content_str) = draft.materials.texts[mat_pos].get("content").and_then(|v| v.as_str()).map(|s| s.to_string()) {
                if let Ok(mut content) = serde_json::from_str::<Value>(&content_str) {
                    let text = content.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let text_len = text.encode_utf16().count() as i64;
                    let filtered: Vec<&TextRangePreset> = ranges.iter().filter(|r| r.start < r.end && r.start < text_len).collect();
                    if !filtered.is_empty() {
                        // Build styled ranges - simplified: use base style for gaps
                        let base_style = content.get("styles").and_then(|v| v.as_array()).and_then(|a| a.first().cloned()).unwrap_or(json!({}));
                        let mut new_styles: Vec<Value> = Vec::new();
                        let mut cursor = 0i64;
                        for r in &filtered {
                            let s = (*r).start.min(text_len);
                            let e = (*r).end.min(text_len);
                            if s > cursor {
                                let mut gap = base_style.clone();
                                if let Some(obj) = gap.as_object_mut() {
                                    obj.insert("range".to_string(), json!([cursor, s]));
                                }
                                new_styles.push(gap);
                            }
                            let mut block = base_style.clone();
                            if let Some(obj) = block.as_object_mut() {
                                obj.insert("range".to_string(), json!([s, e]));
                                if let Some(c) = &r.font_color {
                                    let rgb = hex_to_rgb01(c);
                                    obj.insert("fill".to_string(), json!({"alpha": 1, "content": {"render_type": "solid", "solid": {"alpha": r.font_alpha.unwrap_or(1.0), "color": rgb}}}));
                                }
                                if let Some(sz) = r.font_size { obj.insert("size".to_string(), json!(sz)); }
                                if let Some(b) = r.bold { obj.insert("bold".to_string(), json!(b)); }
                                if let Some(b) = r.italic { obj.insert("italic".to_string(), json!(b)); }
                                if let Some(b) = r.underline { obj.insert("underline".to_string(), json!(b)); }
                            }
                            new_styles.push(block);
                            cursor = e;
                        }
                        if cursor < text_len {
                            let mut tail = base_style.clone();
                            if let Some(obj) = tail.as_object_mut() {
                                obj.insert("range".to_string(), json!([cursor, text_len]));
                            }
                            new_styles.push(tail);
                        }
                        content["styles"] = Value::Array(new_styles);
                        draft.materials.texts[mat_pos].as_object_mut().unwrap().insert("content".to_string(), Value::String(serde_json::to_string(&content).unwrap()));
                    }
                }
            }
        }
    }

    Ok(mat_id)
}

pub fn parse_preset(raw: &str) -> Result<TextStylePreset, String> {
    let v: Value = serde_json::from_str(raw).map_err(|e| format!("Preset is not valid JSON: {e}"))?;
    let obj = v.as_object().ok_or("Preset must be a JSON object")?;
    if !obj.contains_key("capcutCliPreset") {
        return Err("Not a capcut-cli preset (missing capcutCliPreset)".to_string());
    }
    let ver = obj.get("capcutCliPreset").and_then(|x| x.as_i64()).unwrap_or(-1);
    if ver != PRESET_VERSION {
        return Err(format!("Unsupported preset version: {ver}"));
    }
    serde_json::from_value::<TextStylePreset>(v).map_err(|e| e.to_string())
}
