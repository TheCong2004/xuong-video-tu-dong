use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSfxOptions {
    pub slug: String,
    pub start: i64,
    pub duration: i64,
    pub track_name: Option<String>,
    pub namespace: Option<crate::enums::Namespace>,
    pub volume: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSfxResult {
    pub segment_id: String,
    pub material_id: String,
    pub track_id: String,
    pub name: String,
    pub slug: String,
}

pub fn add_sfx(draft: &mut crate::draft::Draft, opts: &AddSfxOptions) -> Result<AddSfxResult> {
    let ns = opts.namespace.unwrap_or(crate::enums::Namespace::CapCut);
    let hit = crate::enums::find_enum(crate::enums::Category::AudioEffects, &opts.slug, ns, None);
    let Some(hit) = hit else {
        let hint = if ns == crate::enums::Namespace::JianYing { " --jianying" } else { "" };
        anyhow::bail!("Unknown SFX slug: {}. Run 'capcut enums --audio-effects{hint}' for the full list.", opts.slug);
    };
    let name = hit.name.clone().unwrap_or_default();
    let effect_id = hit.effect_id.clone().unwrap_or_default();
    let resource_id = hit.resource_id.clone().unwrap_or_default();
    if name.is_empty() || effect_id.is_empty() || resource_id.is_empty() {
        anyhow::bail!("Unknown SFX slug: {}. Run 'capcut enums --audio-effects' for the full list.", opts.slug);
    }
    let seg_id = uuid::Uuid::new_v4().to_string();
    let mat_id = uuid::Uuid::new_v4().to_string();
    let track_name = opts.track_name.clone().unwrap_or_else(|| "sfx".to_string());

    let track_idx = draft.tracks.iter().position(|t| t.kind == "audio" && t.name == track_name);
    let track_id = if let Some(idx) = track_idx {
        draft.tracks[idx].id.clone()
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        let is_default = opts.track_name.is_none();
        let mut extra = serde_json::Map::new();
        if is_default {
            extra.insert("flag".to_string(), serde_json::Value::Number(0.into()));
        }
        draft.tracks.push(crate::draft::Track { id: id.clone(), kind: "audio".to_string(), name: track_name.clone(), segments: vec![], extra });
        id
    };

    let sfx_material = serde_json::json!({
        "id": mat_id,
        "name": name,
        "effect_id": effect_id,
        "resource_id": resource_id,
        "formula_id": "",
        "is_vip": hit.is_vip.unwrap_or(false),
        "md5": hit.md5.clone().unwrap_or_default(),
        "type": "sound_effect",
        "category_id": "",
        "category_name": "",
        "path": "",
        "platform": "all",
        "source_platform": 0,
        "version": "",
    });
    draft.materials.audio_effects.push(sfx_material);

    let seg = crate::draft::Segment {
        id: seg_id.clone(),
        material_id: mat_id.clone(),
        target_timerange: crate::draft::Timerange { start: opts.start, duration: opts.duration },
        source_timerange: Some(crate::draft::Timerange { start: 0, duration: opts.duration }),
        speed: Some(1.0),
        volume: Some(opts.volume.unwrap_or(1.0)),
        extra: {
            let mut m = serde_json::Map::new();
            m.insert("visible".to_string(), serde_json::Value::Bool(true));
            m.insert("clip".to_string(), serde_json::Value::Null);
            m.insert("extra_material_refs".to_string(), serde_json::Value::Array(vec![]));
            m.insert("render_index".to_string(), serde_json::Value::Number(0.into()));
            m
        },
    };
    if let Some(track) = draft.tracks.iter_mut().find(|t| t.id == track_id) {
        track.segments.push(seg);
    }
    Ok(AddSfxResult { segment_id: seg_id, material_id: mat_id, track_id, name, slug: opts.slug.clone() })
}
