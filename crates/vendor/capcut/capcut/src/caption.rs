use crate::draft::{Draft, Segment, Timerange, Track};
use crate::srt::parse_srt;
use crate::ass::parse_ass;

/// Import SRT content into draft: creates text materials + text track segments.
/// Returns number of cues imported.
pub fn import_srt_to_draft(draft: &mut Draft, content: &str) -> Result<usize, String> {
    let cues = parse_srt(content).map_err(|e| e.to_string())?;
    import_cues_to_draft(draft, cues.into_iter().map(|c| (c.start_us, c.end_us, c.text)))
}

/// Import ASS content into draft.
pub fn import_ass_to_draft(draft: &mut Draft, content: &str) -> Result<usize, String> {
    let cues = parse_ass(content).map_err(|e| e.to_string())?;
    import_cues_to_draft(draft, cues.into_iter().map(|c| (c.start_us, c.end_us, c.text)))
}

fn import_cues_to_draft(
    draft: &mut Draft,
    cues: impl IntoIterator<Item = (i64, i64, String)>,
) -> Result<usize, String> {
    // Ensure text track exists
    let track_id = if let Some(t) = draft.tracks.iter().find(|t| t.kind == "text") {
        t.id.clone()
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        draft.tracks.push(Track {
            id: id.clone(),
            kind: "text".to_string(),
            name: "captions".to_string(),
            segments: Vec::new(),
            extra: Default::default(),
        });
        id
    };
    let track = draft.tracks.iter_mut().find(|t| t.id == track_id).unwrap();
    let mut count = 0;
    for (start_us, end_us, text) in cues {
        let duration = end_us - start_us;
        if duration <= 0 {
            continue;
        }
        let mat_id = uuid::Uuid::new_v4().to_string();
        let seg_id = uuid::Uuid::new_v4().to_string();
        // CapCut text material stores content as JSON string in "content" field with styles
        let content_json = serde_json::json!({
            "text": text,
            "styles": []
        });
        let mat = serde_json::json!({
            "id": mat_id,
            "content": content_json.to_string(),
            "text": text,
        });
        // Push to materials.texts
        draft.materials.texts.push(mat);
        // Need material id from the mat we just pushed
        let pushed_mat_id = draft.materials.texts.last().unwrap().get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        // Actually we have mat_id
        let _ = pushed_mat_id;
        track.segments.push(Segment {
            id: seg_id,
            material_id: mat_id,
            target_timerange: Timerange { start: start_us, duration },
            source_timerange: None,
            speed: None,
            volume: None,
            extra: Default::default(),
        });
        if end_us > draft.duration {
            draft.duration = end_us;
        }
        count += 1;
    }
    Ok(count)
}

/// Also expose InternalTimeline-based helper
pub fn import_srt_to_timeline(tl: &mut capcut_core::InternalTimeline, content: &str) -> Result<usize, String> {
    let cues = parse_srt(content).map_err(|e| e.to_string())?;
    for c in &cues {
        let dur = c.end_us - c.start_us;
        if dur > 0 {
            tl.add_text(c.text.clone(), c.start_us, dur, Default::default());
        }
    }
    Ok(cues.len())
}
