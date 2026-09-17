use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromaOptions {
    pub color: String,
    pub intensity: Option<f32>,
    pub shadow: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetChromaResult {
    pub ok: bool,
    pub segment_id: String,
    pub material_id: String,
    pub color: String,
    pub intensity: f32,
    pub shadow: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveChromaResult {
    pub ok: bool,
    pub segment_id: String,
    pub removed: Vec<String>,
}

fn find_segment_mut<'a>(draft: &'a mut crate::draft::Draft, seg_id: &str) -> Option<(&'a mut crate::draft::Track, usize)> {
    for track in &mut draft.tracks {
        if let Some(idx) = track.segments.iter().position(|s| s.id == seg_id) {
            return Some((track, idx));
        }
    }
    None
}

fn find_segment_info<'a>(draft: &'a crate::draft::Draft, seg_id: &str) -> Option<(&'a crate::draft::Track, usize)> {
    for track in &draft.tracks {
        if let Some(idx) = track.segments.iter().position(|s| s.id == seg_id) {
            return Some((track, idx));
        }
    }
    None
}

fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.trim_start_matches('#');
    if hex.len() != 6 { return None; }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(((n >> 16) as u8, ((n >> 8) & 0xff) as u8, (n & 0xff) as u8))
}

fn clamp01(x: f32) -> f32 { x.clamp(0.0, 1.0) }

pub fn set_chroma(draft: &mut crate::draft::Draft, seg_id: &str, opts: &ChromaOptions) -> Result<SetChromaResult> {
    let (kind, _idx) = find_segment_info(draft, seg_id).ok_or_else(|| anyhow::anyhow!("Segment not found: {seg_id}"))?;
    if kind.kind != "video" {
        anyhow::bail!("Chroma key can only be applied to video segments (segment {seg_id} is on a {} track)", kind.kind);
    }
    if parse_hex(&opts.color).is_none() {
        anyhow::bail!("Invalid color: {}. Expected #RRGGBB.", opts.color);
    }
    let intensity = clamp01(opts.intensity.unwrap_or(0.5));
    let shadow = clamp01(opts.shadow.unwrap_or(0.0));
    let mat_id = uuid::Uuid::new_v4().to_string();
    let mat = serde_json::json!({ "id": mat_id, "type": "chromas", "color": opts.color, "intensity": intensity, "shadow": shadow, "path": "" });
    draft.materials.chromas.push(mat);

    let (track, idx) = find_segment_mut(draft, seg_id).unwrap();
    let seg = &mut track.segments[idx];
    let arr = seg.extra.entry("extra_material_refs".to_string()).or_insert_with(|| serde_json::Value::Array(vec![]));
    if let Some(a) = arr.as_array_mut() {
        a.push(serde_json::Value::String(mat_id.clone()));
    } else {
        *arr = serde_json::Value::Array(vec![serde_json::Value::String(mat_id.clone())]);
    }
    Ok(SetChromaResult { ok: true, segment_id: seg_id.to_string(), material_id: mat_id, color: opts.color.clone(), intensity, shadow })
}

pub fn remove_chroma(draft: &mut crate::draft::Draft, seg_id: &str) -> Result<RemoveChromaResult> {
    let _ = find_segment_info(draft, seg_id).ok_or_else(|| anyhow::anyhow!("Segment not found: {seg_id}"))?;
    let chroma_ids: std::collections::HashSet<String> = draft.materials.chromas.iter().filter_map(|v| v.get("id").and_then(|x| x.as_str()).map(|s| s.to_string())).collect();
    let mut removed: Vec<String> = Vec::new();
    if let Some((track, idx)) = find_segment_mut(draft, seg_id) {
        if let Some(arr) = track.segments[idx].extra.get_mut("extra_material_refs").and_then(|v| v.as_array_mut()) {
            let mut keep = Vec::new();
            for val in arr.drain(..) {
                if let Some(s) = val.as_str() {
                    if chroma_ids.contains(s) { removed.push(s.to_string()); continue; }
                }
                keep.push(val);
            }
            *arr = keep;
        }
    }
    draft.materials.chromas.retain(|v| {
        if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
            !removed.contains(&id.to_string())
        } else { true }
    });
    let segment_id = seg_id.to_string();
    Ok(RemoveChromaResult { ok: true, segment_id, removed })
}
