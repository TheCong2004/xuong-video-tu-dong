use capcut_core::error::{CoreError, Result as CoreResult};
use capcut_core::timeline::{CropRect, InternalTimeline, Range, StyleRange, TextStyle, TrackKind};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Op dispatch — mirrors reference compile operations + imperative ops
// ---------------------------------------------------------------------------

/// Apply a list of JSON ops to an [`InternalTimeline`].
///
/// Each element must be an object with an `"op"` string field.
/// Unknown ops return `CoreError::InvalidArgument`.
pub fn apply_ops(timeline: &mut InternalTimeline, ops: &[Value]) -> CoreResult<()> {
    for (idx, op) in ops.iter().enumerate() {
        apply_one(timeline, op).map_err(|e| match e {
            CoreError::InvalidArgument(msg) => {
                CoreError::InvalidArgument(format!("ops[{idx}]: {msg}"))
            }
            CoreError::NotFound(msg) => CoreError::NotFound(format!("ops[{idx}]: {msg}")),
            CoreError::Conflict(msg) => CoreError::Conflict(format!("ops[{idx}]: {msg}")),
        })?;
    }
    Ok(())
}

/// Parse a JSON array string and apply ops.
pub fn run_from_json(timeline: &mut InternalTimeline, json: &str) -> CoreResult<()> {
    let ops: Vec<Value> = serde_json::from_str(json)
        .map_err(|e| CoreError::InvalidArgument(format!("run_from_json: invalid JSON: {e}")))?;
    apply_ops(timeline, &ops)
}

fn str_field<'a>(obj: &'a serde_json::Map<String, Value>, key: &str) -> CoreResult<&'a str> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| CoreError::InvalidArgument(format!("missing string field '{key}'")))
}

fn opt_str<'a>(obj: &'a serde_json::Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn i64_field(obj: &serde_json::Map<String, Value>, key: &str) -> CoreResult<i64> {
    obj.get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| CoreError::InvalidArgument(format!("missing i64 field '{key}'")))
}

fn opt_i64(obj: &serde_json::Map<String, Value>, key: &str) -> Option<i64> {
    obj.get(key).and_then(|v| v.as_i64())
}

fn f64_field(obj: &serde_json::Map<String, Value>, key: &str) -> CoreResult<f64> {
    obj.get(key)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| CoreError::InvalidArgument(format!("missing number field '{key}'")))
}

fn apply_one(timeline: &mut InternalTimeline, op: &Value) -> CoreResult<()> {
    let obj = op
        .as_object()
        .ok_or_else(|| CoreError::InvalidArgument("each op must be an object".to_string()))?;
    let op_name = str_field(obj, "op")?;
    match op_name {
        "add_video" => {
            let path = str_field(obj, "path")?.to_string();
            let start = i64_field(obj, "start")?;
            let duration = i64_field(obj, "duration")?;
            let width = obj
                .get("width")
                .and_then(|v| v.as_u64())
                .unwrap_or(1920) as u32;
            let height = obj
                .get("height")
                .and_then(|v| v.as_u64())
                .unwrap_or(1080) as u32;
            timeline.add_video(path, start, duration, width, height);
            Ok(())
        }
        "add_audio" => {
            let path = str_field(obj, "path")?.to_string();
            let start = i64_field(obj, "start")?;
            let duration = i64_field(obj, "duration")?;
            timeline.add_audio(path, start, duration);
            Ok(())
        }
        "add_text" => {
            let content = str_field(obj, "text")?.to_string();
            let start = i64_field(obj, "start")?;
            let duration = i64_field(obj, "duration")?;
            let mut style = TextStyle::default();
            if let Some(s) = opt_str(obj, "font") {
                style.font = Some(s);
            }
            if let Some(v) = obj.get("font_size").and_then(|v| v.as_u64()) {
                style.size = Some(v as u32);
            }
            if let Some(s) = opt_str(obj, "color") {
                style.color = Some(s);
            }
            timeline.add_text(content, start, duration, style);
            Ok(())
        }
        "trim" => {
            let id = str_field(obj, "segment_id")?;
            let start = i64_field(obj, "start")?;
            let duration = i64_field(obj, "duration")?;
            if duration <= 0 {
                return Err(CoreError::InvalidArgument("trim duration must be >0".to_string()));
            }
            timeline.trim_segment(id, Range::new(start, duration))
        }
        "speed" => {
            let id = str_field(obj, "segment_id")?;
            let speed = f64_field(obj, "speed")?;
            timeline.set_speed(id, speed)
        }
        "volume" => {
            let id = str_field(obj, "segment_id")?;
            let vol = obj
                .get("volume")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| CoreError::InvalidArgument("missing volume".to_string()))?
                as f32;
            timeline.set_volume(id, vol)
        }
        "opacity" => {
            let id = str_field(obj, "segment_id")?;
            let op = obj
                .get("opacity")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| CoreError::InvalidArgument("missing opacity".to_string()))?
                as f32;
            timeline.set_opacity(id, op)
        }
        "crop" => {
            let id = str_field(obj, "segment_id")?;
            let x = f64_field(obj, "x")?;
            let y = f64_field(obj, "y")?;
            let w = f64_field(obj, "w")?;
            let h = f64_field(obj, "h")?;
            let rect = CropRect::new(x, y, w, h)?;
            timeline.set_crop(id, rect)
        }
        "clear_crop" => {
            let id = str_field(obj, "segment_id")?;
            timeline.clear_crop(id)
        }
        "cut" => {
            let id = str_field(obj, "segment_id")?;
            let at = i64_field(obj, "at")?;
            timeline.cut_segment(id, at).map(|_| ())
        }
        "duplicate" => {
            let id = str_field(obj, "segment_id")?;
            let target = opt_str(obj, "target_track");
            timeline
                .duplicate_segment(id, target.as_deref())
                .map(|_| ())
        }
        "remove_segment" => {
            let id = str_field(obj, "segment_id")?;
            timeline.remove_segment(id).map(|_| ())
        }
        "remove_segment_gc" => {
            let id = str_field(obj, "segment_id")?;
            timeline.remove_segment_gc(id).map(|_| ())
        }
        "shift" => {
            let id = str_field(obj, "segment_id")?;
            let delta = i64_field(obj, "delta")?;
            timeline.shift_segment(id, delta)
        }
        "create_track" => {
            let kind_str = str_field(obj, "kind")?;
            let name = str_field(obj, "name")?;
            let kind = match kind_str {
                "video" => TrackKind::Video,
                "audio" => TrackKind::Audio,
                "text" => TrackKind::Text,
                "sticker" => TrackKind::Sticker,
                "effect" => TrackKind::Effect,
                "filter" => TrackKind::Filter,
                _ => return Err(CoreError::InvalidArgument(format!("unknown track kind '{kind_str}'"))),
            };
            timeline.create_track(kind, name).map(|_| ())
        }
        "remove_track" => {
            let name = str_field(obj, "name")?;
            timeline.remove_track(name).map(|_| ())
        }
        "rename_track" => {
            let old = str_field(obj, "old")?;
            let new = str_field(obj, "new")?;
            timeline.rename_track(old, new)
        }
        "text_content" => {
            let id = str_field(obj, "material_id")?;
            let content = str_field(obj, "content")?.to_string();
            timeline.set_text_content(id, content)
        }
        "text_style" => {
            let id = str_field(obj, "material_id")?;
            let mut style = TextStyle::default();
            if let Some(s) = opt_str(obj, "font") {
                style.font = Some(s);
            }
            if let Some(v) = obj.get("font_size").and_then(|v| v.as_u64()) {
                style.size = Some(v as u32);
            }
            if let Some(s) = opt_str(obj, "color") {
                style.color = Some(s);
            }
            timeline.set_text_style(id, style)
        }
        "text_ranges" => {
            let id = str_field(obj, "material_id")?;
            let content = opt_str(obj, "content").unwrap_or_default();
            let ranges_val = obj
                .get("ranges")
                .ok_or_else(|| CoreError::InvalidArgument("missing ranges".to_string()))?;
            let arr = ranges_val
                .as_array()
                .ok_or_else(|| CoreError::InvalidArgument("ranges must be array".to_string()))?;
            let mut ranges = Vec::new();
            for item in arr {
                let a = item
                    .as_array()
                    .ok_or_else(|| CoreError::InvalidArgument("each range must be [start,end]".to_string()))?;
                if a.len() != 2 {
                    return Err(CoreError::InvalidArgument("range must have 2 elements".to_string()));
                }
                let s = a[0]
                    .as_i64()
                    .ok_or_else(|| CoreError::InvalidArgument("range values must be integers".to_string()))?;
                let e = a[1]
                    .as_i64()
                    .ok_or_else(|| CoreError::InvalidArgument("range values must be integers".to_string()))?;
                ranges.push(StyleRange { start: s, end: e });
            }
            // if content is provided use set_text_with_ranges, else just update ranges on existing content
            if content.is_empty() {
                // need existing content length check — do via set_text_with_ranges with current content
                let current = timeline
                    .materials
                    .texts
                    .iter()
                    .find(|m| m.id == id)
                    .ok_or_else(|| CoreError::NotFound(format!("text material {id}")))?
                    .content
                    .clone();
                timeline.set_text_with_ranges(id, current, ranges)
            } else {
                timeline.set_text_with_ranges(id, content, ranges)
            }
        }
        _ => Err(CoreError::InvalidArgument(format!("unknown op '{op_name}'"))),
    }?;
    let _ = opt_i64;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use capcut_core::timeline::{Canvas, InternalTimeline};

    fn fresh_tl() -> InternalTimeline {
        InternalTimeline::new("test", Canvas { width: 1920, height: 1080 }, 30.0)
    }

    #[test]
    fn apply_add_video_and_text() {
        let mut tl = fresh_tl();
        let ops = vec![
            serde_json::json!({"op":"add_video","path":"clip.mp4","start":0,"duration":3_000_000}),
            serde_json::json!({"op":"add_text","text":"hello","start":0,"duration":1_000_000}),
        ];
        apply_ops(&mut tl, &ops).unwrap();
        assert_eq!(tl.tracks.len(), 2);
        assert_eq!(tl.materials.videos.len(), 1);
        assert_eq!(tl.materials.texts.len(), 1);
    }

    #[test]
    fn run_from_json_roundtrip() {
        let mut tl = fresh_tl();
        let json = r#"[{"op":"add_audio","path":"music.mp3","start":0,"duration":2000000}]"#;
        run_from_json(&mut tl, json).unwrap();
        assert_eq!(tl.materials.audios.len(), 1);
    }

    #[test]
    fn unknown_op_errors() {
        let mut tl = fresh_tl();
        let ops = vec![serde_json::json!({"op":"nope"})];
        assert!(apply_ops(&mut tl, &ops).is_err());
    }
}
