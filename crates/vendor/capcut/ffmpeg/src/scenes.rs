use std::path::Path;
use std::process::Command;

const US: f64 = 1_000_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct SceneCut {
    pub time: f64,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSegment {
    pub start: f64,
    pub end: Option<f64>,
    pub duration: Option<f64>,
    pub start_us: i64,
    pub end_us: Option<i64>,
    pub duration_us: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct SceneDetectOptions {
    pub threshold: f64,
    pub min_gap: f64,
    pub limit: Option<usize>,
    pub ffmpeg_cmd: String,
    pub ffprobe_cmd: String,
}

impl Default for SceneDetectOptions {
    fn default() -> Self {
        Self {
            threshold: 0.4,
            min_gap: 2.0,
            limit: None,
            ffmpeg_cmd: "ffmpeg".to_string(),
            ffprobe_cmd: "ffprobe".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SceneCutOutput {
    pub time: f64,
    pub time_us: i64,
    pub timecode: String,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct SceneReport {
    pub video: String,
    pub threshold: f64,
    pub min_gap: f64,
    pub limit: Option<usize>,
    pub duration: Option<f64>,
    pub duration_us: Option<i64>,
    /// "video-stream" | "container" | None
    pub duration_source: Option<String>,
    pub cuts: Vec<SceneCutOutput>,
    pub segments: Vec<SceneSegment>,
}

/// Parse the stderr of `-vf select='gt(scene,T)',metadata=print -f null -`.
/// Each selected frame emits two lines:
///   [Parsed_metadata_1 @ 0x...] frame:0  pts:15360  pts_time:1.024
///   [Parsed_metadata_1 @ 0x...] lavfi.scene_score=0.400000
pub fn parse_scene_cuts(stderr: &str) -> Vec<SceneCut> {
    let mut cuts = Vec::new();
    let mut pending: Option<f64> = None;
    for line in stderr.split('\n') {
        if let Some(t) = extract_pts_time(line) {
            pending = Some(t);
            continue;
        }
        if let Some(score) = extract_scene_score(line) {
            if let Some(time) = pending.take() {
                if time > 0.0 {
                    cuts.push(SceneCut { time, score });
                }
            }
        }
    }
    cuts.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    cuts
}

fn extract_pts_time(line: &str) -> Option<f64> {
    let idx = line.find("pts_time:")?;
    let rest = &line[idx + "pts_time:".len()..];
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        if c == '-' && i == 0 {
            end = 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' || (c == '-' && i == 0) {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    rest[..end].parse::<f64>().ok()
}

fn extract_scene_score(line: &str) -> Option<f64> {
    let idx = line.find("lavfi.scene_score=")?;
    let rest = &line[idx + "lavfi.scene_score=".len()..];
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        if c.is_ascii_digit() || c == '.' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    rest[..end].parse::<f64>().ok()
}

/// Pull the input duration off ffmpeg's stderr header ("Duration: 00:00:03.00, ...").
pub fn parse_ffmpeg_duration(stderr: &str) -> Option<f64> {
    // Search for "Duration: HH:MM:SS.ms"
    let idx = stderr.find("Duration:")?;
    let rest = &stderr[idx + "Duration:".len()..].trim_start().to_string();
    // rest like "00:00:03.00, start: ..."
    let rest = rest.as_str();
    let mut parts = rest.split(':');
    let h_str = parts.next()?.trim();
    let m_str = parts.next()?.trim();
    let s_rest = parts.next()?.trim();
    // s_rest is like "03.00, start: ..."
    let s_str: String = s_rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    if s_str.is_empty() {
        return None;
    }
    let h: f64 = h_str.parse().ok()?;
    let m: f64 = m_str.parse().ok()?;
    let s: f64 = s_str.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

/// Merge cuts closer than `min_gap` seconds.
pub fn merge_close_cuts(cuts: &[SceneCut], min_gap: f64) -> Vec<SceneCut> {
    if min_gap <= 0.0 || cuts.len() < 2 {
        let mut v = cuts.to_vec();
        v.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
        return v;
    }
    let mut sorted = cuts.to_vec();
    sorted.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    let mut merged = Vec::new();
    let mut best = sorted[0].clone();
    let mut prev_time = sorted[0].time;
    for cut in sorted.into_iter().skip(1) {
        if cut.time - prev_time < min_gap {
            if cut.score > best.score {
                best = cut.clone();
            }
        } else {
            merged.push(best);
            best = cut.clone();
        }
        prev_time = cut.time;
    }
    merged.push(best);
    merged
}

/// Keep the N strongest cuts (earliest wins ties), returned in time order.
pub fn limit_cuts(cuts: &[SceneCut], limit: Option<usize>) -> Vec<SceneCut> {
    let Some(n) = limit else { return cuts.to_vec(); };
    if cuts.len() <= n {
        return cuts.to_vec();
    }
    let mut sorted = cuts.to_vec();
    sorted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.time.partial_cmp(&b.time).unwrap())
    });
    sorted.truncate(n);
    sorted.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    sorted
}

/// Turn cut points into contiguous segments [0..cut1][cut1..cut2][..duration].
pub fn build_scene_segments(cuts: &[SceneCut], duration: Option<f64>) -> Vec<SceneSegment> {
    let bounded: Vec<&SceneCut> = match duration {
        None => cuts.iter().collect(),
        Some(d) => cuts.iter().filter(|c| c.time < d).collect(),
    };
    let mut starts = Vec::with_capacity(bounded.len() + 1);
    starts.push(0.0);
    for c in &bounded {
        starts.push(c.time);
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = if i + 1 < starts.len() {
                Some(starts[i + 1])
            } else {
                duration
            };
            let duration_val = end.map(|e| round6(e - start));
            let start_us = (start * US).round() as i64;
            let end_us = end.map(|e| (e * US).round() as i64);
            let duration_us = match (start_us, end_us) {
                (_, Some(eu)) => Some(eu - start_us),
                _ => None,
            };
            SceneSegment {
                start,
                end,
                duration: duration_val,
                start_us,
                end_us,
                duration_us,
            }
        })
        .collect()
}

/// Format seconds as hh:mm:ss.mmm.
pub fn timecode(seconds: f64) -> String {
    let total_ms = (seconds * 1000.0).round() as i64;
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let s_ms = total_ms % 60_000;
    let s = s_ms as f64 / 1000.0;
    format!("{h:02}:{m:02}:{s:06.3}")
}

fn round6(n: f64) -> f64 {
    (n * 1_000_000.0).round() / 1_000_000.0
}

/// Live detection. Runs ffmpeg's scene filter over `video_path`.
pub fn detect_scenes(video_path: &Path, opts: &SceneDetectOptions) -> anyhow::Result<SceneReport> {
    if !video_path.exists() {
        anyhow::bail!("detect-scenes: video not found: {}", video_path.display());
    }
    let args = [
        "-hide_banner",
        "-i",
        &video_path.to_string_lossy().to_string(),
        "-vf",
        &format!("select='gt(scene,{})',metadata=print", opts.threshold),
        "-an",
        "-f",
        "null",
        "-",
    ];
    let output = Command::new(&opts.ffmpeg_cmd)
        .args(&args)
        .output()
        .map_err(|e| anyhow::anyhow!("detect-scenes: ffmpeg is unavailable at '{}': {e}", opts.ffmpeg_cmd))?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        anyhow::bail!("detect-scenes: ffmpeg failed on {}.\n{}", video_path.display(), &stderr[stderr.len().saturating_sub(600)..]);
    }

    // Duration: try probe first, fallback to ffmpeg header
    let probe_result = crate::probe::probe(video_path).ok();
    let (duration, duration_source) = if let Some(ref p) = probe_result {
        if let Some(vd) = p.video_duration_us {
            if vd > 0 {
                (Some(vd as f64 / US), Some("video-stream".to_string()))
            } else if p.duration_us > 0 {
                (Some(p.duration_us as f64 / US), Some("container".to_string()))
            } else {
                let d = parse_ffmpeg_duration(&stderr);
                (d, d.map(|_| "container".to_string()))
            }
        } else if p.duration_us > 0 {
            (Some(p.duration_us as f64 / US), Some("container".to_string()))
        } else {
            let d = parse_ffmpeg_duration(&stderr);
            (d, d.map(|_| "container".to_string()))
        }
    } else {
        let d = parse_ffmpeg_duration(&stderr);
        (d, d.map(|_| "container".to_string()))
    };


    let detected: Vec<SceneCut> = parse_scene_cuts(&stderr)
        .into_iter()
        .filter(|c| duration.is_none() || c.time < duration.unwrap())
        .collect();
    let merged = merge_close_cuts(&detected, opts.min_gap);
    let cuts = limit_cuts(&merged, opts.limit);

    let cuts_out: Vec<SceneCutOutput> = cuts
        .iter()
        .map(|c| SceneCutOutput {
            time: c.time,
            time_us: (c.time * US).round() as i64,
            timecode: timecode(c.time),
            score: c.score,
        })
        .collect();
    let segments = build_scene_segments(&cuts, duration);

    Ok(SceneReport {
        video: video_path.to_string_lossy().to_string(),
        threshold: opts.threshold,
        min_gap: opts.min_gap,
        limit: opts.limit,
        duration,
        duration_us: duration.map(|d| (d * US).round() as i64),
        duration_source,
        cuts: cuts_out,
        segments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scene_cuts_basic() {
        let stderr = "\
[Parsed_metadata_1 @ 0xabc] frame:0 pts:15360 pts_time:1.024\n\
[Parsed_metadata_1 @ 0xabc] lavfi.scene_score=0.400000\n\
[Parsed_metadata_1 @ 0xabc] frame:1 pts:30720 pts_time:2.048\n\
[Parsed_metadata_1 @ 0xabc] lavfi.scene_score=0.900000\n\
[Parsed_metadata_1 @ 0xabc] frame:2 pts:0 pts_time:0\n\
[Parsed_metadata_1 @ 0xabc] lavfi.scene_score=0.999\n";
        let cuts = parse_scene_cuts(stderr);
        assert_eq!(cuts.len(), 2);
        assert!((cuts[0].time - 1.024).abs() < 1e-9);
        assert!((cuts[0].score - 0.4).abs() < 1e-9);
        assert!((cuts[1].time - 2.048).abs() < 1e-9);
        // t=0 is ignored
    }

    #[test]
    fn parse_ffmpeg_duration_basic() {
        let s = "  Duration: 00:01:30.50, start: 0.000000, bitrate: 1000 kb/s";
        let d = parse_ffmpeg_duration(s).unwrap();
        assert!((d - 90.5).abs() < 1e-9);
    }

    #[test]
    fn merge_close_cuts_keeps_strongest() {
        let cuts = vec![
            SceneCut { time: 1.0, score: 0.5 },
            SceneCut { time: 1.5, score: 0.9 },
            SceneCut { time: 5.0, score: 0.6 },
        ];
        let merged = merge_close_cuts(&cuts, 2.0);
        assert_eq!(merged.len(), 2);
        assert!((merged[0].score - 0.9).abs() < 1e-9);
        assert!((merged[1].time - 5.0).abs() < 1e-9);
    }

    #[test]
    fn limit_cuts_keeps_strongest() {
        let cuts = vec![
            SceneCut { time: 1.0, score: 0.5 },
            SceneCut { time: 2.0, score: 0.9 },
            SceneCut { time: 3.0, score: 0.7 },
        ];
        let limited = limit_cuts(&cuts, Some(2));
        assert_eq!(limited.len(), 2);
        assert!((limited[0].time - 2.0).abs() < 1e-9);
        assert!((limited[1].time - 3.0).abs() < 1e-9);
    }

    #[test]
    fn build_segments_open_ended() {
        let cuts = vec![SceneCut { time: 1.0, score: 0.5 }];
        let segs = build_scene_segments(&cuts, None);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[1].end, None);
        assert_eq!(segs[0].start_us, 0);
        assert_eq!(segs[1].start_us, 1_000_000);
    }

    #[test]
    fn timecode_rounding_carry() {
        assert_eq!(timecode(59.9996), "00:01:00.000");
        assert_eq!(timecode(3661.5), "01:01:01.500");
    }
}
