use crate::draft::Draft;
use serde_json::Value;

const PHOTO_META_DURATION_US: i64 = 5_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialKind {
    Video,
    Photo,
    Music,
}

impl MaterialKind {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "video" => Some(Self::Video),
            "photo" | "image" => Some(Self::Photo),
            "music" | "audio" => Some(Self::Music),
            _ => None,
        }
    }
}

fn is_local_path(p: &str) -> bool {
    !p.is_empty()
        && !p.to_lowercase().starts_with("http://")
        && !p.to_lowercase().starts_with("https://")
}

fn material_path(v: &Value) -> Option<&str> {
    v.get("path").and_then(|x| x.as_str())
}

/// Add a material entry to draft.materials with dedup by path.
/// Parity with materials-register.ts.
pub fn register_material(draft: &mut Draft, path: &str, kind: &str) -> String {
    let mk = MaterialKind::from_str(kind).unwrap_or(MaterialKind::Video);
    match &mk {
        MaterialKind::Video | MaterialKind::Photo => {
            for v in &draft.materials.videos {
                if material_path(v) == Some(path) {
                    if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                        return id.to_string();
                    }
                }
            }
        }
        MaterialKind::Music => {
            for v in &draft.materials.audios {
                if material_path(v) == Some(path) {
                    if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                        return id.to_string();
                    }
                }
            }
        }
    }
    let mat_id = uuid::Uuid::new_v4().to_string();
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|x| x.to_str())
        .unwrap_or(path)
        .to_string();
    match mk {
        MaterialKind::Video => {
            let m = serde_json::json!({
                "id": mat_id,
                "type": "video",
                "path": path,
                "duration": PHOTO_META_DURATION_US,
                "width": 1920,
                "height": 1080,
                "material_name": file_name,
                "local_material_id": mat_id,
            });
            let ret = m.get("id").and_then(|x| x.as_str()).unwrap().to_string();
            draft.materials.videos.push(m);
            ret
        }
        MaterialKind::Photo => {
            let m = serde_json::json!({
                "id": mat_id,
                "type": "photo",
                "path": path,
                "duration": PHOTO_META_DURATION_US,
                "width": 1920,
                "height": 1080,
                "material_name": file_name,
                "local_material_id": mat_id,
            });
            let ret = m.get("id").and_then(|x| x.as_str()).unwrap().to_string();
            draft.materials.videos.push(m);
            ret
        }
        MaterialKind::Music => {
            let m = serde_json::json!({
                "id": mat_id,
                "type": "music",
                "path": path,
                "duration": PHOTO_META_DURATION_US,
                "name": file_name,
                "local_material_id": mat_id,
            });
            let ret = m.get("id").and_then(|x| x.as_str()).unwrap().to_string();
            draft.materials.audios.push(m);
            ret
        }
    }
}

pub fn referenced_media(draft: &Draft) -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for v in &draft.materials.videos {
        if let Some(p) = material_path(v) {
            if is_local_path(p) && seen.insert(p.to_string()) {
                let kind = if v.get("type").and_then(|x| x.as_str()) == Some("photo") {
                    "photo"
                } else {
                    "video"
                };
                out.push((p.to_string(), kind.to_string()));
            }
        }
    }
    for v in &draft.materials.audios {
        if let Some(p) = material_path(v) {
            if is_local_path(p) && seen.insert(p.to_string()) {
                out.push((p.to_string(), "music".to_string()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn blank_draft() -> Draft {
        Draft {
            id: "1".into(),
            name: "t".into(),
            duration: 0,
            fps: 30.0,
            canvas_config: Value::Null,
            platform: Value::Null,
            tracks: vec![],
            materials: Default::default(),
            extra_info: Value::Null,
            extra: Default::default(),
        }
    }
    #[test]
    fn dedup_video() {
        let mut d = blank_draft();
        let id1 = register_material(&mut d, "/tmp/a.mp4", "video");
        let id2 = register_material(&mut d, "/tmp/a.mp4", "video");
        assert_eq!(id1, id2);
        assert_eq!(d.materials.videos.len(), 1);
    }
    #[test]
    fn dedup_audio() {
        let mut d = blank_draft();
        let id1 = register_material(&mut d, "/tmp/b.mp3", "music");
        let id2 = register_material(&mut d, "/tmp/b.mp3", "audio");
        assert_eq!(id1, id2);
    }
}
