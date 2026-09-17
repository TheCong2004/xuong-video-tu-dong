use crate::draft::{Draft, Materials, Segment as DraftSegment, Timerange, Track as DraftTrack};
use capcut_core::timeline::*;
use serde_json::{json, Value};

/// Convert InternalTimeline back into a Draft by mutating a base Draft
/// (cloned from template). Fields not touched are preserved byte-identical
/// via base.extra / base.materials.* extra handling.
pub fn into_draft(tl: &InternalTimeline, mut base: Draft) -> Draft {
    base.id = tl.id.clone();
    base.name = tl.name.clone();
    base.duration = tl.duration;
    base.fps = tl.fps;
    if tl.canvas.width != 0 && tl.canvas.height != 0 {
        let ratio = if tl.canvas.width > tl.canvas.height { "16:9" } else if tl.canvas.height > tl.canvas.width * 4 / 3 { "9:16" } else if tl.canvas.width == tl.canvas.height { "1:1" } else { "4:5" };
        if let Value::Object(m) = &mut base.canvas_config {
            m.insert("width".to_string(), json!(tl.canvas.width));
            m.insert("height".to_string(), json!(tl.canvas.height));
            m.insert("ratio".to_string(), json!(ratio));
        } else {
            base.canvas_config = json!({"width": tl.canvas.width, "height": tl.canvas.height, "ratio": ratio});
        }
    }
    // Rebuild tracks from InternalTimeline (field-for-field mapping).
    let mut draft_tracks: Vec<DraftTrack> = Vec::new();
    for track in &tl.tracks {
        let base_track = base.tracks.iter().find(|t| t.kind == track.kind.as_str());
        let id = track.id.clone();
        let extra = base_track.map(|t| t.extra.clone()).unwrap_or_default();
        let mut segs = Vec::new();
        for seg in &track.segments {
            let mut extra_map = match &seg.extra {
                Value::Object(m) => m.clone(),
                _ => serde_json::Map::new(),
            };
            extra_map.remove("id");
            extra_map.remove("material_id");
            extra_map.remove("target_timerange");
            extra_map.remove("source_timerange");
            // opacity is stored in clip.alpha inside extra; keep it in sync
            if let Some(op) = seg.opacity {
                let clip = extra_map.entry("clip").or_insert(json!({}));
                if let Value::Object(c) = clip { c.insert("alpha".to_string(), json!(op)); }
            }
            extra_map.remove("speed");
            extra_map.remove("volume");
            segs.push(DraftSegment {
                id: seg.id.clone(),
                material_id: seg.material_id.clone(),
                target_timerange: Timerange { start: seg.timerange.start, duration: seg.timerange.duration },
                source_timerange: seg.source_timerange.as_ref().map(|r| Timerange { start: r.start, duration: r.duration }),
                speed: seg.speed,
                volume: seg.volume,
                extra: extra_map,
            });
        }
        draft_tracks.push(DraftTrack { id, kind: track.kind.as_str().to_string(), name: track.name.clone(), segments: segs, extra });
    }
    base.tracks = draft_tracks;

    let videos: Vec<Value> = tl.materials.videos.iter().map(|m| {
        let mut v = match &m.extra {
            Value::Object(map) if !map.is_empty() => Value::Object(map.clone()),
            _ => json!({}),
        };
        if let Value::Object(map) = &mut v {
            map.insert("id".to_string(), json!(m.id));
            map.insert("path".to_string(), json!(m.path));
            map.insert("duration".to_string(), json!(m.duration));
            map.insert("width".to_string(), json!(m.width));
            map.insert("height".to_string(), json!(m.height));
            if let Some(crop) = &m.crop {
                map.insert("crop".to_string(), crop.to_corners());
                // keep crop_ratio consistent
                if map.contains_key("crop_ratio") { map.insert("crop_ratio".to_string(), json!("free")); }
            } else {
                map.remove("crop");
            }
        }
        v
    }).collect();
    let audios: Vec<Value> = tl.materials.audios.iter().map(|m| {
        let mut v = match &m.extra {
            Value::Object(map) if !map.is_empty() => Value::Object(map.clone()),
            _ => json!({}),
        };
        if let Value::Object(map) = &mut v {
            map.insert("id".to_string(), json!(m.id));
            map.insert("path".to_string(), json!(m.path));
            map.insert("duration".to_string(), json!(m.duration));
        }
        v
    }).collect();
    let texts: Vec<Value> = tl.materials.texts.iter().map(|m| {
        let mut v = match &m.extra {
            Value::Object(map) if !map.is_empty() => Value::Object(map.clone()),
            _ => json!({}),
        };
        if let Value::Object(map) = &mut v {
            map.insert("id".to_string(), json!(m.id));
            // content may be rich JSON; if style_ranges present, patch it
            let content_val = if m.style_ranges.is_empty() {
                json!(m.content)
            } else {
                // try to parse existing content as rich JSON, then patch styles ranges
                match serde_json::from_str::<Value>(&m.content) {
                    Ok(Value::Object(mut obj)) => {
                        let arr: Vec<Value> = m.style_ranges.iter().map(|r| json!([r.start, r.end])).collect();
                        // if styles exist, update first range; otherwise keep as-is
                        if let Some(Value::Array(styles)) = obj.get_mut("styles") {
                            if let Some(first) = styles.first_mut() {
                                if let Value::Object(s) = first { s.insert("range".to_string(), arr.into_iter().next().unwrap_or(json!([0,0]))); }
                            }
                        }
                        json!(serde_json::to_string(&Value::Object(obj)).unwrap_or(m.content.clone()))
                    }
                    _ => json!(m.content),
                }
            };
            map.insert("content".to_string(), content_val);
            if let Some(c) = &m.style.color { map.insert("color".to_string(), serde_json::json!(c)); }
            if let Some(s) = m.style.size { map.insert("font_size".to_string(), serde_json::json!(s)); }
            if let Some(f) = &m.style.font { map.insert("font".to_string(), serde_json::json!(f)); }
        }
        v
    }).collect();
    let mats = Materials {
        extra: base.materials.extra.clone(),
        stickers: base.materials.stickers.clone(),
        video_effects: base.materials.video_effects.clone(),
        transitions: base.materials.transitions.clone(),
        masks: base.materials.masks.clone(),
        chromas: base.materials.chromas.clone(),
        audio_fades: base.materials.audio_fades.clone(),
        audio_effects: base.materials.audio_effects.clone(),
        canvases: base.materials.canvases.clone(),
        speeds: base.materials.speeds.clone(),
        sound_channel_mappings: base.materials.sound_channel_mappings.clone(),
        vocal_separations: base.materials.vocal_separations.clone(),
        placeholders: base.materials.placeholders.clone(),
        common_mask: base.materials.common_mask.clone(),
        videos,
        audios,
        texts,
    };
    base.materials = mats;
    base
}
