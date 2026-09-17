use capcut_core::error::{CoreError, Result as CoreResult};
use capcut_core::timeline::{Canvas, InternalTimeline, TrackKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const US: i64 = 1_000_000;

// ---------------------------------------------------------------------------
// Spec model — mirrors reference/src/compile.ts CompileSpec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileItem {
    pub start: f64,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub volume: Option<f32>,
    #[serde(rename = "fontSize", default)]
    pub font_size: Option<u32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(rename = "type", default)]
    pub item_type: Option<String>,
    #[serde(rename = "sourceStart", default)]
    pub source_start: Option<f64>,
    #[serde(default)]
    pub speed: Option<f64>,
    #[serde(default)]
    pub opacity: Option<f64>,
    #[serde(default)]
    pub rotation: Option<f64>,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(rename = "ref", default)]
    pub ref_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileTrack {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: Option<String>,
    pub items: Vec<CompileItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileSpec {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub fps: Option<f32>,
    #[serde(default)]
    pub ratio: Option<String>,
    pub tracks: Vec<CompileTrack>,
    #[serde(default)]
    pub operations: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileOptions {
    pub out_dir: Option<String>,
    pub spec_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResult {
    pub name: String,
    pub tracks: usize,
    pub segments: usize,
    pub duration_us: i64,
    pub warnings: Vec<String>,
    pub refs: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilePlan {
    pub name: String,
    pub tracks: usize,
    pub items: usize,
    pub operations: usize,
    pub refs: Vec<String>,
}

// ---------------------------------------------------------------------------
// Validation — mirrors validateSpec / validateDraftName in compile.ts
// ---------------------------------------------------------------------------

fn validate_name(name: &Option<String>) -> CoreResult<()> {
    let Some(n) = name else { return Ok(()); };
    if n.trim().is_empty() || n == "." || n == ".." {
        return Err(CoreError::InvalidArgument(format!(
            "spec.name must be a non-empty folder name (got \"{n}\")"
        )));
    }
    if n.contains('/') || n.contains('\\') || (n.len() >= 2 && n.as_bytes()[1] == b':' && n.as_bytes()[0].is_ascii_alphabetic()) {
        return Err(CoreError::InvalidArgument(format!(
            "spec.name takes a plain folder name, not a path (got \"{n}\")"
        )));
    }
    Ok(())
}

pub fn validate_spec(spec: &CompileSpec) -> CoreResult<()> {
    validate_name(&spec.name)?;
    if spec.tracks.is_empty() {
        return Err(CoreError::InvalidArgument(
            "spec.tracks must be a non-empty array".to_string(),
        ));
    }
    let valid_types = ["video", "audio", "text"];
    for (ti, track) in spec.tracks.iter().enumerate() {
        if !valid_types.contains(&track.kind.as_str()) {
            return Err(CoreError::InvalidArgument(format!(
                "tracks[{ti}].type must be one of video|audio|text (got {})",
                track.kind
            )));
        }
        if track.items.is_empty() {
            return Err(CoreError::InvalidArgument(format!(
                "tracks[{ti}].items must be a non-empty array"
            )));
        }
        for (ii, item) in track.items.iter().enumerate() {
            let wh = format!("tracks[{ti}].items[{ii}]");
            if item.start < 0.0 {
                return Err(CoreError::InvalidArgument(format!("{wh}.start must be >= 0")));
            }
            if track.kind == "text" {
                if item.text.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
                    return Err(CoreError::InvalidArgument(format!(
                        "{wh}.text is required for text tracks"
                    )));
                }
                match item.duration {
                    Some(d) if d > 0.0 => {}
                    _ => {
                        return Err(CoreError::InvalidArgument(format!(
                            "{wh}.duration (seconds) is required for text tracks"
                        )))
                    }
                }
            } else {
                if item.path.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
                    return Err(CoreError::InvalidArgument(format!(
                        "{wh}.path is required for {} tracks",
                        track.kind
                    )));
                }
                if track.kind == "video" && item.item_type.as_deref() == Some("photo") {
                    match item.duration {
                        Some(d) if d > 0.0 => {}
                        _ => {
                            return Err(CoreError::InvalidArgument(format!(
                                "{wh}.duration (seconds) is required for photos"
                            )))
                        }
                    }
                }
            }
        }
    }
    // duplicate refs
    let mut seen = std::collections::HashSet::new();
    for track in &spec.tracks {
        for item in &track.items {
            if let Some(r) = &item.ref_name {
                if !seen.insert(r.clone()) {
                    return Err(CoreError::InvalidArgument(format!("duplicate ref '{r}'")));
                }
            }
        }
    }
    // operations validation
    if let Some(ops) = &spec.operations {
        let allowed = [
            "transition",
            "filter",
            "effect",
            "keyframe",
            "audio-fade",
            "text-style",
            "text-ranges",
            "template",
            "captions",
        ];
        for (idx, op) in ops.iter().enumerate() {
            let obj = op.as_object().ok_or_else(|| {
                CoreError::InvalidArgument(format!("operations[{idx}] must be an object"))
            })?;
            let op_name = obj
                .get("op")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    CoreError::InvalidArgument(format!("operations[{idx}].op is required"))
                })?;
            if !allowed.contains(&op_name) {
                return Err(CoreError::InvalidArgument(format!(
                    "operations[{idx}].op is not supported: {op_name}"
                )));
            }
            if ["transition", "keyframe", "audio-fade", "text-style", "text-ranges"]
                .contains(&op_name)
            {
                let target = obj.get("target").and_then(|v| v.as_str()).unwrap_or("");
                if !seen.contains(target) {
                    return Err(CoreError::InvalidArgument(format!(
                        "operations[{idx}].target must reference a declared item ref"
                    )));
                }
            }
        }
    }
    Ok(())
}

pub fn parse_spec(raw: &str) -> CoreResult<CompileSpec> {
    let spec: CompileSpec = serde_json::from_str(raw)
        .map_err(|e| CoreError::InvalidArgument(format!("spec is not valid JSON: {e}")))?;
    validate_spec(&spec)?;
    Ok(spec)
}

// ---------------------------------------------------------------------------
// Templating — mirrors substitutePlaceholders in compile.ts
// ---------------------------------------------------------------------------

pub fn substitute_placeholders(spec: &mut Value, row: &serde_json::Map<String, Value>) -> CoreResult<()> {
    substitute_value(spec, row)
}

fn substitute_value(v: &mut Value, row: &serde_json::Map<String, Value>) -> CoreResult<()> {
    match v {
        Value::String(s) => {
            // find {{key}} placeholders
            let mut result = String::new();
            let mut rest = s.as_str();
            loop {
                if let Some(start) = rest.find("{{") {
                    if let Some(end) = rest[start..].find("}}") {
                        let key = rest[start + 2..start + end].trim().to_string();
                        if !row.contains_key(&key) {
                            return Err(CoreError::InvalidArgument(format!(
                                "no value for placeholder {{{{{key}}}}} in row"
                            )));
                        }
                        let rv = &row[&key];
                        let replacement = match rv {
                            Value::String(x) => x.clone(),
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => {
                                return Err(CoreError::InvalidArgument(format!(
                                    "row value for {{{{{key}}}}} must be string/number/boolean"
                                )))
                            }
                        };
                        result.push_str(&rest[..start]);
                        result.push_str(&replacement);
                        rest = &rest[start + end + 2..];
                        continue;
                    }
                }
                result.push_str(rest);
                break;
            }
            *s = result;
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                substitute_value(item, row)?;
            }
        }
        Value::Object(map) => {
            for (_, val) in map.iter_mut() {
                substitute_value(val, row)?;
            }
        }
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// plan + compile_timeline
// ---------------------------------------------------------------------------

pub fn plan_compile(spec: &CompileSpec) -> CompilePlan {
    let refs: Vec<String> = spec
        .tracks
        .iter()
        .flat_map(|t| t.items.iter().filter_map(|i| i.ref_name.clone()))
        .collect();
    let items: usize = spec.tracks.iter().map(|t| t.items.len()).sum();
    CompilePlan {
        name: spec.name.clone().unwrap_or_else(|| "compiled-draft".to_string()),
        tracks: spec.tracks.len(),
        items,
        operations: spec.operations.as_ref().map(|v| v.len()).unwrap_or(0),
        refs,
    }
}

/// Build an [`InternalTimeline`] from a validated [`CompileSpec`].
/// Mirrors `compileDraft` in compile.ts but operates on the internal timeline
/// (no filesystem / ffprobe dependency).
pub fn compile_timeline(spec: &CompileSpec) -> CoreResult<(InternalTimeline, CompileResult)> {
    let mut tl = InternalTimeline {
        id: uuid::Uuid::new_v4().to_string(),
        name: spec.name.clone().unwrap_or_else(|| "compiled-draft".to_string()),
        duration: 0,
        fps: spec.fps.unwrap_or(30.0),
        canvas: if let (Some(w), Some(h)) = (spec.width, spec.height) {
            Canvas { width: w, height: h }
        } else {
            Canvas { width: 1920, height: 1080 }
        },
        tracks: Vec::new(),
        materials: Default::default(),
    };
    let mut segments = 0usize;
    let mut max_end: i64 = 0;
    let mut refs: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for track in &spec.tracks {
        let kind = match track.kind.as_str() {
            "video" => TrackKind::Video,
            "audio" => TrackKind::Audio,
            "text" => TrackKind::Text,
            _ => unreachable!(),
        };
        for item in &track.items {
            let start = (item.start * US as f64).round() as i64;
            let duration = item
                .duration
                .map(|d| (d * US as f64).round() as i64)
                .unwrap_or(2 * US);
            if duration <= 0 {
                return Err(CoreError::InvalidArgument(format!(
                    "duration must be >0 for ref {:?}",
                    item.ref_name
                )));
            }
            let seg_id = match track.kind.as_str() {
                "video" => {
                    let w = item.width.unwrap_or(1920);
                    let h = item.height.unwrap_or(1080);
                    let path = item.path.clone().unwrap_or_default();
                    let (sid, _) = tl.add_video(path, start, duration, w, h);
                    apply_item_props(&mut tl, &sid, item)?;
                    sid
                }
                "audio" => {
                    let path = item.path.clone().unwrap_or_default();
                    let (sid, _) = tl.add_audio(path, start, duration);
                    if let Some(vol) = item.volume {
                        tl.set_volume(&sid, vol)?;
                    }
                    apply_item_props(&mut tl, &sid, item)?;
                    sid
                }
                "text" => {
                    let text = item.text.clone().unwrap_or_default();
                    let mut style = capcut_core::timeline::TextStyle::default();
                    if let Some(c) = &item.color {
                        style.color = Some(c.clone());
                    }
                    if let Some(s) = item.font_size {
                        style.size = Some(s);
                    }
                    let (sid, _) = tl.add_text(text, start, duration, style);
                    apply_item_props(&mut tl, &sid, item)?;
                    sid
                }
                _ => unreachable!(),
            };
            let _ = kind;
            if let Some(r) = &item.ref_name {
                refs.insert(r.clone(), seg_id);
            }
            max_end = max_end.max(start + duration);
            segments += 1;
        }
    }

    // Operations that map to timeline ops
    if let Some(ops) = &spec.operations {
        for op in ops {
            let obj = op.as_object().unwrap();
            let op_name = obj.get("op").and_then(|v| v.as_str()).unwrap_or("");
            match op_name {
                "audio-fade" | "keyframe" | "transition" | "filter" | "effect" | "template"
                | "captions" => {
                    // Not applicable to InternalTimeline without draft filesystem — warn
                    // but don't fail. Track count already validated.
                }
                "text-style" => {
                    if let Some(target) = obj.get("target").and_then(|v| v.as_str()) {
                        if let Some(seg_id) = refs.get(target) {
                            // resolve material id for text segment
                            let mat_id = tl
                                .tracks
                                .iter()
                                .flat_map(|t| &t.segments)
                                .find(|s| &s.id == seg_id)
                                .map(|s| s.material_id.clone());
                            if let Some(mid) = mat_id {
                                if let Some(style_val) = obj.get("style") {
                                    let mut style = capcut_core::timeline::TextStyle::default();
                                    if let Some(c) = style_val.get("color").and_then(|v| v.as_str()) {
                                        style.color = Some(c.to_string());
                                    }
                                    if let Some(s) = style_val.get("font_size").and_then(|v| v.as_u64()) {
                                        style.size = Some(s as u32);
                                    }
                                    let _ = tl.set_text_style(&mid, style);
                                }
                            }
                        }
                    }
                }
                "text-ranges" => {
                    if let Some(target) = obj.get("target").and_then(|v| v.as_str()) {
                        if let Some(seg_id) = refs.get(target) {
                            let mat_id = tl
                                .tracks
                                .iter()
                                .flat_map(|t| &t.segments)
                                .find(|s| &s.id == seg_id)
                                .map(|s| s.material_id.clone());
                            if let Some(mid) = mat_id {
                                if let Some(ranges_val) = obj.get("ranges").and_then(|v| v.as_array()) {
                                    let current = tl
                                        .materials
                                        .texts
                                        .iter()
                                        .find(|m| m.id == mid)
                                        .map(|m| m.content.clone())
                                        .unwrap_or_default();
                                    let mut ranges = Vec::new();
                                    for r in ranges_val {
                                        if let Some(arr) = r.as_array() {
                                            if arr.len() == 2 {
                                                if let (Some(s), Some(e)) = (arr[0].as_i64(), arr[1].as_i64()) {
                                                    ranges.push(capcut_core::timeline::StyleRange { start: s, end: e });
                                                }
                                            }
                                        }
                                    }
                                    let _ = tl.set_text_with_ranges(&mid, current, ranges);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    tl.duration = max_end;
    let result = CompileResult {
        name: tl.name.clone(),
        tracks: spec.tracks.len(),
        segments,
        duration_us: max_end,
        warnings: Vec::new(),
        refs,
    };
    Ok((tl, result))
}

fn apply_item_props(
    tl: &mut InternalTimeline,
    seg_id: &str,
    item: &CompileItem,
) -> CoreResult<()> {
    if let Some(speed) = item.speed {
        tl.set_speed(seg_id, speed)?;
    }
    if let Some(vol) = item.volume {
        // audio already handled; video/text volume is segment volume
        let _ = tl.set_volume(seg_id, vol);
    }
    if let Some(op) = item.opacity {
        let _ = tl.set_opacity(seg_id, op as f32);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_spec() -> CompileSpec {
        CompileSpec {
            name: Some("test".to_string()),
            width: Some(1080),
            height: Some(1920),
            fps: Some(30.0),
            ratio: Some("9:16".to_string()),
            tracks: vec![CompileTrack {
                kind: "text".to_string(),
                name: None,
                items: vec![CompileItem {
                    start: 0.0,
                    duration: Some(2.0),
                    path: None,
                    text: Some("hello".to_string()),
                    volume: None,
                    font_size: Some(18),
                    color: Some("#FFD700".to_string()),
                    x: None,
                    y: None,
                    width: None,
                    height: None,
                    item_type: None,
                    source_start: None,
                    speed: None,
                    opacity: None,
                    rotation: None,
                    scale: None,
                    ref_name: Some("r1".to_string()),
                }],
            }],
            operations: None,
        }
    }

    #[test]
    fn validate_and_compile() {
        let spec = minimal_spec();
        validate_spec(&spec).unwrap();
        let (tl, res) = compile_timeline(&spec).unwrap();
        assert_eq!(res.tracks, 1);
        assert_eq!(res.segments, 1);
        assert_eq!(tl.duration, 2 * US);
        assert!(res.refs.contains_key("r1"));
    }

    #[test]
    fn parse_spec_roundtrip() {
        let raw = serde_json::to_string(&minimal_spec()).unwrap();
        let parsed = parse_spec(&raw).unwrap();
        assert_eq!(parsed.name.as_deref(), Some("test"));
    }

    #[test]
    fn reject_bad_name() {
        let mut spec = minimal_spec();
        spec.name = Some("../evil".to_string());
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn plan_compile_counts() {
        let spec = minimal_spec();
        let plan = plan_compile(&spec);
        assert_eq!(plan.items, 1);
        assert_eq!(plan.refs, vec!["r1"]);
    }
}
