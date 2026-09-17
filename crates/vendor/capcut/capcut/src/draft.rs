#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Data model — mirrors reference/src/draft.ts
// Unknown fields preserved via flatten so CapCut never rejects on re-open.
// ---------------------------------------------------------------------------

/// Mirrors CapCut draft_content.json top-level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub duration: i64,
    #[serde(default = "default_fps")]
    pub fps: f32,
    #[serde(default)]
    pub canvas_config: Value,
    #[serde(default)]
    pub platform: Value,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub materials: Materials,
    #[serde(default)]
    pub extra_info: Value,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

fn default_fps() -> f32 {
    30.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    pub material_id: String,
    pub target_timerange: Timerange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_timerange: Option<Timerange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timerange {
    pub start: i64,
    pub duration: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Materials {
    #[serde(default)]
    pub videos: Vec<Value>,
    #[serde(default)]
    pub audios: Vec<Value>,
    #[serde(default)]
    pub texts: Vec<Value>,
    #[serde(default)]
    pub stickers: Vec<Value>,
    #[serde(default)]
    pub video_effects: Vec<Value>,
    #[serde(default)]
    pub transitions: Vec<Value>,
    #[serde(default)]
    pub masks: Vec<Value>,
    #[serde(default)]
    pub chromas: Vec<Value>,
    #[serde(default)]
    pub audio_fades: Vec<Value>,
    #[serde(default)]
    pub audio_effects: Vec<Value>,
    #[serde(default)]
    pub canvases: Vec<Value>,
    #[serde(default)]
    pub speeds: Vec<Value>,
    #[serde(default)]
    pub sound_channel_mappings: Vec<Value>,
    #[serde(default)]
    pub vocal_separations: Vec<Value>,
    #[serde(default)]
    pub placeholders: Vec<Value>,
    #[serde(default)]
    pub common_mask: Vec<Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

// ---------------------------------------------------------------------------
// Track ordering — parity with reference/src/draft.ts TRACK_RANK + sortTracks
// ---------------------------------------------------------------------------

/// Canonical bottom->top layer order CapCut expects.
/// Derived from real CapCut-authored draft: video 0, audio 1, sticker 2,
/// effect 3, filter 4, text 5. Unknown types sort last (rank 99).
fn track_rank(kind: &str) -> i32 {
    match kind {
        "video" => 0,
        "audio" => 1,
        "sticker" => 2,
        "effect" => 3,
        "filter" => 4,
        "text" => 5,
        _ => 99,
    }
}

/// Sort tracks into canonical layer order. Stable: same-type tracks keep
/// authored order (tiebreak on original index). Mirrors `sortTracks` in TS.
pub fn sort_tracks(draft: &mut Draft) {
    // Indexed stable sort.
    let mut indexed: Vec<(usize, Track)> = draft.tracks.drain(..).enumerate().map(|(i, t)| (i, t)).collect();
    indexed.sort_by(|(ia, ta), (ib, tb)| {
        let ra = track_rank(&ta.kind);
        let rb = track_rank(&tb.kind);
        ra.cmp(&rb).then_with(|| ia.cmp(ib))
    });
    draft.tracks = indexed.into_iter().map(|(_, t)| t).collect();
}

/// Exposed rank helper for callers/tests that need to reason about ordering.
pub fn track_rank_for(kind: &str) -> i32 {
    track_rank(kind)
}

// ---------------------------------------------------------------------------
// make_track — parity with makeTrack(type, name, isDefaultName)
// ---------------------------------------------------------------------------

/// Fresh empty track. Key order not significant in Rust/serde_json but
/// semantics match TS `makeTrack`.
pub fn make_track(kind: &str, name: &str, is_default_name: bool) -> Track {
    let mut extra = serde_json::Map::new();
    extra.insert("attribute".to_string(), Value::Number(0.into()));
    extra.insert("is_default_name".to_string(), Value::Bool(is_default_name));
    extra.insert("flag".to_string(), Value::Number(0.into()));
    Track {
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        name: name.to_string(),
        segments: Vec::new(),
        extra,
    }
}

// ---------------------------------------------------------------------------
// find_segment — parity with findSegment(draft, id)
// Supports exact id or case-insensitive prefix (short id).
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct FindSegmentResult<'a> {
    pub track_index: usize,
    pub segment_index: usize,
    pub track: &'a Track,
    pub segment: &'a Segment,
}

pub fn find_segment<'a>(draft: &'a Draft, id: &str) -> Option<FindSegmentResult<'a>> {
    let short = id.to_ascii_lowercase();
    for (ti, track) in draft.tracks.iter().enumerate() {
        for (si, seg) in track.segments.iter().enumerate() {
            if seg.id == id || seg.id.to_ascii_lowercase().starts_with(&short) {
                return Some(FindSegmentResult {
                    track_index: ti,
                    segment_index: si,
                    track,
                    segment: seg,
                });
            }
        }
    }
    None
}

// Mutable variant for in-place edits.
pub struct FindSegmentMut<'a> {
    pub track_index: usize,
    pub segment_index: usize,
    pub track: &'a mut Track,
}

pub fn find_segment_mut<'a>(draft: &'a mut Draft, id: &str) -> Option<FindSegmentMut<'a>> {
    let short = id.to_ascii_lowercase();
    for ti in 0..draft.tracks.len() {
        // Check without holding borrow across iteration.
        let found_si = {
            let track = &draft.tracks[ti];
            track
                .segments
                .iter()
                .position(|seg| seg.id == id || seg.id.to_ascii_lowercase().starts_with(&short))
        };
        if let Some(si) = found_si {
            let track = &mut draft.tracks[ti];
            return Some(FindSegmentMut {
                track_index: ti,
                segment_index: si,
                track,
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Material helpers — parity with findMaterial / getTracksByType /
// getMaterialTypes / findMaterialGlobal
// ---------------------------------------------------------------------------

fn material_id(v: &Value) -> Option<&str> {
    v.get("id")?.as_str()
}

/// Find material by id in a slice. Exact match on `id` field of Value.
pub fn find_material<'a>(arr: &'a [Value], id: &str) -> Option<&'a Value> {
    arr.iter().find(|v| material_id(v) == Some(id))
}

pub fn find_material_mut<'a>(arr: &'a mut [Value], id: &str) -> Option<&'a mut Value> {
    arr.iter_mut().find(|v| material_id(v) == Some(id))
}

pub fn get_tracks_by_type<'a>(draft: &'a Draft, kind: &str) -> Vec<&'a Track> {
    draft.tracks.iter().filter(|t| t.kind == kind).collect()
}

pub fn get_tracks_by_type_mut<'a>(draft: &'a mut Draft, kind: &str) -> Vec<*mut Track> {
    // Returns raw pointers to avoid double-borrow issues; caller must be careful.
    // Preferred: iterate mutably directly.
    draft
        .tracks
        .iter_mut()
        .filter(|t| t.kind == kind)
        .map(|t| t as *mut Track)
        .collect()
}

#[derive(Debug, Clone)]
pub struct MaterialTypeCount {
    pub kind: String,
    pub count: usize,
}

/// Counts per known + extra material arrays, sorted descending by count.
/// Mirrors `getMaterialTypes` in TS.
pub fn get_material_types(draft: &Draft) -> Vec<MaterialTypeCount> {
    let mut out: Vec<MaterialTypeCount> = Vec::new();
    let push = |kind: &str, arr: &[Value], out: &mut Vec<MaterialTypeCount>| {
        out.push(MaterialTypeCount {
            kind: kind.to_string(),
            count: arr.len(),
        });
    };
    push("videos", &draft.materials.videos, &mut out);
    push("audios", &draft.materials.audios, &mut out);
    push("texts", &draft.materials.texts, &mut out);
    push("stickers", &draft.materials.stickers, &mut out);
    push("video_effects", &draft.materials.video_effects, &mut out);
    push("transitions", &draft.materials.transitions, &mut out);
    push("masks", &draft.materials.masks, &mut out);
    push("chromas", &draft.materials.chromas, &mut out);
    push("audio_fades", &draft.materials.audio_fades, &mut out);
    push("audio_effects", &draft.materials.audio_effects, &mut out);
    push("canvases", &draft.materials.canvases, &mut out);
    push("speeds", &draft.materials.speeds, &mut out);
    push(
        "sound_channel_mappings",
        &draft.materials.sound_channel_mappings,
        &mut out,
    );
    push(
        "vocal_separations",
        &draft.materials.vocal_separations,
        &mut out,
    );
    push("placeholders", &draft.materials.placeholders, &mut out);
    push("common_mask", &draft.materials.common_mask, &mut out);
    for (k, v) in &draft.materials.extra {
        if let Value::Array(arr) = v {
            out.push(MaterialTypeCount {
                kind: k.clone(),
                count: arr.len(),
            });
        }
    }
    out.sort_by(|a, b| b.count.cmp(&a.count));
    out
}

#[derive(Debug)]
pub struct GlobalMaterial<'a> {
    pub kind: String,
    pub material: &'a Value,
}

/// Global search across all material arrays by id (exact or prefix, case-insensitive).
/// Mirrors `findMaterialGlobal` in TS.
pub fn find_material_global<'a>(draft: &'a Draft, id: &str) -> Option<GlobalMaterial<'a>> {
    let short = id.to_ascii_lowercase();
    let matches = |mat_id: &str| mat_id == id || mat_id.to_ascii_lowercase().starts_with(&short);

    macro_rules! search {
        ($field:ident, $name:expr) => {
            for m in &draft.materials.$field {
                if let Some(mid) = material_id(m) {
                    if matches(mid) {
                        return Some(GlobalMaterial {
                            kind: $name.to_string(),
                            material: m,
                        });
                    }
                }
            }
        };
    }
    search!(videos, "videos");
    search!(audios, "audios");
    search!(texts, "texts");
    search!(stickers, "stickers");
    search!(video_effects, "video_effects");
    search!(transitions, "transitions");
    search!(masks, "masks");
    search!(chromas, "chromas");
    search!(audio_fades, "audio_fades");
    search!(audio_effects, "audio_effects");
    search!(canvases, "canvases");
    search!(speeds, "speeds");
    search!(sound_channel_mappings, "sound_channel_mappings");
    search!(vocal_separations, "vocal_separations");
    search!(placeholders, "placeholders");
    search!(common_mask, "common_mask");

    for (k, v) in &draft.materials.extra {
        if let Value::Array(arr) = v {
            for m in arr {
                if let Some(mid) = material_id(m) {
                    if matches(mid) {
                        return Some(GlobalMaterial {
                            kind: k.clone(),
                            material: m,
                        });
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Text content helpers — parity with extractText / extractStyleRanges /
// extractCodeUnitStyleRanges / updateTextContent
// ---------------------------------------------------------------------------

/// Extract plain text from a text material's `content` JSON string.
/// Mirrors `extractText` in TS.
pub fn extract_text(content: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<Value>(content) {
        if let Some(t) = parsed.get("text").and_then(|v| v.as_str()) {
            return t.to_string();
        }
    }
    // Fallback: strip bracket/angle markup like TS does.
    // TS: content.replace(/<[^>]*>/g,"").replace(/\[|\]/g,"").trim()
    let mut s = String::with_capacity(content.len());
    let mut in_tag = false;
    for ch in content.chars() {
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if ch == '>' && in_tag {
            in_tag = false;
            continue;
        }
        if in_tag {
            continue;
        }
        if ch == '[' || ch == ']' {
            continue;
        }
        s.push(ch);
    }
    s.trim().to_string()
}

/// Raw stored style ranges exactly as in `content.styles[].range`.
/// Mirrors `extractStyleRanges`.
pub fn extract_style_ranges(content: &str) -> Vec<[i64; 2]> {
    let Ok(parsed) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };
    let Some(styles) = parsed.get("styles").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for style in styles {
        if let Some(range) = style.get("range").and_then(|v| v.as_array()) {
            if range.len() == 2 {
                if let (Some(a), Some(b)) = (range[0].as_i64(), range[1].as_i64()) {
                    out.push([a, b]);
                } else if let (Some(a), Some(b)) = (range[0].as_f64(), range[1].as_f64()) {
                    out.push([a as i64, b as i64]);
                }
            }
        }
    }
    out
}

/// Style ranges as code-unit offsets, repairing doubled form if needed.
/// Mirrors `extractCodeUnitStyleRanges`.
pub fn extract_code_unit_style_ranges(content: &str) -> Vec<[i64; 2]> {
    let ranges = extract_style_ranges(content);
    let text = extract_text(content);
    if let Some(repaired) = crate::text_offsets::repair_doubled_ranges(&text, &ranges) {
        return repaired;
    }
    ranges
}

/// Update text material `content` JSON to hold `new_text`, preserving style
/// structure. When `styles` has entries, first range is expanded to full span.
/// Falls back to plain replacement if content is not JSON. Mirrors
/// `updateTextContent` in TS.
pub fn update_text_content(content: &str, new_text: &str) -> String {
    if let Ok(mut parsed) = serde_json::from_str::<Value>(content) {
        if parsed.get("text").is_some() {
            if let Some(obj) = parsed.as_object_mut() {
                obj.insert("text".to_string(), Value::String(new_text.to_string()));
                // Preserve style ranges: first style's range -> [0, len]
                if let Some(styles) = obj.get_mut("styles").and_then(|v| v.as_array_mut()) {
                    if !styles.is_empty() {
                        let n = crate::text_offsets::stored_text_length(new_text) as i64;
                        if let Some(first) = styles.get_mut(0) {
                            if let Some(fobj) = first.as_object_mut() {
                                fobj.insert("range".to_string(), Value::Array(vec![Value::Number(0.into()), Value::Number(n.into())]));
                            }
                        }
                    }
                }
            }
            return serde_json::to_string(&parsed).unwrap_or_else(|_| new_text.to_string());
        }
    }
    // Non-JSON or no text field: if content contains brackets, replace inside.
    if content.contains('[') && content.contains(']') {
        // Replace first [...] with [new_text] like TS does with /\[[^\]]*\]/
        if let Some(start) = content.find('[') {
            if let Some(end_rel) = content[start..].find(']') {
                let end = start + end_rel;
                let mut out = String::with_capacity(content.len());
                out.push_str(&content[..start + 1]);
                out.push_str(new_text);
                out.push_str(&content[end..]);
                return out;
            }
        }
    }
    new_text.to_string()
}

// ---------------------------------------------------------------------------
// Atomic write helpers — parity with writeTemp / writeAtomic in draft.ts
// ---------------------------------------------------------------------------

/// Create a temp file next to `target` with `content`, fsync, and return its path.
/// Uses exclusive create (no symlink following) + random suffix.
pub fn write_temp(target: &std::path::Path, content: &str) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;
    for _ in 0..8 {
        let rand: String = uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string();
        let tmp = std::path::PathBuf::from(format!(
            "{}.capcut-cli-{}-{}.tmp",
            target.display(),
            std::process::id(),
            rand
        ));
        // O_EXCL via create_new
        let open_res = {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true);
            opts.create_new(true);
            #[cfg(unix)]
            opts.mode(0o600);
            opts.open(&tmp)
        };
        match open_res {
            Ok(mut f) => {
                f.write_all(content.as_bytes())?;
                f.sync_all()?;
                drop(f);
                return Ok(tmp);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!("Could not create temp file next to {} after 8 attempts", target.display()),
    ))
}

/// Atomic write: temp + rename. Mirrors `writeAtomic` in TS.
pub fn write_atomic(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    let tmp = write_temp(path, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// Extra flatten preservation is structural via serde flatten on Draft/Materials/Track/Segment extra maps.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_draft() -> Draft {
        Draft {
            id: "draft-id".into(),
            name: "demo".into(),
            duration: 1000,
            fps: 30.0,
            canvas_config: json!({}),
            platform: json!({}),
            tracks: vec![
                Track { id: "t1".into(), kind: "text".into(), name: "".into(), segments: vec![], extra: Default::default() },
                Track { id: "t2".into(), kind: "video".into(), name: "".into(), segments: vec![], extra: Default::default() },
                Track { id: "t3".into(), kind: "audio".into(), name: "".into(), segments: vec![], extra: Default::default() },
            ],
            materials: Materials::default(),
            extra_info: json!({}),
            extra: Default::default(),
        }
    }

    #[test]
    fn sort_tracks_canonical() {
        let mut d = sample_draft();
        sort_tracks(&mut d);
        assert_eq!(d.tracks[0].kind, "video");
        assert_eq!(d.tracks[1].kind, "audio");
        assert_eq!(d.tracks[2].kind, "text");
    }

    #[test]
    fn sort_tracks_stable() {
        let mut d = Draft {
            id: "".into(), name: "".into(), duration: 0, fps: 30.0,
            canvas_config: json!({}), platform: json!({}), extra_info: json!({}), extra: Default::default(),
            tracks: vec![
                Track { id: "a".into(), kind: "video".into(), name: "v1".into(), segments: vec![], extra: Default::default() },
                Track { id: "b".into(), kind: "video".into(), name: "v2".into(), segments: vec![], extra: Default::default() },
                Track { id: "c".into(), kind: "audio".into(), name: "a1".into(), segments: vec![], extra: Default::default() },
            ],
            materials: Materials::default(),
        };
        sort_tracks(&mut d);
        assert_eq!(d.tracks[0].id, "a");
        assert_eq!(d.tracks[1].id, "b");
        assert_eq!(d.tracks[2].id, "c");
    }

    #[test]
    fn extract_and_update_text() {
        let content = json!({"text":"hello","styles":[{"range":[0,5],"size":20}]}).to_string();
        assert_eq!(extract_text(&content), "hello");
        assert_eq!(extract_style_ranges(&content), vec![[0,5]]);
        let updated = update_text_content(&content, "hi there");
        let v: Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(v["text"], "hi there");
        assert_eq!(v["styles"][0]["range"], json!([0, 8]));
    }

    #[test]
    fn find_segment_prefix() {
        let mut d = sample_draft();
        d.tracks[0].segments.push(Segment {
            id: "abc-123-xyz".into(), material_id: "m1".into(),
            target_timerange: Timerange{ start: 0, duration: 100 },
            source_timerange: None, speed: None, volume: None, extra: Default::default()
        });
        assert!(find_segment(&d, "abc-123").is_some());
        assert!(find_segment(&d, "notfound").is_none());
    }
}
