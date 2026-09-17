use std::path::Path;
use std::process::Command;

const US: f64 = 1_000_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct SilenceSpan {
    pub start: f64,
    pub end: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: Option<f64>,
    pub duration: Option<f64>,
    pub start_us: i64,
    pub end_us: Option<i64>,
    pub duration_us: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct SilenceDetectOptions {
    pub threshold_db: f64,
    pub min_silence: f64,
    pub pad: f64,
    pub limit: Option<usize>,
    pub ffmpeg_cmd: String,
    pub ffprobe_cmd: String,
}

impl Default for SilenceDetectOptions {
    fn default() -> Self {
        Self {
            threshold_db: -30.0,
            min_silence: 0.5,
            pad: 0.1,
            limit: None,
            ffmpeg_cmd: "ffmpeg".to_string(),
            ffprobe_cmd: "ffprobe".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SilenceReport {
    pub media: String,
    pub threshold_db: f64,
    pub min_silence: f64,
    pub pad: f64,
    pub limit: Option<usize>,
    pub duration: Option<f64>,
    pub duration_us: Option<i64>,
    pub duration_source: Option<String>,
    pub silences: Vec<Segment>,
    pub keeps: Vec<Segment>,
}

/// Parse the stderr of `-af silencedetect=noise=TdB:d=D -f null -`.
pub fn parse_silence_spans(stderr: &str) -> Vec<SilenceSpan> {
    let mut spans = Vec::new();
    let mut pending: Option<f64> = None;
    for line in stderr.split('\n') {
        // handle \r as well
        let line = line.trim_end_matches('\r');
        if let Some(v) = extract_silence_start(line) {
            pending = Some(v.max(0.0));
            continue;
        }
        if let Some(v) = extract_silence_end(line) {
            if let Some(start) = pending.take() {
                if v > start {
                    spans.push(SilenceSpan { start, end: Some(v) });
                }
            }
        }
    }
    if let Some(start) = pending {
        spans.push(SilenceSpan { start, end: None });
    }
    spans.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    spans
}

fn extract_silence_start(line: &str) -> Option<f64> {
    let idx = line.find("silence_start:")?;
    let rest = &line[idx + "silence_start:".len()..].trim_start();
    let s: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if s.is_empty() || s == "-" {
        return None;
    }
    s.parse::<f64>().ok()
}

fn extract_silence_end(line: &str) -> Option<f64> {
    let idx = line.find("silence_end:")?;
    let rest = &line[idx + "silence_end:".len()..].trim_start();
    let s: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if s.is_empty() || s == "-" {
        return None;
    }
    s.parse::<f64>().ok()
}

pub fn close_silence_spans(spans: &[SilenceSpan], duration: Option<f64>) -> Vec<SilenceSpan> {
    let Some(d) = duration else {
        return spans.to_vec();
    };
    let mut closed = Vec::new();
    for s in spans {
        if s.start >= d {
            continue;
        }
        let end = match s.end {
            None => d,
            Some(e) => e.min(d),
        };
        if end > s.start {
            closed.push(SilenceSpan { start: s.start, end: Some(end) });
        }
    }
    closed
}

pub fn pad_silence_spans(spans: &[SilenceSpan], pad: f64) -> Vec<SilenceSpan> {
    if pad <= 0.0 {
        return spans.to_vec();
    }
    let mut out = Vec::new();
    for s in spans {
        let start = s.start + pad;
        match s.end {
            None => out.push(SilenceSpan { start, end: None }),
            Some(e) => {
                let end = e - pad;
                if end > start {
                    out.push(SilenceSpan { start, end: Some(end) });
                }
            }
        }
    }
    out
}

pub fn limit_silences(spans: &[SilenceSpan], limit: Option<usize>) -> Vec<SilenceSpan> {
    let Some(n) = limit else { return spans.to_vec(); };
    if spans.len() <= n {
        return spans.to_vec();
    }
    let len_of = |s: &SilenceSpan| match s.end {
        None => f64::INFINITY,
        Some(e) => e - s.start,
    };
    let mut sorted = spans.to_vec();
    sorted.sort_by(|a, b| {
        len_of(b)
            .partial_cmp(&len_of(a))
            .unwrap()
            .then_with(|| a.start.partial_cmp(&b.start).unwrap())
    });
    sorted.truncate(n);
    sorted.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    sorted
}

pub fn build_keep_spans(silences: &[SilenceSpan], duration: Option<f64>) -> Vec<SilenceSpan> {
    let mut keeps = Vec::new();
    let mut cursor = 0.0;
    for s in silences {
        if s.start > cursor {
            keeps.push(SilenceSpan { start: cursor, end: Some(s.start) });
        }
        match s.end {
            None => return keeps,
            Some(e) => cursor = cursor.max(e),
        }
    }
    match duration {
        None => keeps.push(SilenceSpan { start: cursor, end: None }),
        Some(d) => {
            if cursor < d {
                keeps.push(SilenceSpan { start: cursor, end: Some(d) });
            }
        }
    }
    keeps
}

pub fn spans_to_segments(spans: &[SilenceSpan]) -> Vec<Segment> {
    spans
        .iter()
        .map(|s| {
            let duration = s.end.map(|e| round6(e - s.start));
            let start_us = (s.start * US).round() as i64;
            let end_us = s.end.map(|e| (e * US).round() as i64);
            let duration_us = match end_us {
                Some(eu) => Some(eu - start_us),
                None => None,
            };
            Segment {
                start: s.start,
                end: s.end,
                duration,
                start_us,
                end_us,
                duration_us,
            }
        })
        .collect()
}

fn round6(n: f64) -> f64 {
    (n * 1_000_000.0).round() / 1_000_000.0
}

fn parse_ffmpeg_duration(stderr: &str) -> Option<f64> {
    let idx = stderr.find("Duration:")?;
    let rest = &stderr[idx + "Duration:".len()..].trim_start().to_string();
    let rest = rest.as_str();
    let mut parts = rest.split(':');
    let h_str = parts.next()?.trim();
    let m_str = parts.next()?.trim();
    let s_rest = parts.next()?.trim();
    let s_str: String = s_rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    if s_str.is_empty() {
        return None;
    }
    let h: f64 = h_str.parse().ok()?;
    let m: f64 = m_str.parse().ok()?;
    let s: f64 = s_str.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

pub fn detect_silence(media_path: &Path, opts: &SilenceDetectOptions) -> anyhow::Result<SilenceReport> {
    if !media_path.exists() {
        anyhow::bail!("detect-silence: media not found: {}", media_path.display());
    }
    let args = [
        "-hide_banner",
        "-i",
        &media_path.to_string_lossy().to_string(),
        "-af",
        &format!("silencedetect=noise={}dB:d={}", opts.threshold_db, opts.min_silence),
        "-vn",
        "-f",
        "null",
        "-",
    ];
    let output = Command::new(&opts.ffmpeg_cmd)
        .args(&args)
        .output()
        .map_err(|e| anyhow::anyhow!("detect-silence: ffmpeg is unavailable at '{}': {e}", opts.ffmpeg_cmd))?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        anyhow::bail!(
            "detect-silence: ffmpeg failed on {}.\n{}",
            media_path.display(),
            &stderr[stderr.len().saturating_sub(600)..]
        );
    }

    let probe_result = crate::probe::probe(media_path).ok();
    let (duration, duration_source) = if let Some(ref p) = probe_result {
        if p.duration_us > 0 {
            (Some(p.duration_us as f64 / US), Some("container".to_string()))
        } else {
            let d = parse_ffmpeg_duration(&stderr);
            (d, d.map(|_| "ffmpeg-header".to_string()))
        }
    } else {
        let d = parse_ffmpeg_duration(&stderr);
        (d, d.map(|_| "ffmpeg-header".to_string()))
    };

    let silences = limit_silences(
        &pad_silence_spans(&close_silence_spans(&parse_silence_spans(&stderr), duration), opts.pad),
        opts.limit,
    );

    let keeps = build_keep_spans(&silences, duration);

    Ok(SilenceReport {
        media: media_path.to_string_lossy().to_string(),
        threshold_db: opts.threshold_db,
        min_silence: opts.min_silence,
        pad: opts.pad,
        limit: opts.limit,
        duration,
        duration_us: duration.map(|d| (d * US).round() as i64),
        duration_source,
        silences: spans_to_segments(&silences),
        keeps: spans_to_segments(&keeps),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_silence_spans_basic() {
        let stderr = "\
[silencedetect @ 0xabc] silence_start: 1.33915\n\
[silencedetect @ 0xabc] silence_end: 3.55712 | silence_duration: 2.21798\n\
[silencedetect @ 0xabc] silence_start: 5.0\n";
        let spans = parse_silence_spans(stderr);
        assert_eq!(spans.len(), 2);
        assert!((spans[0].start - 1.33915).abs() < 1e-9);
        assert!((spans[0].end.unwrap() - 3.55712).abs() < 1e-9);
        assert_eq!(spans[1].end, None);
    }

    #[test]
    fn parse_negative_start_clamped() {
        let stderr = "[silencedetect @ 0x0] silence_start: -0.00119\n[silencedetect @ 0x0] silence_end: 0.5 | silence_duration: 0.5\n";
        let spans = parse_silence_spans(stderr);
        assert_eq!(spans[0].start, 0.0);
    }

    #[test]
    fn close_spans_open_ended() {
        let spans = vec![SilenceSpan { start: 2.0, end: None }];
        let closed = close_silence_spans(&spans, Some(10.0));
        assert_eq!(closed[0].end, Some(10.0));
    }

    #[test]
    fn pad_shrinks_spans() {
        let spans = vec![SilenceSpan { start: 1.0, end: Some(2.0) }];
        let padded = pad_silence_spans(&spans, 0.1);
        assert!((padded[0].start - 1.1).abs() < 1e-9);
        assert!((padded[0].end.unwrap() - 1.9).abs() < 1e-9);
    }

    #[test]
    fn build_keep_complement() {
        let silences = vec![SilenceSpan { start: 1.0, end: Some(2.0) }];
        let keeps = build_keep_spans(&silences, Some(5.0));
        assert_eq!(keeps.len(), 2);
        assert!((keeps[0].start - 0.0).abs() < 1e-9);
        assert!((keeps[0].end.unwrap() - 1.0).abs() < 1e-9);
        assert!((keeps[1].start - 2.0).abs() < 1e-9);
    }

    #[test]
    fn spans_to_segments_us() {
        let spans = vec![SilenceSpan { start: 1.0, end: Some(2.0) }];
        let segs = spans_to_segments(&spans);
        assert_eq!(segs[0].start_us, 1_000_000);
        assert_eq!(segs[0].end_us, Some(2_000_000));
        assert_eq!(segs[0].duration_us, Some(1_000_000));
    }
}
