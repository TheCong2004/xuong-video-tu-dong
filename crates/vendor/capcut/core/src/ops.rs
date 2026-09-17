use crate::timeline::*;
use crate::error::{CoreError, Result};
use uuid::Uuid;

fn new_id() -> String { Uuid::new_v4().to_string() }

impl InternalTimeline {
    fn ensure_track(&mut self, kind: TrackKind) -> &mut Track {
        let idx = self.tracks.iter().position(|t| t.kind == kind);
        if let Some(i) = idx { return &mut self.tracks[i]; }
        let name = kind.as_str().to_string();
        self.tracks.push(Track { id: new_id(), kind: kind.clone(), name, segments: Vec::new(), muted: false });
        self.tracks.sort_by_key(|t| t.kind.rank());
        let i = self.tracks.iter().position(|t| t.kind == kind).unwrap();
        &mut self.tracks[i]
    }

    /// Ensure a named track of given kind exists; creates it if absent.
    /// Mirrors track creation in `reference/src/factory.ts` (`makeTrack`).
    pub fn ensure_track_named(&mut self, kind: TrackKind, name: impl Into<String>) -> &mut Track {
        let name_str = name.into();
        if let Some(i) = self.tracks.iter().position(|t| t.name == name_str && t.kind == kind) {
            return &mut self.tracks[i];
        }
        if let Some(i) = self.tracks.iter().position(|t| t.name == name_str) {
            let existing_kind = self.tracks[i].kind.clone();
            panic!("track name collision: \"{name_str}\" already exists as {}", existing_kind.as_str());
        }
        self.tracks.push(Track { id: new_id(), kind: kind.clone(), name: name_str.clone(), segments: Vec::new(), muted: false });
        self.tracks.sort_by_key(|t| t.kind.rank());
        let i = self.tracks.iter().position(|t| t.name == name_str).unwrap();
        &mut self.tracks[i]
    }

    /// Create a new track explicitly. Fails if name already exists.
    pub fn create_track(&mut self, kind: TrackKind, name: impl Into<String>) -> Result<String> {
        let name = name.into();
        if self.tracks.iter().any(|t| t.name == name) {
            return Err(CoreError::Conflict(format!("track \"{name}\" already exists")));
        }
        let id = new_id();
        self.tracks.push(Track { id: id.clone(), kind, name, segments: Vec::new(), muted: false });
        self.tracks.sort_by_key(|t| t.kind.rank());
        Ok(id)
    }

    /// Remove an empty track by name. Fails if track has segments or not found.
    pub fn remove_track(&mut self, name: &str) -> Result<Track> {
        let idx = self.tracks.iter().position(|t| t.name == name).ok_or_else(|| CoreError::NotFound(format!("track {name}")))?;
        if !self.tracks[idx].segments.is_empty() {
            return Err(CoreError::Conflict(format!("track \"{name}\" still has segments")));
        }
        Ok(self.tracks.remove(idx))
    }

    /// Rename a track.
    pub fn rename_track(&mut self, old: &str, new: &str) -> Result<()> {
        if self.tracks.iter().any(|t| t.name == new) {
            return Err(CoreError::Conflict(format!("track \"{new}\" already exists")));
        }
        let t = self.tracks.iter_mut().find(|t| t.name == old).ok_or_else(|| CoreError::NotFound(format!("track {old}")))?;
        t.name = new.to_string();
        Ok(())
    }

    pub fn add_video(&mut self, path: impl Into<String>, start: Us, duration: Us, width: u32, height: u32) -> (String, String) {
        let mat_id = new_id();
        let seg_id = new_id();
        self.materials.videos.push(VideoMaterial { id: mat_id.clone(), path: path.into(), duration, width, height, crop: None, extra: serde_json::Value::Null });
        let seg = Segment { id: seg_id.clone(), material_id: mat_id.clone(), timerange: Range::new(start, duration), source_timerange: Some(Range::new(0, duration)), speed: None, volume: None, opacity: None, extra: serde_json::Value::Null };
        self.ensure_track(TrackKind::Video).segments.push(seg);
        self.recalc_duration();
        (seg_id, mat_id)
    }

    pub fn add_audio(&mut self, path: impl Into<String>, start: Us, duration: Us) -> (String, String) {
        let mat_id = new_id();
        let seg_id = new_id();
        self.materials.audios.push(AudioMaterial { id: mat_id.clone(), path: path.into(), duration, extra: serde_json::Value::Null });
        let seg = Segment { id: seg_id.clone(), material_id: mat_id.clone(), timerange: Range::new(start, duration), source_timerange: Some(Range::new(0, duration)), speed: None, volume: None, opacity: None, extra: serde_json::Value::Null };
        self.ensure_track(TrackKind::Audio).segments.push(seg);
        self.recalc_duration();
        (seg_id, mat_id)
    }

    pub fn add_text(&mut self, content: impl Into<String>, start: Us, duration: Us, style: TextStyle) -> (String, String) {
        let mat_id = new_id();
        let seg_id = new_id();
        self.materials.texts.push(TextMaterial { id: mat_id.clone(), content: content.into(), style, style_ranges: Vec::new(), extra: serde_json::Value::Null });
        let seg = Segment { id: seg_id.clone(), material_id: mat_id.clone(), timerange: Range::new(start, duration), source_timerange: None, speed: None, volume: None, opacity: None, extra: serde_json::Value::Null };
        self.ensure_track(TrackKind::Text).segments.push(seg);
        self.recalc_duration();
        (seg_id, mat_id)
    }

    pub fn find_segment_mut(&mut self, id: &str) -> Option<(&mut Track, usize)> {
        for track in &mut self.tracks {
            if let Some(idx) = track.segments.iter().position(|s| s.id == id) {
                return Some((track, idx));
            }
        }
        None
    }

    /// Immutable segment lookup.
    pub fn find_segment(&self, id: &str) -> Option<(&Track, usize)> {
        for track in &self.tracks {
            if let Some(idx) = track.segments.iter().position(|s| s.id == id) {
                return Some((track, idx));
            }
        }
        None
    }

    pub fn trim_segment(&mut self, id: &str, new_range: Range) -> Result<()> {
        let (track, idx) = self.find_segment_mut(id).ok_or_else(|| CoreError::NotFound(format!("segment {id}")))?;
        track.segments[idx].timerange = new_range;
        self.recalc_duration();
        Ok(())
    }

    /// Set playback speed for a segment. Mirrors factory `speed` field.
    pub fn set_speed(&mut self, id: &str, speed: f64) -> Result<()> {
        if speed <= 0.0 || !speed.is_finite() { return Err(CoreError::InvalidArgument(format!("speed must be >0 finite, got {speed}"))); }
        let (track, idx) = self.find_segment_mut(id).ok_or_else(|| CoreError::NotFound(format!("segment {id}")))?;
        track.segments[idx].speed = Some(speed);
        Ok(())
    }

    /// Set volume (0..1) for a segment. Mirrors `volume` field.
    pub fn set_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&volume) { return Err(CoreError::InvalidArgument(format!("volume 0..1, got {volume}"))); }
        let (track, idx) = self.find_segment_mut(id).ok_or_else(|| CoreError::NotFound(format!("segment {id}")))?;
        track.segments[idx].volume = Some(volume);
        Ok(())
    }

    /// Set opacity / alpha (0..1) for a segment. Mirrors `clip.alpha` in factory `baseSegment`.
    pub fn set_opacity(&mut self, id: &str, opacity: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&opacity) { return Err(CoreError::InvalidArgument(format!("opacity 0..1, got {opacity}"))); }
        let (track, idx) = self.find_segment_mut(id).ok_or_else(|| CoreError::NotFound(format!("segment {id}")))?;
        track.segments[idx].opacity = Some(opacity);
        Ok(())
    }

    /// Shift a segment in time by delta microseconds (positive = forward).
    pub fn shift_segment(&mut self, id: &str, delta: Us) -> Result<()> {
        {
            let (track, idx) = self.find_segment_mut(id).ok_or_else(|| CoreError::NotFound(format!("segment {id}")))?;
            let new_start = track.segments[idx].timerange.start + delta;
            if new_start < 0 { return Err(CoreError::InvalidArgument(format!("shift would make start negative: {new_start}"))); }
            track.segments[idx].timerange.start = new_start;
        }
        self.recalc_duration();
        Ok(())
    }

    /// Crop a video segment by setting its material's crop rect. Mirrors `setCrop`.
    pub fn set_crop(&mut self, segment_id: &str, rect: CropRect) -> Result<()> {
        // rect already validated via CropRect::new
        let mat_id = {
            let (track, idx) = self.find_segment_mut(segment_id).ok_or_else(|| CoreError::NotFound(format!("segment {segment_id}")))?;
            track.segments[idx].material_id.clone()
        };
        let mat = self.materials.videos.iter_mut().find(|m| m.id == mat_id)
            .ok_or_else(|| CoreError::InvalidArgument(format!("crop only applies to video/photo segments (segment {segment_id})")))?;
        mat.crop = Some(rect);
        Ok(())
    }

    /// Clear crop on a video segment (restore full frame).
    pub fn clear_crop(&mut self, segment_id: &str) -> Result<()> {
        let mat_id = {
            let (track, idx) = self.find_segment_mut(segment_id).ok_or_else(|| CoreError::NotFound(format!("segment {segment_id}")))?;
            track.segments[idx].material_id.clone()
        };
        let mat = self.materials.videos.iter_mut().find(|m| m.id == mat_id)
            .ok_or_else(|| CoreError::InvalidArgument(format!("crop only applies to video/photo segments (segment {segment_id})")))?;
        mat.crop = None;
        Ok(())
    }

    /// Cut a segment at `at` (absolute timeline time, microseconds) into two segments.
    /// Returns the new segment id. Mirrors split logic around `cutProject` source_timerange handling.
    pub fn cut_segment(&mut self, id: &str, at: Us) -> Result<String> {
        let (track_idx, seg_idx) = {
            let mut found = None;
            for (ti, t) in self.tracks.iter().enumerate() {
                if let Some(si) = t.segments.iter().position(|s| s.id == id) { found = Some((ti, si)); break; }
            }
            found.ok_or_else(|| CoreError::NotFound(format!("segment {id}")))?
        };
        let seg = self.tracks[track_idx].segments[seg_idx].clone();
        if at <= seg.timerange.start || at >= seg.timerange.end() {
            return Err(CoreError::InvalidArgument(format!("cut point {at} outside segment {}..{}", seg.timerange.start, seg.timerange.end())));
        }
        let left_dur = at - seg.timerange.start;
        let right_dur = seg.timerange.duration - left_dur;
        let speed = seg.speed.unwrap_or(1.0);
        // left keeps original source start, duration scaled by speed
        let mut left = seg.clone();
        left.timerange.duration = left_dur;
        if let Some(ref mut sr) = left.source_timerange { sr.duration = (left_dur as f64 * speed).round() as i64; }
        // right is a new segment
        let new_id = new_id();
        let mut right = seg;
        right.id = new_id.clone();
        right.timerange.start = at;
        right.timerange.duration = right_dur;
        if let Some(ref mut sr) = right.source_timerange {
            sr.start += (left_dur as f64 * speed).round() as i64;
            sr.duration = (right_dur as f64 * speed).round() as i64;
        }
        self.tracks[track_idx].segments[seg_idx] = left;
        self.tracks[track_idx].segments.insert(seg_idx + 1, right);
        self.recalc_duration();
        Ok(new_id)
    }

    /// Extract time range [start, end) — drop segments outside, clip intersecting, rebase to 0.
    /// Mirrors `cutProject` in `reference/src/factory.ts`.
    pub fn cut_project(&mut self, start: Us, end: Us) -> Result<(usize, usize)> {
        if end <= start { return Err(CoreError::InvalidArgument(format!("cut end must be > start: {start}..{end}"))); }
        let duration = end - start;
        let mut kept = 0usize;
        let mut removed = 0usize;
        let mut surviving_mat_ids = std::collections::HashSet::new();
        for track in &mut self.tracks {
            let mut surviving = Vec::new();
            for seg in &mut track.segments {
                let s = seg.timerange.start;
                let e = seg.timerange.end();
                if e <= start || s >= end { removed += 1; continue; }
                let clipped_start = s.max(start);
                let clipped_end = e.min(end);
                let trim = clipped_start - s;
                let new_dur = clipped_end - clipped_start;
                if let Some(ref mut sr) = seg.source_timerange {
                    let speed = seg.speed.unwrap_or(1.0);
                    sr.start += (trim as f64 * speed).round() as i64;
                    sr.duration = (new_dur as f64 * speed).round() as i64;
                }
                seg.timerange.start = clipped_start - start;
                seg.timerange.duration = new_dur;
                surviving_mat_ids.insert(seg.material_id.clone());
                surviving.push(seg.clone());
                kept += 1;
            }
            track.segments = surviving;
        }
        self.tracks.retain(|t| !t.segments.is_empty());
        // GC orphan materials
        self.materials.videos.retain(|m| surviving_mat_ids.contains(&m.id));
        self.materials.audios.retain(|m| surviving_mat_ids.contains(&m.id));
        self.materials.texts.retain(|m| surviving_mat_ids.contains(&m.id));
        self.duration = duration;
        Ok((kept, removed))
    }

    /// Duplicate a segment at same position onto a new track above source, or onto named track.
    /// Mirrors `duplicateSegment` in `reference/src/factory.ts`.
    pub fn duplicate_segment(&mut self, seg_id: &str, target_track: Option<&str>) -> Result<String> {
        let (src_track_idx, seg_idx) = {
            let mut found = None;
            for (ti, t) in self.tracks.iter().enumerate() {
                if let Some(si) = t.segments.iter().position(|s| s.id == seg_id) { found = Some((ti, si)); break; }
            }
            found.ok_or_else(|| CoreError::NotFound(format!("segment {seg_id}")))?
        };
        let src_seg = self.tracks[src_track_idx].segments[seg_idx].clone();
        let src_track_kind = self.tracks[src_track_idx].kind.clone();
        let src_track_name = self.tracks[src_track_idx].name.clone();

        // Resolve target track
        let target_idx = if let Some(name) = target_track {
            let idx = self.tracks.iter().position(|t| t.name == name).ok_or_else(|| CoreError::NotFound(format!("track {name}")))?;
            if self.tracks[idx].kind != src_track_kind {
                return Err(CoreError::InvalidArgument(format!("track \"{name}\" is {} track; copy needs {}", self.tracks[idx].kind.as_str(), src_track_kind.as_str())));
            }
            // overlap check
            let s = src_seg.timerange.start;
            let e = src_seg.timerange.end();
            if self.tracks[idx].segments.iter().any(|x| x.timerange.start < e && x.timerange.end() > s) {
                return Err(CoreError::Conflict(format!("track \"{name}\" occupied over {s}..{e}")));
            }
            idx
        } else {
            // create new track after source
            let base = format!("{src_track_name}-copy");
            let mut name = base.clone();
            let names: std::collections::HashSet<_> = self.tracks.iter().map(|t| t.name.as_str()).collect();
            let mut n = 2;
            while names.contains(name.as_str()) { name = format!("{base}-{n}"); n += 1; }
            let id = new_id();
            self.tracks.insert(src_track_idx + 1, Track { id, kind: src_track_kind.clone(), name, segments: Vec::new(), muted: false });
            src_track_idx + 1
        };

        // Clone material with new id (per-segment state)
        let new_seg_id = new_id();
        let mut new_seg = src_seg.clone();
        new_seg.id = new_seg_id.clone();
        let (old_mat_id, new_mat_id) = (src_seg.material_id.clone(), new_id());
        new_seg.material_id = new_mat_id.clone();

        match src_track_kind {
            TrackKind::Video => {
                if let Some(m) = self.materials.videos.iter().find(|m| m.id == old_mat_id).cloned() {
                    let mut c = m; c.id = new_mat_id; self.materials.videos.push(c);
                }
            }
            TrackKind::Audio => {
                if let Some(m) = self.materials.audios.iter().find(|m| m.id == old_mat_id).cloned() {
                    let mut c = m; c.id = new_mat_id; self.materials.audios.push(c);
                }
            }
            TrackKind::Text => {
                if let Some(m) = self.materials.texts.iter().find(|m| m.id == old_mat_id).cloned() {
                    let mut c = m; c.id = new_mat_id; self.materials.texts.push(c);
                }
            }
            _ => {
                // effect/sticker: try each
                if let Some(m) = self.materials.videos.iter().find(|m| m.id == old_mat_id).cloned() { let mut c = m; c.id = new_mat_id.clone(); self.materials.videos.push(c); }
                else if let Some(m) = self.materials.audios.iter().find(|m| m.id == old_mat_id).cloned() { let mut c = m; c.id = new_mat_id.clone(); self.materials.audios.push(c); }
                else if let Some(m) = self.materials.texts.iter().find(|m| m.id == old_mat_id).cloned() { let mut c = m; c.id = new_mat_id.clone(); self.materials.texts.push(c); }
            }
        }

        self.tracks[target_idx].segments.push(new_seg);
        self.recalc_duration();
        Ok(new_seg_id)
    }

    pub fn remove_segment(&mut self, id: &str) -> Result<Segment> {
        for track in &mut self.tracks {
            if let Some(idx) = track.segments.iter().position(|s| s.id == id) {
                let seg = track.segments.remove(idx);
                self.recalc_duration();
                return Ok(seg);
            }
        }
        Err(CoreError::NotFound(format!("segment {id}")))
    }

    /// Remove segment and GC orphan materials similarly to `pruneOrphanMaterials`.
    pub fn remove_segment_gc(&mut self, id: &str) -> Result<(Segment, usize)> {
        let seg = self.remove_segment(id)?;
        // drop empty tracks
        self.tracks.retain(|t| !t.segments.is_empty());
        let referenced: std::collections::HashSet<_> = self.tracks.iter().flat_map(|t| t.segments.iter().map(|s| s.material_id.as_str())).collect();
        let mut removed = 0;
        let before_v = self.materials.videos.len();
        self.materials.videos.retain(|m| referenced.contains(m.id.as_str()));
        removed += before_v - self.materials.videos.len();
        let before_a = self.materials.audios.len();
        self.materials.audios.retain(|m| referenced.contains(m.id.as_str()));
        removed += before_a - self.materials.audios.len();
        let before_t = self.materials.texts.len();
        self.materials.texts.retain(|m| referenced.contains(m.id.as_str()));
        removed += before_t - self.materials.texts.len();
        Ok((seg, removed))
    }

    pub fn set_text_content(&mut self, material_id: &str, content: impl Into<String>) -> Result<()> {
        let m = self.materials.texts.iter_mut().find(|m| m.id == material_id)
            .ok_or_else(|| CoreError::NotFound(format!("text material {material_id}")))?;
        m.content = content.into();
        // keep first style range covering full text
        if !m.style_ranges.is_empty() {
            let len = m.content.encode_utf16().count() as i64;
            m.style_ranges[0].start = 0;
            m.style_ranges[0].end = len;
        }
        Ok(())
    }

    /// Set text content with explicit style ranges (offsets in UTF-16 code units).
    /// Mirrors rich-text `content` JSON with `styles[].range` handling + offset helpers.
    pub fn set_text_with_ranges(&mut self, material_id: &str, content: impl Into<String>, ranges: Vec<StyleRange>) -> Result<()> {
        let content = content.into();
        let n = content.encode_utf16().count() as i64;
        for r in &ranges {
            if r.start < 0 || r.end < 0 || r.start > r.end || r.end > n {
                return Err(CoreError::InvalidArgument(format!("style range [{},{}] out of bounds for text len {n}", r.start, r.end)));
            }
        }
        let m = self.materials.texts.iter_mut().find(|m| m.id == material_id)
            .ok_or_else(|| CoreError::NotFound(format!("text material {material_id}")))?;
        m.content = content;
        m.style_ranges = ranges;
        Ok(())
    }

    /// Update style on a text material. Mirrors `STYLE_FIELDS` handling.
    pub fn set_text_style(&mut self, material_id: &str, style: TextStyle) -> Result<()> {
        let m = self.materials.texts.iter_mut().find(|m| m.id == material_id)
            .ok_or_else(|| CoreError::NotFound(format!("text material {material_id}")))?;
        m.style = style;
        Ok(())
    }

    /// Set individual style fields without replacing whole style.
    pub fn patch_text_style(&mut self, material_id: &str, font: Option<String>, size: Option<u32>, color: Option<String>) -> Result<()> {
        let m = self.materials.texts.iter_mut().find(|m| m.id == material_id)
            .ok_or_else(|| CoreError::NotFound(format!("text material {material_id}")))?;
        if let Some(v) = font { m.style.font = Some(v); }
        if let Some(v) = size { m.style.size = Some(v); }
        if let Some(v) = color { m.style.color = Some(v); }
        Ok(())
    }
}
