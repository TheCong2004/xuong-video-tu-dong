use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Probe result — mirrors `reference/src/probe.ts: MediaProbe`.
/// Legacy scalar fields (`duration_us: i64`, `width: u32`…) are kept for
/// backward compat with `apps/cli`; new optional refinements are additive.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeInfo {
    /// Container duration (longest stream) in microseconds. 0 when unknown.
    pub duration_us: i64,
    /// Video stream's own duration in microseconds, when present.
    pub video_duration_us: Option<i64>,
    /// Audio stream's own duration in microseconds, when present.
    pub audio_duration_us: Option<i64>,
    /// Display width after rotation correction. 0 when unknown / audio-only.
    pub width: u32,
    /// Display height after rotation correction. 0 when unknown / audio-only.
    pub height: u32,
    /// Display rotation snapped to 0/90/180/270.
    pub rotation: i32,
    /// Preferred fps (`avg_frame_rate` falling back to `r_frame_rate`). 30.0 fallback.
    pub fps: f32,
    pub avg_fps: Option<f32>,
    pub base_fps: Option<f32>,
    pub has_video: bool,
    pub has_audio: bool,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<u32>,
}

fn fraction(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64().filter(|x| x.is_finite() && *x > 0.0),
        serde_json::Value::String(s) => {
            if s.contains('/') {
                let (a, b) = s.split_once('/')?;
                let an: f64 = a.trim().parse().ok()?;
                let bn: f64 = b.trim().parse().ok()?;
                if bn == 0.0 { return None; }
                let r = an / bn;
                if r.is_finite() && r > 0.0 { Some(r) } else { None }
            } else {
                let r: f64 = s.trim().parse().ok()?;
                if r.is_finite() && r > 0.0 { Some(r) } else { None }
            }
        }
        _ => None,
    }
}

pub fn normalize_rotation(rotation: f64) -> i32 {
    let r = ((rotation.round() as i64 % 360) + 360) % 360;
    ((r as f64 / 90.0).round() as i64 * 90 % 360) as i32
}

fn parse_rotation(video: &serde_json::Value) -> i32 {
    if let Some(tags) = video.get("tags").and_then(|x| x.as_object()) {
        if let Some(v) = tags.get("rotate") {
            let n = if let Some(s) = v.as_str() { s.parse::<f64>().ok() } else { v.as_f64() };
            if let Some(n) = n { if n.is_finite() { return normalize_rotation(n); } }
            if let Some(n) = v.as_i64() { return normalize_rotation(n as f64); }
        }
    }
    if let Some(list) = video.get("side_data_list").and_then(|x| x.as_array()) {
        for item in list {
            if let Some(obj) = item.as_object() {
                if let Some(v) = obj.get("rotation") {
                    let n = if let Some(s) = v.as_str() { s.parse::<f64>().ok() } else { v.as_f64() };
                    if let Some(n) = n { if n.is_finite() { return normalize_rotation(n); } }
                    if let Some(n) = v.as_i64() { return normalize_rotation(n as f64); }
                }
            }
        }
    }
    0
}

/// Pure ffprobe JSON parser. Returns `None` when JSON is malformed or the
/// container has neither a video nor an audio stream.
pub fn parse_probe_json(json: &str) -> Option<ProbeInfo> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    parse_probe_value(&v)
}

pub fn parse_probe_value(v: &serde_json::Value) -> Option<ProbeInfo> {
    let streams = v.get("streams").and_then(|x| x.as_array())?;
    let video = streams.iter().find(|s| s.get("codec_type").and_then(|x| x.as_str()) == Some("video"));
    let audio = streams.iter().find(|s| s.get("codec_type").and_then(|x| x.as_str()) == Some("audio"));
    if video.is_none() && audio.is_none() {
        return None;
    }

    let rotation = video.map(parse_rotation).unwrap_or(0);

    let mut width = video.and_then(|x| x.get("width")).and_then(|x| x.as_u64()).and_then(|n| if n > 0 { Some(n as u32) } else { None });
    let mut height = video.and_then(|x| x.get("height")).and_then(|x| x.as_u64()).and_then(|n| if n > 0 { Some(n as u32) } else { None });
    if (rotation == 90 || rotation == 270) && width.is_some() && height.is_some() {
        std::mem::swap(&mut width, &mut height);
    }

    let format = v.get("format");
    let dur_format = format.and_then(|f| f.get("duration")).and_then(fraction);
    let dur_video = video.and_then(|x| x.get("duration")).and_then(fraction);
    let dur_audio = audio.and_then(|x| x.get("duration")).and_then(fraction);
    let duration_seconds = dur_format.or(dur_video).or(dur_audio);
    let video_duration_seconds = dur_video;

    let avg = video.and_then(|x| x.get("avg_frame_rate")).and_then(fraction);
    let base = video.and_then(|x| x.get("r_frame_rate")).and_then(fraction);
    let fps_opt = avg.or(base);

    let duration_us = duration_seconds.map(|d| (d * 1_000_000.0).round() as i64).unwrap_or(0);
    let video_duration_us = video_duration_seconds.map(|d| (d * 1_000_000.0).round() as i64);
    let audio_duration_us = dur_audio.map(|d| (d * 1_000_000.0).round() as i64);

    let video_codec = video.and_then(|x| x.get("codec_name")).and_then(|x| x.as_str()).map(|s| s.to_string());
    let audio_codec = audio.and_then(|x| x.get("codec_name")).and_then(|x| x.as_str()).map(|s| s.to_string());
    let audio_channels = audio.and_then(|x| x.get("channels")).and_then(|x| x.as_u64()).and_then(|n| if n > 0 { Some(n as u32) } else { None });

    let has_video = video.is_some();
    let has_audio = audio.is_some();

    Some(ProbeInfo {
        duration_us,
        video_duration_us,
        audio_duration_us,
        width: width.unwrap_or(0),
        height: height.unwrap_or(0),
        rotation,
        fps: fps_opt.map(|x| x as f32).unwrap_or(30.0),
        avg_fps: avg.map(|x| x as f32),
        base_fps: base.map(|x| x as f32),
        has_video,
        has_audio,
        video_codec,
        audio_codec,
        audio_channels,
    })
}

pub fn probe(path: &Path) -> Result<ProbeInfo> {
    probe_with_cmd(path, "ffprobe")
}

pub fn probe_with_cmd(path: &Path, ffprobe_cmd: &str) -> Result<ProbeInfo> {
    let out = Command::new(ffprobe_cmd)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
            &path.to_string_lossy(),
        ])
        .output()
        .context("run ffprobe")?;
    if !out.status.success() {
        anyhow::bail!("ffprobe failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let json = String::from_utf8_lossy(&out.stdout);
    parse_probe_json(&json).context("parse ffprobe json: no video/audio streams")
}

pub fn ffprobe_available(ffprobe_cmd: &str) -> bool {
    Command::new(ffprobe_cmd)
        .args(["-version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_width_height_unrotated() {
        let j = r#"{"streams":[{"codec_type":"video","width":1920,"height":1080}]}"#;
        let p = parse_probe_json(j).unwrap();
        assert_eq!(p.width, 1920);
        assert_eq!(p.height, 1080);
        assert_eq!(p.rotation, 0);
    }

    #[test]
    fn parse_rotation_swaps() {
        let j = r#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"tags":{"rotate":"90"}}]}"#;
        let p = parse_probe_json(j).unwrap();
        assert_eq!(p.width, 1080);
        assert_eq!(p.height, 1920);
        assert_eq!(p.rotation, 90);
    }

    #[test]
    fn parse_null_when_no_streams() {
        let j = r#"{"streams":[{"codec_type":"subtitle"}]}"#;
        assert!(parse_probe_json(j).is_none());
    }

    #[test]
    fn parse_malformed_null() {
        assert!(parse_probe_json("not json").is_none());
    }

    #[test]
    fn parse_duration_and_fps() {
        let j = r#"{
            "streams":[
                {"codec_type":"video","codec_name":"h264","width":160,"height":120,"duration":"6.000000","avg_frame_rate":"30/1","r_frame_rate":"30/1"},
                {"codec_type":"audio","codec_name":"aac","duration":"6.0","channels":2}
            ],
            "format":{"duration":"6.000000"}
        }"#;
        let p = parse_probe_json(j).unwrap();
        assert_eq!(p.duration_us, 6_000_000);
        assert_eq!(p.video_duration_us, Some(6_000_000));
        assert_eq!(p.fps, 30.0);
        assert_eq!(p.avg_fps, Some(30.0));
        assert_eq!(p.base_fps, Some(30.0));
        assert!(p.has_video);
        assert!(p.has_audio);
        assert_eq!(p.video_codec.as_deref(), Some("h264"));
        assert_eq!(p.audio_channels, Some(2));
    }

    #[test]
    fn parse_video_duration_separate() {
        let j = r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":160,"height":120,"duration":"6.000000"}],"format":{"duration":"10.0"}}"#;
        let p = parse_probe_json(j).unwrap();
        assert_eq!(p.duration_us, 10_000_000);
        assert_eq!(p.video_duration_us, Some(6_000_000));
    }
}
