use capcut_core::error::{CoreError, Result as CoreResult};
use serde_json::{Value, json};

const CAPTION_MARKER_COLOR: &str = "YELLOW";

// ---------------------------------------------------------------------------
// OTIO helper constructors (mirrors interchange.ts)
// ---------------------------------------------------------------------------

fn rational_time(value: i64, rate: f64) -> Value {
    json!({"OTIO_SCHEMA":"RationalTime.1","rate":rate,"value":value})
}

fn time_range(start: i64, duration: i64, rate: f64) -> Value {
    json!({"OTIO_SCHEMA":"TimeRange.1","start_time":rational_time(start, rate),"duration":rational_time(duration, rate)})
}

fn gap(duration: i64, rate: f64) -> Value {
    json!({
        "OTIO_SCHEMA":"Gap.1","effects":[],"markers":[],"metadata":{},"name":"",
        "source_range": time_range(0, duration, rate)
    })
}

fn frames_for(us: i64, rate: f64) -> i64 {
    capcut_core::time::frames_for(us, Some(rate))
}

// ---------------------------------------------------------------------------
// Export types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OtioSkip {
    pub track: String,
    pub type_name: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct OtioStats {
    pub tracks: usize,
    pub clips: usize,
    pub gaps: usize,
    pub captions: usize,
    pub skipped: Vec<OtioSkip>,
}

#[derive(Debug, Clone, Default)]
pub struct OtioExportOptions {
    pub captions: CaptionsMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CaptionsMode {
    #[default]
    Skip,
    Markers,
}

// ---------------------------------------------------------------------------
// Import plan types (mirrors ImportPlan / ImportClipPlan / ImportTrackPlan)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ImportClipPlan {
    pub name: String,
    pub target_start_us: i64,
    pub target_duration_us: i64,
    pub source_start_us: i64,
    pub source_duration_us: i64,
    pub speed: f64,
    pub volume: Option<f32>,
    pub media_path: Option<String>,
    pub media_duration_us: i64,
}

#[derive(Debug, Clone)]
pub struct ImportTrackPlan {
    pub kind: String, // "video" | "audio"
    pub name: String,
    pub clips: Vec<ImportClipPlan>,
}

#[derive(Debug, Clone)]
pub struct ImportCaptionPlan {
    pub text: String,
    pub start_us: i64,
    pub duration_us: i64,
    pub track: String,
}

#[derive(Debug, Clone)]
pub struct ImportPlan {
    pub name: String,
    pub rate: f64,
    pub tracks: Vec<ImportTrackPlan>,
    pub clips: usize,
    pub gaps: usize,
    pub captions: Vec<ImportCaptionPlan>,
    pub skipped: Vec<OtioSkip>,
}

// ---------------------------------------------------------------------------
// draft -> OTIO (mirrors draftToOtio)
// ---------------------------------------------------------------------------

fn extract_text(content: &str) -> String {
    // reuse crate::draft::extract_text if available; fallback to naive
    crate::draft::extract_text(content)
}

/// Export a [`crate::draft::Draft`] to an OTIO 0.14 document.
pub fn draft_to_otio(
    draft: &crate::draft::Draft,
    opts: &OtioExportOptions,
) -> (Value, OtioStats) {
    let rate: f64 = if draft.fps > 0.0 { draft.fps as f64 } else { 30.0 };
    let to_frames = |us: i64| frames_for(us, rate);
    let mut children: Vec<Value> = Vec::new();
    let mut stack_markers: Vec<Value> = Vec::new();
    let mut stats = OtioStats { tracks: 0, clips: 0, gaps: 0, captions: 0, skipped: Vec::new() };

    for track in &draft.tracks {
        let otio_kind = match track.kind.as_str() {
            "video" => Some("Video"),
            "audio" => Some("Audio"),
            _ => None,
        };
        if otio_kind.is_none() {
            if track.kind == "text" && opts.captions == CaptionsMode::Markers {
                let mut segs = track.segments.clone();
                segs.sort_by_key(|s| s.target_timerange.start);
                for seg in &segs {
                    let mat = draft.materials.texts.iter().find(|m| m.get("id").and_then(|v| v.as_str()) == Some(&seg.material_id));
                    let text = mat.and_then(|m| m.get("content")).and_then(|v| v.as_str()).map(extract_text).unwrap_or_default();
                    if text.is_empty() { continue; }
                    let marker = json!({
                        "OTIO_SCHEMA":"Marker.1",
                        "color": CAPTION_MARKER_COLOR,
                        "marked_range": time_range(to_frames(seg.target_timerange.start), to_frames(seg.target_timerange.duration).max(1), rate),
                        "metadata": {
                            "Resolve_OTIO": {"Keywords":[],"Note": text},
                            "capcut": {"kind":"caption","text": text, "track": track.name, "track_id": track.id, "segment_id": seg.id, "material_id": seg.material_id}
                        },
                        "name": text
                    });
                    stack_markers.push(marker);
                    stats.captions += 1;
                }
                continue;
            }
            stats.skipped.push(OtioSkip {
                track: track.name.clone(),
                type_name: track.kind.clone(),
                reason: if track.kind == "text" {
                    "OTIO has no standard title schema — pass captions=markers or use export-srt".to_string()
                } else {
                    "no portable OTIO equivalent for this track type".to_string()
                },
            });
            continue;
        }
        if track.segments.is_empty() { continue; }
        let mut segs = track.segments.clone();
        segs.sort_by_key(|s| s.target_timerange.start);
        let mut items: Vec<Value> = Vec::new();
        let mut cursor_us: i64 = 0;
        for seg in &segs {
            let gap_us = seg.target_timerange.start - cursor_us;
            let gap_frames = to_frames(gap_us);
            if gap_frames > 0 {
                items.push(gap(gap_frames, rate));
                stats.gaps += 1;
            }
            // media lookup
            let media = find_media_for_segment(draft, seg);
            let speed = seg.speed.unwrap_or(1.0);
            let mut effects: Vec<Value> = Vec::new();
            if (speed - 1.0).abs() > f64::EPSILON {
                effects.push(json!({
                    "OTIO_SCHEMA":"LinearTimeWarp.1",
                    "effect_name":"LinearTimeWarp","metadata":{},"name":"speed",
                    "time_scalar": speed
                }));
            }
            let media_ref = if let Some((path, name, dur_us)) = media {
                if !path.is_empty() {
                    let avail = if dur_us > 0 { Value::Object({
                        let mut m = serde_json::Map::new();
                        let tr = time_range(0, to_frames(dur_us), rate);
                        if let Value::Object(o) = tr { for (k,v) in o { m.insert(k,v); } }
                        m
                    }) } else { Value::Null };
                    // use avail as TimeRange or null
                    let avail2 = if dur_us > 0 { time_range(0, to_frames(dur_us), rate) } else { Value::Null };
                    let _ = avail;
                    json!({"OTIO_SCHEMA":"ExternalReference.1","available_range": avail2,"metadata":{},"name": name,"target_url": path})
                } else {
                    json!({"OTIO_SCHEMA":"MissingReference.1","metadata":{},"name": name})
                }
            } else {
                json!({"OTIO_SCHEMA":"MissingReference.1","metadata":{},"name": seg.material_id})
            };
            let clip = json!({
                "OTIO_SCHEMA":"Clip.1",
                "effects": effects,
                "markers": [],
                "media_reference": media_ref,
                "metadata": {"capcut":{"material_id": seg.material_id, "segment_id": seg.id, "speed": speed, "volume": seg.volume}},
                "name": seg.material_id,
                "source_range": time_range(
                    to_frames(seg.source_timerange.as_ref().map(|r| r.start).unwrap_or(0)),
                    to_frames(seg.source_timerange.as_ref().map(|r| r.duration).unwrap_or(seg.target_timerange.duration)),
                    rate
                )
            });
            items.push(clip);
            stats.clips += 1;
            cursor_us = seg.target_timerange.start + seg.target_timerange.duration;
        }
        children.push(json!({
            "OTIO_SCHEMA":"Track.1",
            "children": items,
            "effects": [],
            "kind": otio_kind.unwrap(),
            "markers": [],
            "metadata": {"capcut":{"track_id": track.id}},
            "name": track.name,
            "source_range": Value::Null
        }));
        stats.tracks += 1;
    }

    let doc = json!({
        "OTIO_SCHEMA":"Timeline.1",
        "global_start_time": rational_time(0, rate),
        "metadata": {"capcut":{"draft_id": draft.id, "duration_us": draft.duration, "exported_by":"capcut-cli export-timeline"}},
        "name": if draft.name.is_empty() { Value::String("capcut draft".to_string()) } else { Value::String(draft.name.clone()) },
        "tracks": {
            "OTIO_SCHEMA":"Stack.1",
            "children": children,
            "effects": [],
            "markers": stack_markers,
            "metadata": {},
            "name": "tracks",
            "source_range": Value::Null
        }
    });
    (doc, stats)
}

fn find_media_for_segment(
    draft: &crate::draft::Draft,
    seg: &crate::draft::Segment,
) -> Option<(String, String, i64)> {
    for kind in ["videos", "audios"] {
        let arr = match kind {
            "videos" => &draft.materials.videos,
            "audios" => &draft.materials.audios,
            _ => continue,
        };
        for mat in arr {
            if mat.get("id").and_then(|v| v.as_str()) != Some(&seg.material_id) { continue; }
            let path = mat.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = mat.get("material_name").or_else(|| mat.get("name")).and_then(|v| v.as_str()).unwrap_or(if path.is_empty() { &seg.material_id } else { &path }).to_string();
            let dur = mat.get("duration").and_then(|v| v.as_i64()).unwrap_or(0);
            return Some((path, name, dur));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// OTIO -> ImportPlan (mirrors otioToImportPlan)
// ---------------------------------------------------------------------------

fn schema_of(v: &Value) -> &str {
    v.get("OTIO_SCHEMA").and_then(|x| x.as_str()).unwrap_or("")
}

fn us_from_rational(rt: &Value, fallback_rate: f64, ctx: &str) -> CoreResult<i64> {
    if schema_of(rt) != "RationalTime.1" {
        return Err(CoreError::InvalidArgument(format!("{ctx} is not a RationalTime.1")));
    }
    let rate = rt.get("rate").and_then(|v| v.as_f64()).filter(|r| r.is_finite() && *r > 0.0).unwrap_or(fallback_rate);
    let value = rt.get("value").and_then(|v| v.as_f64()).ok_or_else(|| CoreError::InvalidArgument(format!("{ctx} has no numeric value")))?;
    Ok(((value / rate) * 1_000_000.0).round() as i64)
}

fn time_range_us(v: &Value, fallback_rate: f64, ctx: &str) -> CoreResult<(i64, i64)> {
    if schema_of(v) != "TimeRange.1" {
        return Err(CoreError::InvalidArgument(format!("{ctx} is not a TimeRange.1")));
    }
    let start = us_from_rational(&v["start_time"], fallback_rate, &format!("{ctx}.start_time"))?;
    let dur = us_from_rational(&v["duration"], fallback_rate, &format!("{ctx}.duration"))?;
    Ok((start, dur))
}

pub fn otio_to_import_plan(doc: &Value) -> CoreResult<ImportPlan> {
    if schema_of(doc) != "Timeline.1" {
        return Err(CoreError::InvalidArgument(format!(
            "not an OTIO Timeline.1 document (OTIO_SCHEMA: {})",
            schema_of(doc)
        )));
    }
    let mut skipped: Vec<OtioSkip> = Vec::new();
    let rate = doc
        .get("global_start_time")
        .and_then(|v| v.get("rate"))
        .and_then(|v| v.as_f64())
        .filter(|r| r.is_finite() && *r > 0.0)
        .unwrap_or(30.0);
    if let Some(v) = doc.get("global_start_time").and_then(|x| x.get("value")).and_then(|v| v.as_f64()) {
        if v != 0.0 {
            skipped.push(OtioSkip { track: "(timeline)".to_string(), type_name: "global_start_time".to_string(), reason: "non-zero timeline start is ignored — CapCut drafts start at 0".to_string() });
        }
    }
    let stack = &doc["tracks"];
    if schema_of(stack) != "Stack.1" || !stack.get("children").map(|v| v.is_array()).unwrap_or(false) {
        return Err(CoreError::InvalidArgument("timeline has no Stack.1 tracks container".to_string()));
    }
    // captions from markers
    let mut captions: Vec<ImportCaptionPlan> = Vec::new();
    if let Some(markers) = stack.get("markers").and_then(|v| v.as_array()) {
        let mut foreign = 0usize;
        for m in markers {
            let capcut = m.get("metadata").and_then(|x| x.get("capcut"));
            let is_caption = capcut.and_then(|c| c.get("kind")).and_then(|v| v.as_str()) == Some("caption");
            if !is_caption { foreign += 1; continue; }
            let range = match m.get("marked_range") {
                Some(r) => r,
                None => continue,
            };
            let (start, dur) = time_range_us(range, rate, "caption marker marked_range")?;
            let text = capcut.and_then(|c| c.get("text")).and_then(|v| v.as_str()).or_else(|| m.get("name").and_then(|v| v.as_str())).unwrap_or("").to_string();
            if text.is_empty() { skipped.push(OtioSkip{track:"(timeline)".to_string(), type_name:"markers".to_string(), reason:"a caption marker carries no text — skipped".to_string()}); continue; }
            let track = capcut.and_then(|c| c.get("track")).and_then(|v| v.as_str()).unwrap_or("captions").to_string();
            captions.push(ImportCaptionPlan { text, start_us: start, duration_us: dur.max(1), track });
        }
        if foreign > 0 {
            skipped.push(OtioSkip{track:"(timeline)".to_string(), type_name:"markers".to_string(), reason: format!("{foreign} timeline marker(s) without capcut caption metadata have no CapCut equivalent")});
        }
    }

    let mut plan = ImportPlan {
        name: doc.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        rate,
        tracks: Vec::new(),
        clips: 0,
        gaps: 0,
        captions,
        skipped,
    };

    let children = stack.get("children").and_then(|v| v.as_array()).unwrap();
    for child in children {
        let schema = schema_of(child);
        let label = child.get("name").and_then(|v| v.as_str()).unwrap_or(schema).to_string();
        if schema != "Track.1" {
            plan.skipped.push(OtioSkip{track: label, type_name: schema.to_string(), reason:"unsupported stack child — only Track.1 imports".to_string()});
            continue;
        }
        let kind_raw = child.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let kind = match kind_raw { "Video" => Some("video"), "Audio" => Some("audio"), _ => None };
        let Some(kind) = kind else {
            plan.skipped.push(OtioSkip{track: label.clone(), type_name: kind_raw.to_string(), reason:"unsupported track kind — only Video and Audio tracks import".to_string()});
            continue;
        };
        if child.get("effects").and_then(|v| v.as_array()).map(|a| !a.is_empty()).unwrap_or(false) {
            plan.skipped.push(OtioSkip{track: label.clone(), type_name:"effects".to_string(), reason:"track-level effects have no CapCut equivalent".to_string()});
        }
        let mut clips: Vec<ImportClipPlan> = Vec::new();
        let mut cursor_us: i64 = 0;
        let items = child.get("children").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        for item in &items {
            let s = schema_of(item);
            if s == "Gap.1" {
                if let Some(sr) = item.get("source_range") {
                    if let Ok((_, dur)) = time_range_us(sr, rate, &format!("gap in track \"{label}\"")) {
                        cursor_us += dur;
                        plan.gaps += 1;
                    }
                }
                continue;
            }
            if s != "Clip.1" {
                plan.skipped.push(OtioSkip{track: label.clone(), type_name: s.to_string(), reason:"unsupported timeline item — only Clip.1 and Gap.1 import".to_string()});
                continue;
            }
            // source_range
            let source = item.get("source_range").ok_or_else(|| CoreError::InvalidArgument(format!("clip in track \"{label}\" has no source_range")))?;
            let (src_start, src_dur) = time_range_us(source, rate, &format!("clip in track \"{label}\" source_range"))?;
            // speed from LinearTimeWarp
            let mut speed = 1.0;
            let mut saw = false;
            for eff in item.get("effects").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
                if schema_of(&eff) == "LinearTimeWarp.1" && !saw {
                    if let Some(sc) = eff.get("time_scalar").and_then(|v| v.as_f64()) { if sc > 0.0 { speed = sc; saw = true; continue; } }
                }
                plan.skipped.push(OtioSkip{track: label.clone(), type_name: schema_of(&eff).to_string(), reason: format!("unsupported effect on clip \"{}\"", item.get("name").and_then(|v| v.as_str()).unwrap_or("clip"))});
            }
            let target_dur = ((src_dur as f64) / speed).round() as i64;
            // media
            let media_ref = &item["media_reference"];
            let (media_path, media_dur) = match schema_of(media_ref) {
                "ExternalReference.1" => {
                    let p = media_ref.get("target_url").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let d = media_ref.get("available_range").and_then(|v| if v.is_null() { None } else { Some(v) }).map(|v| time_range_us(v, rate, "available_range").map(|(_, d)| d).unwrap_or(0)).unwrap_or(0);
                    (p, d)
                }
                "MissingReference.1" => (None, 0),
                other => {
                    plan.skipped.push(OtioSkip{track: label.clone(), type_name: other.to_string(), reason: format!("unsupported media reference on clip \"{}\"", item.get("name").and_then(|v| v.as_str()).unwrap_or("clip"))});
                    (None, 0)
                }
            };
            let vol = item.get("metadata").and_then(|m| m.get("capcut")).and_then(|c| c.get("volume")).and_then(|v| v.as_f64()).map(|v| v as f32);
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("clip").to_string();
            let clip = ImportClipPlan {
                name,
                target_start_us: cursor_us,
                target_duration_us: target_dur,
                source_start_us: src_start,
                source_duration_us: src_dur,
                speed,
                volume: vol,
                media_path,
                media_duration_us: media_dur,
            };
            cursor_us += target_dur;
            clips.push(clip);
            plan.clips += 1;
        }
        plan.tracks.push(ImportTrackPlan { kind: kind.to_string(), name: child.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(), clips });
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_draft() -> crate::draft::Draft {
        let mut d = crate::draft::Draft {
            id: "draft1".to_string(),
            name: "test".to_string(),
            duration: 3_000_000,
            fps: 30.0,
            canvas_config: json!({}),
            platform: json!({}),
            tracks: vec![],
            materials: Default::default(),
            extra_info: json!({}),
            extra: Default::default(),
        };
        d.tracks.push(crate::draft::Track {
            id: "t1".to_string(),
            kind: "video".to_string(),
            name: "video".to_string(),
            segments: vec![crate::draft::Segment {
                id: "s1".to_string(),
                material_id: "m1".to_string(),
                target_timerange: crate::draft::Timerange { start: 0, duration: 3_000_000 },
                source_timerange: Some(crate::draft::Timerange { start: 0, duration: 3_000_000 }),
                speed: None,
                volume: None,
                extra: Default::default(),
            }],
            extra: Default::default(),
        });
        d.materials.videos.push(json!({"id":"m1","path":"/tmp/clip.mp4","duration":5_000_000}));
        d
    }

    #[test]
    fn draft_to_otio_roundtrip() {
        let draft = sample_draft();
        let (doc, stats) = draft_to_otio(&draft, &OtioExportOptions::default());
        assert_eq!(stats.tracks, 1);
        assert_eq!(stats.clips, 1);
        let plan = otio_to_import_plan(&doc).unwrap();
        assert_eq!(plan.tracks.len(), 1);
        assert_eq!(plan.clips, 1);
        assert_eq!(plan.tracks[0].clips[0].target_duration_us, 3_000_000);
    }

    #[test]
    fn gap_accounted() {
        let mut draft = sample_draft();
        draft.tracks[0].segments.push(crate::draft::Segment {
            id: "s2".to_string(),
            material_id: "m1".to_string(),
            target_timerange: crate::draft::Timerange { start: 5_000_000, duration: 1_000_000 },
            source_timerange: Some(crate::draft::Timerange { start: 0, duration: 1_000_000 }),
            speed: None,
            volume: None,
            extra: Default::default(),
        });
        let (doc, stats) = draft_to_otio(&draft, &OtioExportOptions::default());
        assert_eq!(stats.gaps, 1);
        let plan = otio_to_import_plan(&doc).unwrap();
        assert_eq!(plan.gaps, 1);
        assert_eq!(plan.clips, 2);
    }
}
