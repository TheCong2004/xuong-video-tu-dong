use serde::{Deserialize, Serialize};

/// Microsecond unit - matches CapCut draft timerange.
pub type Us = i64;

/// Inclusive start + duration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Us,
    pub duration: Us,
}

impl Range {
    pub fn new(start: Us, duration: Us) -> Self { Self { start, duration } }
    pub fn end(&self) -> Us { self.start + self.duration }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Video,
    Audio,
    Text,
    Sticker,
    Effect,
    Filter,
    #[serde(other)]
    Unknown,
}

impl TrackKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Text => "text",
            Self::Sticker => "sticker",
            Self::Effect => "effect",
            Self::Filter => "filter",
            Self::Unknown => "unknown",
        }
    }
    pub fn rank(&self) -> u8 {
        match self {
            Self::Video => 0,
            Self::Audio => 1,
            Self::Text => 2,
            Self::Sticker => 3,
            Self::Effect => 4,
            Self::Filter => 5,
            Self::Unknown => 99,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CropRect {
    /// Normalized x of top-left corner (0..1).
    pub x: f64,
    /// Normalized y of top-left corner (0..1).
    pub y: f64,
    /// Normalized width (0..1).
    pub w: f64,
    /// Normalized height (0..1).
    pub h: f64,
}

impl CropRect {
    /// Validate and normalize. Mirrors `setCrop` validation in `reference/src/factory.ts`.
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Result<Self, crate::error::CoreError> {
        const EPS: f64 = 1e-9;
        if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            return Err(crate::error::CoreError::InvalidArgument(format!("crop rect must be finite: x={x}, y={y}, w={w}, h={h}")));
        }
        if x < 0.0 || y < 0.0 {
            return Err(crate::error::CoreError::InvalidArgument(format!("crop x/y must be >=0: x={x}, y={y}")));
        }
        if w <= 0.0 || h <= 0.0 {
            return Err(crate::error::CoreError::InvalidArgument(format!("crop w/h must be >0: w={w}, h={h}")));
        }
        if x + w > 1.0 + EPS || y + h > 1.0 + EPS {
            return Err(crate::error::CoreError::InvalidArgument(format!("crop must stay in frame x+w<=1 y+h<=1: x+w={}, y+h={}", x + w, y + h)));
        }
        Ok(Self { x, y, w, h })
    }

    /// 8-corner struct CapCut stores on the video material (mirrors `setCrop` in factory.ts).
    pub fn to_corners(&self) -> serde_json::Value {
        let right = (self.x + self.w).min(1.0);
        let bottom = (self.y + self.h).min(1.0);
        serde_json::json!({
            "lower_left_x": self.x, "lower_left_y": bottom,
            "lower_right_x": right, "lower_right_y": bottom,
            "upper_left_x": self.x, "upper_left_y": self.y,
            "upper_right_x": right, "upper_right_y": self.y,
        })
    }

    /// Preset ratio centered maximal crop, mirroring `cropRectForRatio`.
    pub fn for_ratio(width: u32, height: u32, preset: &str) -> Self {
        if preset == "free" || width == 0 || height == 0 {
            return Self { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        }
        let ratios: std::collections::HashMap<&str, f64> = [
            ("16:9", 16.0/9.0), ("9:16", 9.0/16.0), ("4:3", 4.0/3.0),
            ("3:4", 3.0/4.0), ("1:1", 1.0), ("21:9", 21.0/9.0),
        ].into_iter().collect();
        let Some(&target) = ratios.get(preset) else {
            return Self { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        };
        let src = width as f64 / height as f64;
        if (src - target).abs() < 1e-9 {
            return Self { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        }
        if src > target {
            let cw = height as f64 * target / width as f64;
            let x = (1.0 - cw) / 2.0;
            Self { x, y: 0.0, w: cw, h: 1.0 }
        } else {
            let ch = width as f64 / target / height as f64;
            let y = (1.0 - ch) / 2.0;
            Self { x: 0.0, y, w: 1.0, h: ch }
        }
    }
}

/// Style range on a text material, mirroring `styles[].range` in draft_content.json.
/// Offsets are UTF-16 code units (JS `String.length`), see `capcut-capcut::text_offsets`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleRange {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    pub material_id: String,
    pub timerange: Range,
    pub source_timerange: Option<Range>,
    /// Speed factor (1.0 = normal). None = 1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    /// Opacity / alpha (0..1). None = 1.0 opaque. Mirrors `clip.alpha` in reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    /// Passthrough for CapCut extra fields (kept but not interpreted).
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    pub name: String,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoMaterial {
    pub id: String,
    pub path: String,
    pub duration: Us,
    pub width: u32,
    pub height: u32,
    /// Normalized crop rect (material-level, mirrors `setCrop` in factory.ts). None = full frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<CropRect>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioMaterial {
    pub id: String,
    pub path: String,
    pub duration: Us,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextMaterial {
    pub id: String,
    pub content: String,
    #[serde(default)]
    pub style: TextStyle,
    /// Per-range styles, offsets in UTF-16 code units (mirrors `styles[].range`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style_ranges: Vec<StyleRange>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Materials {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub videos: Vec<VideoMaterial>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audios: Vec<AudioMaterial>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<TextMaterial>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InternalTimeline {
    pub id: String,
    pub name: String,
    pub duration: Us,
    pub fps: f32,
    pub canvas: Canvas,
    pub tracks: Vec<Track>,
    pub materials: Materials,
}

impl InternalTimeline {
    pub fn new(name: impl Into<String>, canvas: Canvas, fps: f32) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            duration: 0,
            fps,
            canvas,
            tracks: Vec::new(),
            materials: Materials::default(),
        }
    }

    pub fn recalc_duration(&mut self) {
        let max = self.tracks.iter()
            .flat_map(|t| t.segments.iter().map(|s| s.timerange.end()))
            .max().unwrap_or(0);
        self.duration = max;
    }

    pub fn sort_tracks(&mut self) {
        self.tracks.sort_by_key(|t| t.kind.rank());
    }
}
