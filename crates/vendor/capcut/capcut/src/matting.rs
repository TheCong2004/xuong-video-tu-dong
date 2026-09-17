use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const MATTING_OFF: i64 = 0;
pub const MATTING_SMART_PORTRAIT: i64 = 3;

#[allow(non_snake_case)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattingObject {
    pub flag: i64,
    pub has_use_quick_brush: bool,
    pub has_use_quick_eraser: bool,
    #[serde(default)]
    pub interactiveTime: Vec<serde_json::Value>,
    pub path: String,
    pub strokes: Vec<serde_json::Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattingResult {
    pub ok: bool,
    pub segment_id: String,
    pub material_id: String,
    pub flag: i64,
    pub enabled: bool,
    pub shared_segments: Vec<String>,
}

fn matting_object(flag: i64, existing: Option<&serde_json::Value>) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "flag": flag,
        "has_use_quick_brush": false,
        "has_use_quick_eraser": false,
        "interactiveTime": [],
        "path": "",
        "strokes": [],
    });
    if let Some(ex) = existing.and_then(|v| v.as_object()) {
        if let Some(map) = obj.as_object_mut() {
            for (k, v) in ex {
                if k != "flag" {
                    map.insert(k.clone(), v.clone());
                }
            }
            map.insert("flag".to_string(), serde_json::Value::Number(flag.into()));
        }
    }
    obj
}

fn find_segment_info<'a>(draft: &'a crate::draft::Draft, seg_id: &str) -> Option<(&'a crate::draft::Track, usize)> {
    for track in &draft.tracks {
        if let Some(idx) = track.segments.iter().position(|s| s.id == seg_id) {
            return Some((track, idx));
        }
    }
    None
}

fn resolve_video_material_mut(draft: &mut crate::draft::Draft, seg_id: &str) -> Result<(String, String, Vec<String>)> {
    let mat_id = {
        let (track, idx) = find_segment_info(draft, seg_id).ok_or_else(|| anyhow::anyhow!("Segment not found: {seg_id}"))?;
        if track.kind != "video" {
            anyhow::bail!("Smart matting only applies to video/photo segments (segment {seg_id} is on a {} track)", track.kind);
        }
        track.segments[idx].material_id.clone()
    };
    let mat_exists = draft.materials.videos.iter().any(|v| v.get("id").and_then(|x| x.as_str()) == Some(&mat_id));
    if !mat_exists {
        anyhow::bail!("Segment {seg_id} references a missing video material: {mat_id}");
    }
    let seg_id_owned = seg_id.to_string();
    let mut shared: Vec<String> = Vec::new();
    for track in &draft.tracks {
        for seg in &track.segments {
            if seg.id != seg_id_owned && seg.material_id == mat_id {
                shared.push(seg.id.clone());
            }
        }
    }
    Ok((seg_id_owned, mat_id, shared))
}

pub fn set_matting(draft: &mut crate::draft::Draft, seg_id: &str) -> Result<MattingResult> {
    let (segment_id, material_id, shared_segments) = resolve_video_material_mut(draft, seg_id)?;
    if let Some(mat) = draft.materials.videos.iter_mut().find(|v| v.get("id").and_then(|x| x.as_str()) == Some(&material_id)) {
        let existing = mat.get("matting").cloned();
        let obj = matting_object(MATTING_SMART_PORTRAIT, existing.as_ref());
        if let Some(obj_map) = mat.as_object_mut() {
            obj_map.insert("matting".to_string(), obj);
        }
    }
    Ok(MattingResult { ok: true, segment_id, material_id, flag: MATTING_SMART_PORTRAIT, enabled: true, shared_segments })
}

pub fn clear_matting(draft: &mut crate::draft::Draft, seg_id: &str) -> Result<MattingResult> {
    let (segment_id, material_id, shared_segments) = resolve_video_material_mut(draft, seg_id)?;
    if let Some(mat) = draft.materials.videos.iter_mut().find(|v| v.get("id").and_then(|x| x.as_str()) == Some(&material_id)) {
        let existing = mat.get("matting").cloned();
        let obj = matting_object(MATTING_OFF, existing.as_ref());
        if let Some(obj_map) = mat.as_object_mut() {
            obj_map.insert("matting".to_string(), obj);
        }
    }
    Ok(MattingResult { ok: true, segment_id, material_id, flag: MATTING_OFF, enabled: false, shared_segments })
}

pub fn matting_state(draft: &crate::draft::Draft, seg_id: &str) -> Result<(String, String, i64)> {
    let (track, idx) = find_segment_info(draft, seg_id).ok_or_else(|| anyhow::anyhow!("Segment not found: {seg_id}"))?;
    let seg = &track.segments[idx];
    let mat_id = seg.material_id.clone();
    let flag = draft.materials.videos.iter().find(|v| v.get("id").and_then(|x| x.as_str()) == Some(&mat_id))
        .and_then(|v| v.get("matting")).and_then(|m| m.get("flag")).and_then(|f| f.as_i64()).unwrap_or(0);
    Ok((seg.id.clone(), mat_id, flag))
}
