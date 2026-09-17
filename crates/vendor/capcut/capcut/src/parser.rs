use crate::draft::Draft;
use capcut_core::timeline::*;

fn track_kind(s: &str) -> TrackKind {
    match s {
        "video" => TrackKind::Video,
        "audio" => TrackKind::Audio,
        "text" => TrackKind::Text,
        "sticker" => TrackKind::Sticker,
        _ => TrackKind::Unknown,
    }
}

pub fn from_draft(draft: &Draft) -> InternalTimeline {
    let canvas = parse_canvas(&draft.canvas_config);
    let mut tl = InternalTimeline {
        id: draft.id.clone(),
        name: draft.name.clone(),
        duration: draft.duration,
        fps: draft.fps,
        canvas,
        tracks: Vec::new(),
        materials: Materials::default(),
    };

    // materials: extract known fields, keep rest in extra
    for v in &draft.materials.videos {
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let path = v.get("path").or_else(|| v.get("local_material_id")).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let duration = v.get("duration").and_then(|x| x.as_i64()).unwrap_or(0);
        let width = v.get("width").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        let height = v.get("height").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        let crop = parse_crop(v);
        if id.is_empty() { continue; }
        tl.materials.videos.push(VideoMaterial { id, path, duration, width, height, crop, extra: v.clone() });
    }
    for v in &draft.materials.audios {
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let path = v.get("path").or_else(|| v.get("local_material_id")).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let duration = v.get("duration").and_then(|x| x.as_i64()).unwrap_or(0);
        if id.is_empty() { continue; }
        tl.materials.audios.push(AudioMaterial { id, path, duration, extra: v.clone() });
    }
    for v in &draft.materials.texts {
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let content = extract_text_content(v);
        let style_ranges = parse_style_ranges(v);
        if id.is_empty() { continue; }
        let mut style = TextStyle::default();
        if let Some(c) = v.get("color").and_then(|x| x.as_str()) { style.color = Some(c.to_string()); }
        if let Some(s) = v.get("font_size").and_then(|x| x.as_u64()) { style.size = Some(s as u32); }
        if let Some(f) = v.get("font").and_then(|x| x.as_str()) { style.font = Some(f.to_string()); }
        tl.materials.texts.push(TextMaterial { id, content, style, style_ranges, extra: v.clone() });
    }

    for t in &draft.tracks {
        let kind = track_kind(&t.kind);
        let mut track = Track { id: t.id.clone(), kind, name: t.name.clone(), segments: Vec::new(), muted: false };
        for s in &t.segments {
            let opacity = s.extra.get("clip").and_then(|c| c.get("alpha")).and_then(|v| v.as_f64()).map(|v| v as f32);
            track.segments.push(Segment {
                id: s.id.clone(),
                material_id: s.material_id.clone(),
                timerange: Range::new(s.target_timerange.start, s.target_timerange.duration),
                source_timerange: s.source_timerange.as_ref().map(|r| Range::new(r.start, r.duration)),
                speed: s.speed,
                volume: s.volume,
                opacity,
                extra: serde_json::Value::Object(s.extra.clone()),
            });
        }
        tl.tracks.push(track);
    }

    tl
}

fn parse_canvas(v: &serde_json::Value) -> Canvas {
    let w = v.get("width").and_then(|x| x.as_u64()).unwrap_or(1920) as u32;
    let h = v.get("height").and_then(|x| x.as_u64()).unwrap_or(1080) as u32;
    Canvas { width: w, height: h }
}

fn parse_crop(v: &serde_json::Value) -> Option<CropRect> {
    let crop = v.get("crop")?.as_object()?;
    let x = crop.get("upper_left_x")?.as_f64()?;
    let y = crop.get("upper_left_y")?.as_f64()?;
    let rx = crop.get("upper_right_x")?.as_f64()?;
    let by = crop.get("lower_left_y")?.as_f64()?;
    let w = rx - x;
    let h = by - y;
    CropRect::new(x, y, w, h).ok()
}

fn parse_style_ranges(v: &serde_json::Value) -> Vec<StyleRange> {
    // CapCut rich text: content is JSON string with styles[].range
    let content_str = match v.get("content").and_then(|x| x.as_str()) {
        Some(s) => s,
        None => return Vec::new(),
    };
    let parsed: serde_json::Value = match serde_json::from_str(content_str) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let arr = match parsed.get("styles").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for s in arr {
        if let Some(r) = s.get("range").and_then(|x| x.as_array()) {
            if r.len() == 2 {
                if let (Some(a), Some(b)) = (r[0].as_i64(), r[1].as_i64()) {
                    out.push(StyleRange { start: a, end: b });
                }
            }
        }
    }
    out
}

fn extract_text_content(v: &serde_json::Value) -> String {
    if let Some(s) = v.get("content").and_then(|x| x.as_str()) {
        // try rich JSON
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
            if let Some(t) = parsed.get("text").and_then(|x| x.as_str()) { return t.to_string(); }
        }
        return s.to_string();
    }
    if let Some(s) = v.get("text").and_then(|x| x.as_str()) { return s.to_string(); }
    String::new()
}
