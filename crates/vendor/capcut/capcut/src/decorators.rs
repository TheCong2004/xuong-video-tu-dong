#![allow(dead_code, unused_variables, unused_assignments, non_snake_case)]
use std::collections::HashMap;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::draft::Draft;
use crate::enums::{find_enum, slugs_for, Category, Namespace};
use crate::version::at_least;

// ---------------------------------------------------------------------------
// Keyframe property mapping
// ---------------------------------------------------------------------------

pub const PROPERTY_MAP: &[(&str, &str)] = &[
    ("position_x", "KFTypePositionX"),
    ("position_y", "KFTypePositionY"),
    ("rotation", "KFTypeRotation"),
    ("scale_x", "KFTypeScaleX"),
    ("scale_y", "KFTypeScaleY"),
    ("uniform_scale", "UNIFORM_SCALE"),
    ("alpha", "KFTypeAlpha"),
    ("saturation", "KFTypeSaturation"),
    ("contrast", "KFTypeContrast"),
    ("brightness", "KFTypeBrightness"),
    ("volume", "KFTypeVolume"),
];

pub const PROPERTY_ALIASES: &[(&str, &str)] = &[
    ("scale", "uniform_scale"),
    ("x", "position_x"),
    ("y", "position_y"),
    ("opacity", "alpha"),
];

pub fn keyframe_properties() -> Vec<String> {
    PROPERTY_MAP.iter().map(|(k, _)| k.to_string()).collect()
}

pub fn keyframe_property_aliases() -> HashMap<String, String> {
    PROPERTY_ALIASES
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

pub fn resolve_keyframe_property(name: &str) -> Result<String, String> {
    let key = name.trim();
    for (k, _) in PROPERTY_MAP {
        if *k == key {
            return Ok(key.to_string());
        }
    }
    for (alias, canonical) in PROPERTY_ALIASES {
        if *alias == key {
            return Ok(canonical.to_string());
        }
    }
    let aliases = PROPERTY_ALIASES
        .iter()
        .map(|(a, c)| format!("{a}={c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let supported = PROPERTY_MAP
        .iter()
        .map(|(k, _)| *k)
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "Unsupported keyframe property: {name}. Supported: {supported} (aliases: {aliases})"
    ))
}

pub fn keyframe_property_types() -> Vec<String> {
    PROPERTY_MAP.iter().map(|(_, v)| v.to_string()).collect()
}

pub fn property_type_for(canonical: &str) -> Option<&'static str> {
    for (k, v) in PROPERTY_MAP {
        if *k == canonical {
            return Some(v);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Helpers: find segment / text material
#[allow(dead_code)]
fn find_segment_mut<'a>(
    draft: &'a mut Draft,
    seg_id: &str,
) -> Option<(&'a mut crate::draft::Track, usize)> {
    for track in &mut draft.tracks {
        for (idx, seg) in track.segments.iter().enumerate() {
            if seg.id == seg_id {
                return Some((track, idx));
            }
        }
    }
    None
}

#[allow(dead_code)]
fn find_text_material_mut<'a>(
    draft: &'a mut Draft,
    seg_id: &str,
) -> Result<(usize, Value), String> {
    let track_idx = draft
        .tracks
        .iter()
        .position(|t| t.segments.iter().any(|s| s.id == seg_id))
        .ok_or_else(|| format!("Segment not found: {seg_id}"))?;
    let seg = draft.tracks[track_idx]
        .segments
        .iter()
        .find(|s| s.id == seg_id)
        .unwrap();
    let mat_id = seg.material_id.clone();
    let pos = draft
        .materials
        .texts
        .iter()
        .position(|v| v.get("id").and_then(|x| x.as_str()) == Some(&mat_id))
        .ok_or_else(|| format!("Text material not found for segment {seg_id}"))?;
    Ok((pos, draft.materials.texts[pos].clone()))
}

// ---------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------

pub fn transition_slugs(namespace: Namespace) -> Vec<String> {
    slugs_for(Category::Transitions, namespace)
}

pub fn add_transition(
    draft: &mut Draft,
    segment_id: &str,
    slug: &str,
    duration_us: Option<i64>,
    namespace: Namespace,
) -> Result<(String, String, i64), String> {
    let meta = find_enum(Category::Transitions, slug, namespace, None)
        .ok_or_else(|| {
            let hint = if namespace == Namespace::JianYing {
                " --jianying"
            } else {
                ""
            };
            format!(
                "Unknown transition: {slug}. Run 'capcut enums --transitions{hint}' for the full list."
            )
        })?;
    let name = meta.name.clone().ok_or_else(|| format!("Unknown transition: {slug}"))?;
    let effect_id = meta
        .effect_id
        .clone()
        .ok_or_else(|| format!("Unknown transition: {slug}"))?;
    let resource_id = meta
        .resource_id
        .clone()
        .ok_or_else(|| format!("Unknown transition: {slug}"))?;

    // find segment
    let seg_exists = draft.tracks.iter().any(|t| t.segments.iter().any(|s| s.id == segment_id));
    if !seg_exists {
        return Err(format!("Segment not found: {segment_id}"));
    }
    // refuse stacking
    let existing = draft.tracks.iter().any(|t| {
        t.segments.iter().any(|s| {
            if s.id != segment_id {
                return false;
            }
            if let Some(Value::Array(refs)) = s.extra.get("extra_material_refs") {
                for r in refs {
                    if let Some(rid) = r.as_str() {
                        if draft
                            .materials
                            .transitions
                            .iter()
                            .any(|m| m.get("id").and_then(|x| x.as_str()) == Some(rid))
                        {
                            return true;
                        }
                    }
                }
            }
            false
        })
    });
    if existing {
        return Err("Segment already has a transition. Remove it first.".to_string());
    }

    let dur = duration_us
        .or(meta.default_duration)
        .or(meta.duration)
        .unwrap_or(500_000);
    let id = Uuid::new_v4().to_string();
    let entry = json!({
        "category_id": "",
        "category_name": "",
        "duration": dur,
        "effect_id": effect_id,
        "id": id,
        "is_overlap": meta.is_overlap.unwrap_or(false),
        "name": name,
        "platform": "all",
        "resource_id": resource_id,
        "type": "transition",
    });
    draft.materials.transitions.push(entry);
    // attach to segment
    for track in &mut draft.tracks {
        for seg in &mut track.segments {
            if seg.id == segment_id {
                let refs = seg
                    .extra
                    .entry("extra_material_refs")
                    .or_insert_with(|| Value::Array(vec![]));
                if let Value::Array(arr) = refs {
                    arr.push(Value::String(id.clone()));
                }
            }
        }
    }
    Ok((id, name, dur))
}

// ---------------------------------------------------------------------------
// Masks
// ---------------------------------------------------------------------------

pub const MASK_FIELDS: &[&str] = &["common_masks", "common_mask", "masks"];

fn mask_aliases() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("linear".to_string(), "split".to_string());
    m.insert("mirror".to_string(), "filmstrip".to_string());
    m.insert("star".to_string(), "stars".to_string());
    m
}

pub fn mask_slugs(namespace: Namespace) -> Vec<String> {
    let mut out: Vec<String> = mask_aliases().keys().cloned().collect();
    for s in slugs_for(Category::Masks, namespace) {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct MaskOptions {
    pub center_x: Option<f64>,
    pub center_y: Option<f64>,
    pub size: Option<f64>,
    pub rotation: Option<f64>,
    pub feather: Option<f64>,
    pub invert: Option<bool>,
    pub rect_width: Option<f64>,
    pub round_corner: Option<f64>,
    pub field: Option<String>,
}

fn mask_entries(draft: &Draft, field: &str) -> Vec<Value> {
    match field {
        "masks" => draft.materials.masks.clone(),
        "common_mask" => draft.materials.common_mask.clone(),
        _ => draft
            .materials
            .extra
            .get(field)
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default(),
    }
}

fn all_mask_entries(draft: &Draft) -> Vec<Value> {
    let mut out = Vec::new();
    for f in MASK_FIELDS {
        out.extend(mask_entries(draft, f));
    }
    out
}

pub fn mask_target_field(draft: &Draft, override_field: Option<&str>) -> String {
    if let Some(o) = override_field {
        return o.to_string();
    }
    // version-gated: lv/jianying
    let source = draft
        .platform
        .get("app_source")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let version = draft
        .platform
        .get("app_version")
        .and_then(|v| v.as_str());
    if source == "lv" {
        if let Some(ver) = version {
            return if at_least(Some(ver), "9.6") {
                "common_masks".to_string()
            } else {
                "masks".to_string()
            };
        }
    }
    for f in MASK_FIELDS {
        if !mask_entries(draft, f).is_empty() {
            return f.to_string();
        }
    }
    if source == "lv" {
        "masks".to_string()
    } else {
        "common_mask".to_string()
    }
}

pub fn add_mask(
    draft: &mut Draft,
    segment_id: &str,
    slug: &str,
    opts: MaskOptions,
    namespace: Namespace,
) -> Result<(String, String, String), String> {
    // handle --off style via separate function; here slug="off" is invalid
    let aliases = mask_aliases();
    let meta = find_enum(Category::Masks, slug, namespace, Some(&aliases))
        .ok_or_else(|| {
            let hint = if namespace == Namespace::JianYing {
                " --jianying"
            } else {
                ""
            };
            format!("Unknown mask: {slug}. Run 'capcut enums --masks{hint}' for the full list.")
        })?;
    let name = meta.name.clone().ok_or_else(|| format!("Unknown mask: {slug}"))?;
    let effect_id = meta
        .effect_id
        .clone()
        .ok_or_else(|| format!("Unknown mask: {slug}"))?;
    let resource_id = meta
        .resource_id
        .clone()
        .ok_or_else(|| format!("Unknown mask: {slug}"))?;
    let resource_type = meta
        .resource_type
        .clone()
        .ok_or_else(|| format!("Unknown mask: {slug}"))?;

    let seg_exists = draft.tracks.iter().any(|t| t.segments.iter().any(|s| s.id == segment_id));
    if !seg_exists {
        return Err(format!("Segment not found: {segment_id}"));
    }

    let field = mask_target_field(draft, opts.field.as_deref());

    // refuse stacking
    let all = all_mask_entries(draft);
    let already = draft.tracks.iter().any(|t| {
        t.segments.iter().any(|s| {
            if s.id != segment_id {
                return false;
            }
            if let Some(Value::Array(refs)) = s.extra.get("extra_material_refs") {
                for r in refs {
                    if let Some(rid) = r.as_str() {
                        if all.iter().any(|m| m.get("id").and_then(|x| x.as_str()) == Some(rid)) {
                            return true;
                        }
                    }
                }
            }
            false
        })
    });
    if already {
        return Err("Segment already has a mask. Remove it first.".to_string());
    }

    let resolved = aliases.get(slug).map(|s| s.as_str()).unwrap_or(slug).to_ascii_lowercase();
    let is_rect = resolved == "rectangle";
    if (opts.rect_width.is_some() || opts.round_corner.is_some()) && !is_rect {
        return Err("--rect-width / --round-corner only valid for rectangle mask".to_string());
    }

    let aspect_ratio = meta.default_aspect_ratio.unwrap_or(1.0);
    let size = opts.size.unwrap_or(0.5);
    let canvas_w = draft
        .canvas_config
        .get("width")
        .and_then(|v| v.as_f64())
        .unwrap_or(1920.0);
    let canvas_h = draft
        .canvas_config
        .get("height")
        .and_then(|v| v.as_f64())
        .unwrap_or(1080.0);
    let width = if is_rect {
        opts.rect_width.unwrap_or(size)
    } else {
        size * canvas_h * aspect_ratio / canvas_w
    };
    let height = size;

    let id = Uuid::new_v4().to_string();
    let entry = json!({
        "config": {
            "aspectRatio": aspect_ratio,
            "centerX": opts.center_x.unwrap_or(0.0),
            "centerY": opts.center_y.unwrap_or(0.0),
            "feather": opts.feather.unwrap_or(0.0) / 100.0,
            "height": height,
            "invert": opts.invert.unwrap_or(false),
            "rotation": opts.rotation.unwrap_or(0.0),
            "roundCorner": opts.round_corner.unwrap_or(0.0) / 100.0,
            "width": width,
        },
        "category": "video",
        "category_id": "",
        "category_name": "",
        "id": id,
        "name": name,
        "platform": "all",
        "position_info": "",
        "resource_type": resource_type,
        "resource_id": resource_id,
        "type": "mask",
    });

    match field.as_str() {
        "masks" => draft.materials.masks.push(entry),
        "common_mask" => draft.materials.common_mask.push(entry),
        other => {
            let arr = draft
                .materials
                .extra
                .entry(other.to_string())
                .or_insert_with(|| Value::Array(vec![]));
            if let Value::Array(a) = arr {
                a.push(entry);
            }
        }
    }

    for track in &mut draft.tracks {
        for seg in &mut track.segments {
            if seg.id == segment_id {
                let refs = seg
                    .extra
                    .entry("extra_material_refs")
                    .or_insert_with(|| Value::Array(vec![]));
                if let Value::Array(arr) = refs {
                    arr.push(Value::String(id.clone()));
                }
            }
        }
    }

    Ok((id, name, field))
}

/// Remove mask refs from segment (--off). Returns number removed.
pub fn remove_masks(draft: &mut Draft, segment_id: &str) -> Result<usize, String> {
    let seg_exists = draft.tracks.iter().any(|t| t.segments.iter().any(|s| s.id == segment_id));
    if !seg_exists {
        return Err(format!("Segment not found: {segment_id}"));
    }
    let all = all_mask_entries(draft);
    let mut removed = 0;
    for track in &mut draft.tracks {
        for seg in &mut track.segments {
            if seg.id == segment_id {
                if let Some(Value::Array(refs)) = seg.extra.get("extra_material_refs").cloned() {
                    let filtered: Vec<Value> = refs
                        .into_iter()
                        .filter(|r| {
                            if let Some(rid) = r.as_str() {
                                let is_mask = all
                                    .iter()
                                    .any(|m| m.get("id").and_then(|x| x.as_str()) == Some(rid));
                                if is_mask {
                                    removed += 1;
                                    return false;
                                }
                            }
                            true
                        })
                        .collect();
                    seg.extra
                        .insert("extra_material_refs".to_string(), Value::Array(filtered));
                }
            }
        }
    }
    Ok(removed)
}

// ---------------------------------------------------------------------------
// Bg blur
// ---------------------------------------------------------------------------

const BLUR_LEVELS: [f64; 4] = [0.0625, 0.375, 0.75, 1.0];

pub fn set_bg_blur(
    draft: &mut Draft,
    segment_id: &str,
    level: Option<u8>,
) -> Result<(Option<String>, Option<f64>), String> {
    let seg_exists = draft.tracks.iter().any(|t| t.segments.iter().any(|s| s.id == segment_id));
    if !seg_exists {
        return Err(format!("Segment not found: {segment_id}"));
    }
    // remove existing canvas refs
    let canvas_ids: Vec<String> = draft
        .materials
        .canvases
        .iter()
        .filter_map(|c| c.get("id").and_then(|x| x.as_str()).map(|s| s.to_string()))
        .collect();
    for track in &mut draft.tracks {
        for seg in &mut track.segments {
            if seg.id == segment_id {
                if let Some(Value::Array(refs)) = seg.extra.get("extra_material_refs").cloned() {
                    let filtered: Vec<Value> = refs
                        .into_iter()
                        .filter(|r| {
                            if let Some(rid) = r.as_str() {
                                return !canvas_ids.contains(&rid.to_string());
                            }
                            true
                        })
                        .collect();
                    seg.extra
                        .insert("extra_material_refs".to_string(), Value::Array(filtered));
                }
            }
        }
    }
    if level.is_none() {
        return Ok((None, None));
    }
    let lvl = level.unwrap();
    if lvl < 1 || lvl > 4 {
        return Err("bg-blur level must be 1-4 or off".to_string());
    }
    let blur = BLUR_LEVELS[(lvl - 1) as usize];
    let id = Uuid::new_v4().to_string();
    draft.materials.canvases.push(json!({
        "album_image": "",
        "blur": blur,
        "color": "",
        "id": id,
        "image": "",
        "image_id": "",
        "image_name": "",
        "source_platform": 0,
        "team_id": "",
        "type": "canvas_blur",
    }));
    for track in &mut draft.tracks {
        for seg in &mut track.segments {
            if seg.id == segment_id {
                let refs = seg
                    .extra
                    .entry("extra_material_refs")
                    .or_insert_with(|| Value::Array(vec![]));
                if let Value::Array(arr) = refs {
                    arr.push(Value::String(id.clone()));
                }
            }
        }
    }
    Ok((Some(id), Some(blur)))
}

// ---------------------------------------------------------------------------
// Text style
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct TextStyleOptions {
    pub alpha: Option<f64>,
    pub vertical: Option<bool>,
    pub fixed_width: Option<f64>,
    pub fixed_height: Option<f64>,
    pub shadow: Option<bool>,
    pub shadow_alpha: Option<f64>,
    pub shadow_angle: Option<f64>,
    pub shadow_color: Option<String>,
    pub shadow_distance: Option<f64>,
    pub shadow_smoothing: Option<f64>,
    pub border_width: Option<f64>,
    pub border_color: Option<String>,
    pub border_alpha: Option<f64>,
    pub bg_color: Option<String>,
    pub bg_alpha: Option<f64>,
    pub bg_style: Option<i64>,
    pub bg_round_radius: Option<f64>,
    pub bg_width: Option<f64>,
    pub bg_height: Option<f64>,
    pub bg_h_offset: Option<f64>,
    pub bg_v_offset: Option<f64>,
}

pub fn set_text_style(
    draft: &mut Draft,
    segment_id: &str,
    opts: TextStyleOptions,
) -> Result<(String, Vec<String>), String> {
    let mat_pos = {
        let seg = draft
            .tracks
            .iter()
            .find(|t| t.segments.iter().any(|s| s.id == segment_id))
            .ok_or_else(|| format!("Segment not found: {segment_id}"))?;
        let seg = seg.segments.iter().find(|s| s.id == segment_id).unwrap();
        let mid = seg.material_id.clone();
        draft
            .materials
            .texts
            .iter()
            .position(|v| v.get("id").and_then(|x| x.as_str()) == Some(&mid))
            .ok_or_else(|| format!("Text material not found for segment {segment_id}"))?
    };
    let mut applied = Vec::new();
    let mat = draft.materials.texts[mat_pos].as_object_mut().unwrap();
    if let Some(v) = opts.alpha {
        mat.insert("text_alpha".to_string(), json!(v));
        applied.push("alpha".to_string());
    }
    if let Some(v) = opts.vertical {
        mat.insert("typesetting".to_string(), json!(if v { 1 } else { 0 }));
        applied.push("vertical".to_string());
    }
    if let Some(v) = opts.fixed_width {
        mat.insert("fixed_width".to_string(), json!(v));
        applied.push("fixed_width".to_string());
    }
    if let Some(v) = opts.fixed_height {
        mat.insert("fixed_height".to_string(), json!(v));
        applied.push("fixed_height".to_string());
    }
    if let Some(shadow) = opts.shadow {
        if shadow {
            mat.insert("has_shadow".to_string(), json!(true));
            if let Some(v) = opts.shadow_alpha {
                mat.insert("shadow_alpha".to_string(), json!(v));
            }
            if let Some(v) = opts.shadow_angle {
                mat.insert("shadow_angle".to_string(), json!(v));
            }
            if let Some(v) = opts.shadow_color {
                mat.insert("shadow_color".to_string(), json!(v));
            }
            if let Some(v) = opts.shadow_distance {
                mat.insert("shadow_distance".to_string(), json!(v));
            }
            if let Some(v) = opts.shadow_smoothing {
                mat.insert("shadow_smoothing".to_string(), json!(v));
            }
            applied.push("shadow".to_string());
        } else {
            mat.insert("has_shadow".to_string(), json!(false));
            applied.push("shadow-off".to_string());
        }
    }
    if opts.border_width.is_some() || opts.border_color.is_some() || opts.border_alpha.is_some() {
        if let Some(v) = opts.border_width {
            mat.insert("border_width".to_string(), json!(v));
        } else if !mat.contains_key("border_width") {
            mat.insert("border_width".to_string(), json!(0));
        }
        if let Some(v) = opts.border_color {
            mat.insert("border_color".to_string(), json!(v));
        } else if !mat.contains_key("border_color") {
            mat.insert("border_color".to_string(), json!("#000000"));
        }
        if let Some(v) = opts.border_alpha {
            mat.insert("border_alpha".to_string(), json!(v));
        } else if !mat.contains_key("border_alpha") {
            mat.insert("border_alpha".to_string(), json!(1));
        }
        mat.insert("has_border".to_string(), json!(true));
        applied.push("border".to_string());
    }
    if opts.bg_color.is_some()
        || opts.bg_alpha.is_some()
        || opts.bg_style.is_some()
        || opts.bg_round_radius.is_some()
        || opts.bg_width.is_some()
        || opts.bg_height.is_some()
        || opts.bg_h_offset.is_some()
        || opts.bg_v_offset.is_some()
    {
        mat.insert("has_text_shadow_config".to_string(), json!(true));
        if let Some(v) = opts.bg_color {
            mat.insert("background_color".to_string(), json!(v));
        }
        if let Some(v) = opts.bg_alpha {
            mat.insert("background_alpha".to_string(), json!(v));
        }
        if let Some(v) = opts.bg_style {
            mat.insert("background_style".to_string(), json!(v));
        }
        if let Some(v) = opts.bg_round_radius {
            mat.insert("background_round_radius".to_string(), json!(v));
        }
        if let Some(v) = opts.bg_width {
            mat.insert("background_width".to_string(), json!(v));
        }
        if let Some(v) = opts.bg_height {
            mat.insert("background_height".to_string(), json!(v));
        }
        if let Some(v) = opts.bg_h_offset {
            mat.insert("background_horizontal_offset".to_string(), json!(v));
        }
        if let Some(v) = opts.bg_v_offset {
            mat.insert("background_vertical_offset".to_string(), json!(v));
        }
        applied.push("bg".to_string());
    }
    let id = draft.materials.texts[mat_pos]
        .get("id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Ok((id, applied))
}

// ---------------------------------------------------------------------------
// Animations (text / image)
// ---------------------------------------------------------------------------

fn text_anim_aliases() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("blur-text-in".to_string(), "blur".to_string());
    m.insert("zoom-in-text".to_string(), "zoom-in".to_string());
    m
}

pub fn text_anim_slugs() -> Vec<String> {
    vec![
        "fade-in".to_string(),
        "fade-out".to_string(),
        "typewriter".to_string(),
        "pop-up".to_string(),
        "throw-out".to_string(),
        "blur-text-in".to_string(),
        "zoom-in-text".to_string(),
    ]
}

#[derive(Debug, Clone, Default)]
pub struct TextAnimOptions {
    pub intro: Option<String>,
    pub outro: Option<String>,
    pub intro_duration_us: Option<i64>,
    pub outro_duration_us: Option<i64>,
}

fn ensure_anim_container(draft: &mut Draft, seg_id: &str) -> Result<String, String> {
    // find seg extra_material_refs that points to material_animations
    let anims_val = draft
        .materials
        .extra
        .get("material_animations")
        .cloned()
        .unwrap_or(Value::Array(vec![]));
    let anims_arr = anims_val.as_array().cloned().unwrap_or_default();
    let mut existing_id: Option<String> = None;
    for track in &draft.tracks {
        for seg in &track.segments {
            if seg.id == seg_id {
                if let Some(Value::Array(refs)) = seg.extra.get("extra_material_refs") {
                    for r in refs {
                        if let Some(rid) = r.as_str() {
                            for a in &anims_arr {
                                if a.get("id").and_then(|x| x.as_str()) == Some(rid) {
                                    existing_id = Some(rid.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(id) = existing_id {
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    let fresh = json!({
        "animations": [],
        "id": id,
        "multi_language_current": "none",
        "type": "sticker_animation",
    });
    let entry = draft
        .materials
        .extra
        .entry("material_animations".to_string())
        .or_insert_with(|| Value::Array(vec![]));
    if let Value::Array(arr) = entry {
        arr.push(fresh);
    }
    for track in &mut draft.tracks {
        for seg in &mut track.segments {
            if seg.id == seg_id {
                let refs = seg
                    .extra
                    .entry("extra_material_refs")
                    .or_insert_with(|| Value::Array(vec![]));
                if let Value::Array(a) = refs {
                    a.push(Value::String(id.clone()));
                }
            }
        }
    }
    Ok(id)
}

fn anim_duration_seg(draft: &Draft, seg_id: &str) -> i64 {
    for t in &draft.tracks {
        for s in &t.segments {
            if s.id == seg_id {
                return s.target_timerange.duration;
            }
        }
    }
    0
}

pub fn add_text_anim(
    draft: &mut Draft,
    segment_id: &str,
    opts: TextAnimOptions,
    namespace: Namespace,
) -> Result<(String, Vec<Value>), String> {
    if opts.intro.is_none() && opts.outro.is_none() {
        return Err("at least one of --intro or --outro is required".to_string());
    }
    let seg_exists = draft.tracks.iter().any(|t| t.segments.iter().any(|s| s.id == segment_id));
    if !seg_exists {
        return Err(format!("Segment not found: {segment_id}"));
    }
    let container_id = ensure_anim_container(draft, segment_id)?;
    let target_dur = anim_duration_seg(draft, segment_id);
    let aliases = text_anim_aliases();
    let mut added = Vec::new();

    let do_one = |draft: &mut Draft,
                  slug: &str,
                  override_dur: Option<i64>,
                  anim_type: &str|
     -> Result<Value, String> {
        let category = if anim_type == "in" {
            Category::TextIntros
        } else {
            Category::TextOutros
        };
        let meta = find_enum(category, slug, namespace, Some(&aliases)).ok_or_else(|| {
            let hint = if namespace == Namespace::JianYing {
                " --jianying"
            } else {
                ""
            };
            let kind = if anim_type == "in" { "intro" } else { "outro" };
            format!(
                "Unknown text {kind}: {slug}. Run 'capcut enums --text-{}s{hint}' for the full list.",
                kind
            )
        })?;
        let effect_id = meta.effect_id.clone().ok_or_else(|| format!("Unknown text anim: {slug}"))?;
        let resource_id = meta.resource_id.clone().ok_or_else(|| format!("Unknown text anim: {slug}"))?;
        let name = meta
            .title
            .clone()
            .or(meta.name.clone())
            .unwrap_or(slug.to_string());
        // check duplicate
        let anims = draft
            .materials
            .extra
            .get("material_animations")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for a in &anims {
            if a.get("id").and_then(|x| x.as_str()) == Some(&container_id) {
                if let Some(arr) = a.get("animations").and_then(|x| x.as_array()) {
                    if arr.iter().any(|x| x.get("type").and_then(|t| t.as_str()) == Some(anim_type)) {
                        return Err(format!("segment already has a {anim_type} text animation"));
                    }
                }
            }
        }
        let dur = override_dur
            .or(meta.duration)
            .or(meta.default_duration)
            .unwrap_or(500_000);
        if dur > target_dur {
            return Err(format!("duration ({dur}us) exceeds segment duration ({target_dur}us)"));
        }
        let start = if anim_type == "out" { target_dur - dur } else { 0 };
        let cat_id = if anim_type == "in" { "in_fav" } else { "out_fav" };
        Ok(json!({
            "anim_adjust_params": null,
            "category_id": cat_id,
            "category_name": cat_id,
            "duration": dur,
            "id": effect_id,
            "material_type": "text",
            "name": name,
            "panel": "",
            "path": "",
            "platform": "all",
            "request_id": "",
            "resource_id": resource_id,
            "source_platform": 1,
            "start": start,
            "third_resource_id": "",
            "type": anim_type,
        }))
    };

    let mut entries: Vec<Value> = Vec::new();
    if let Some(slug) = opts.intro.clone() {
        entries.push(do_one(draft, &slug, opts.intro_duration_us, "in")?);
    }
    if let Some(slug) = opts.outro.clone() {
        entries.push(do_one(draft, &slug, opts.outro_duration_us, "out")?);
    }
    // push into container
    if let Some(Value::Array(anims_arr)) = draft.materials.extra.get_mut("material_animations") {
        for anim in anims_arr.iter_mut() {
            if anim.get("id").and_then(|x| x.as_str()) == Some(&container_id) {
                if let Some(Value::Array(arr)) = anim.get_mut("animations") {
                    for e in &entries {
                        arr.push(e.clone());
                        added.push(e.clone());
                    }
                }
            }
        }
    }
    Ok((container_id, added))
}

// Image anims: small inline catalogue + fallback to enums
struct ImageAnimMeta {
    name: &'static str,
    effect_id: &'static str,
    resource_id: &'static str,
    md5: &'static str,
    default_duration_us: i64,
    category_id: &'static str,
    third_resource_id: &'static str,
    anim_type: &'static str,
}

const IMAGE_ANIMS: &[(&str, ImageAnimMeta)] = &[
    ("fade-in", ImageAnimMeta { name: "Fade In", effect_id: "6798320778182922760", resource_id: "6798320778182922760", md5: "883ad04bd79b502aaa55b5d9b87175ea", default_duration_us: 500000, category_id: "2037708296", third_resource_id: "6798320778182922760", anim_type: "in" }),
    ("fade-out", ImageAnimMeta { name: "Fade Out", effect_id: "6798320902548230669", resource_id: "6798320902548230669", md5: "c6f05ce62355b537be762550040bfc08", default_duration_us: 500000, category_id: "2037708296", third_resource_id: "0", anim_type: "out" }),
    ("zoom-out", ImageAnimMeta { name: "Zoom Out", effect_id: "6798332584276267527", resource_id: "6798332584276267527", md5: "0c736f993d36a7b1ef00cc73d2ba656f", default_duration_us: 2000000, category_id: "", third_resource_id: "", anim_type: "in" }),
];

pub fn image_anim_slugs() -> Vec<String> {
    IMAGE_ANIMS.iter().map(|(k, _)| k.to_string()).collect()
}

#[derive(Debug, Clone, Default)]
pub struct ImageAnimOptions {
    pub intro: Option<String>,
    pub outro: Option<String>,
    pub combo: Option<String>,
    pub intro_duration_us: Option<i64>,
    pub outro_duration_us: Option<i64>,
    pub combo_duration_us: Option<i64>,
}

pub fn add_image_anim(
    draft: &mut Draft,
    segment_id: &str,
    opts: ImageAnimOptions,
    namespace: Namespace,
) -> Result<(String, Vec<Value>), String> {
    if opts.intro.is_none() && opts.outro.is_none() && opts.combo.is_none() {
        return Err("at least one of --intro, --outro, --combo is required".to_string());
    }
    let seg_exists = draft.tracks.iter().any(|t| t.segments.iter().any(|s| s.id == segment_id));
    if !seg_exists {
        return Err(format!("Segment not found: {segment_id}"));
    }
    let container_id = ensure_anim_container(draft, segment_id)?;
    let target_dur = anim_duration_seg(draft, segment_id);
    let mut entries: Vec<Value> = Vec::new();
    let mut added = Vec::new();

    let do_one = |draft: &mut Draft,
                  slug: &str,
                  override_dur: Option<i64>,
                  anim_type: &str|
     -> Result<Value, String> {
        // inline first
        let mut name: Option<String> = None;
        let mut effect_id: Option<String> = None;
        let mut resource_id: Option<String> = None;
        let mut md5 = String::new();
        let mut default_dur: i64 = 500000;
        let mut category_id = String::new();
        let mut third = "0".to_string();
        if let Some((_, meta)) = IMAGE_ANIMS.iter().find(|(k, _)| *k == slug) {
            name = Some(meta.name.to_string());
            effect_id = Some(meta.effect_id.to_string());
            resource_id = Some(meta.resource_id.to_string());
            md5 = meta.md5.to_string();
            default_dur = meta.default_duration_us;
            category_id = meta.category_id.to_string();
            third = meta.third_resource_id.to_string();
        } else {
            let cat = match anim_type {
                "in" => Category::ImageIntros,
                "out" => Category::ImageOutros,
                _ => Category::ImageCombos,
            };
            let meta = find_enum(cat, slug, namespace, None).ok_or_else(|| {
                let hint = if namespace == Namespace::JianYing { " --jianying" } else { "" };
                format!("Unknown image {anim_type} animation: {slug}. Run 'capcut enums --image-{}s{hint}' for the full list.", anim_type)
            })?;
            name = Some(meta.title.clone().or(meta.name.clone()).unwrap_or(slug.to_string()));
            effect_id = meta.effect_id.clone();
            resource_id = meta.resource_id.clone();
            md5 = meta.md5.clone().unwrap_or_default();
            default_dur = meta.duration.or(meta.default_duration).unwrap_or(500000);
            category_id = if anim_type == "in" { "in_fav".to_string() } else if anim_type == "out" { "out_fav".to_string() } else { String::new() };
            third = "0".to_string();
        }
        let effect_id = effect_id.ok_or_else(|| format!("Unknown image anim: {slug}"))?;
        let resource_id = resource_id.ok_or_else(|| format!("Unknown image anim: {slug}"))?;
        let name = name.unwrap();
        // duplicate check
        let anims = draft
            .materials
            .extra
            .get("material_animations")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for a in &anims {
            if a.get("id").and_then(|x| x.as_str()) == Some(&container_id) {
                if let Some(arr) = a.get("animations").and_then(|x| x.as_array()) {
                    if arr.iter().any(|x| x.get("type").and_then(|t| t.as_str()) == Some(anim_type)) {
                        return Err(format!("segment already has a {anim_type} video animation"));
                    }
                }
            }
        }
        let dur = override_dur.unwrap_or(default_dur);
        if dur > target_dur {
            return Err(format!("duration ({dur}us) exceeds segment duration ({target_dur}us)"));
        }
        let start = if anim_type == "out" { target_dur - dur } else { 0 };
        let cache_base = std::env::var("HOME").unwrap_or_default() + "/Library/Containers/com.lemon.lvoverseas/Data/Movies/CapCut/User Data/Cache/effect";
        let path = if md5.is_empty() { String::new() } else { format!("{cache_base}/{effect_id}/{md5}") };
        Ok(json!({
            "anim_adjust_params": null,
            "category_id": category_id,
            "category_name": category_id,
            "duration": dur,
            "id": effect_id,
            "material_type": "video",
            "name": name,
            "panel": "video",
            "path": path,
            "platform": "all",
            "request_id": "",
            "resource_id": resource_id,
            "source_platform": 1,
            "start": start,
            "third_resource_id": third,
            "type": anim_type,
        }))
    };

    if let Some(s) = opts.intro.clone() {
        entries.push(do_one(draft, &s, opts.intro_duration_us, "in")?);
    }
    if let Some(s) = opts.outro.clone() {
        entries.push(do_one(draft, &s, opts.outro_duration_us, "out")?);
    }
    if let Some(s) = opts.combo.clone() {
        entries.push(do_one(draft, &s, opts.combo_duration_us, "group")?);
    }
    if let Some(Value::Array(anims_arr)) = draft.materials.extra.get_mut("material_animations") {
        for anim in anims_arr.iter_mut() {
            if anim.get("id").and_then(|x| x.as_str()) == Some(&container_id) {
                if let Some(Value::Array(arr)) = anim.get_mut("animations") {
                    for e in &entries {
                        arr.push(e.clone());
                        added.push(e.clone());
                    }
                }
            }
        }
    }
    Ok((container_id, added))
}
