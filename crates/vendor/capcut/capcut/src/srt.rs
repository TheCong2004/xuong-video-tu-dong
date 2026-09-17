use capcut_core::time::srt_time;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrtCue {
    pub index: usize,
    pub start_us: i64,
    pub end_us: i64,
    pub text: String,
}

#[derive(Debug, Error)]
pub enum SrtError {
    #[error("invalid SRT timestamp near line {line}: {content}")]
    InvalidTimestamp { line: usize, content: String },
    #[error("SRT cue {index} has end <= start")]
    InvalidRange { index: usize },
}

fn ts_to_us(h: &str, m: &str, s: &str, ms: &str) -> i64 {
    let ms_padded = format!("{ms}000");
    let ms3 = &ms_padded[..3];
    let h: i64 = h.parse().unwrap_or(0);
    let m: i64 = m.parse().unwrap_or(0);
    let s: i64 = s.parse().unwrap_or(0);
    let ms: i64 = ms3.parse().unwrap_or(0);
    ((h * 3600 + m * 60 + s) * 1000 + ms) * 1000
}

/// Parse SRT content. Supports '.' or ',' as ms separator, index line optional.
/// Mirrors reference/src/srt.ts parseSrt.
pub fn parse_srt(content: &str) -> Result<Vec<SrtCue>, SrtError> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut cues = Vec::new();
    let mut i = 0usize;
    let mut auto_idx: usize = 0;

    // Regex equivalent: (\d{1,2}):(\d{2}):(\d{2})[.,](\d{1,3})\s*-->\s*(\d{1,2}):(\d{2}):(\d{2})[.,](\d{1,3})
    // We parse manually to avoid regex dep.
    while i < lines.len() {
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        if i >= lines.len() {
            break;
        }
        let mut idx = auto_idx + 1;
        if lines[i].trim().chars().all(|c| c.is_ascii_digit()) && !lines[i].trim().is_empty() {
            if let Ok(n) = lines[i].trim().parse::<usize>() {
                idx = n;
            }
            i += 1;
        }
        if i >= lines.len() {
            break;
        }
        let ts_line = lines[i];
        let (start_us, end_us) = parse_timestamp_line(ts_line).ok_or_else(|| SrtError::InvalidTimestamp {
            line: i + 1,
            content: ts_line.to_string(),
        })?;
        i += 1;
        let mut text_lines = Vec::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            text_lines.push(lines[i]);
            i += 1;
        }
        if end_us <= start_us {
            return Err(SrtError::InvalidRange { index: idx });
        }
        cues.push(SrtCue {
            index: idx,
            start_us,
            end_us,
            text: text_lines.join("\n"),
        });
        auto_idx = idx;
    }
    Ok(cues)
}

fn parse_timestamp_line(line: &str) -> Option<(i64, i64)> {
    // Split by -->
    let arrow = line.find("-->")?;
    let left = line[..arrow].trim();
    let right = line[arrow + 3..].trim();
    let (sh, sm, ss, sms) = parse_ts_part(left)?;
    let (eh, em, es, ems) = parse_ts_part(right)?;
    Some((ts_to_us(&sh, &sm, &ss, &sms), ts_to_us(&eh, &em, &es, &ems)))
}

fn parse_ts_part(s: &str) -> Option<(String, String, String, String)> {
    // expect HH:MM:SS[.,]mmm  where HH 1-2 digits, MM 2, SS 2, ms 1-3
    let s = s.trim();
    let mut parts = s.split(':');
    let h = parts.next()?.to_string();
    let m = parts.next()?.to_string();
    let rest = parts.next()?;
    let dot = rest.find(|c| c == '.' || c == ',')?;
    let sec = rest[..dot].to_string();
    let ms = rest[dot + 1..].to_string();
    // Validate digit counts roughly
    if h.len() > 2 || m.len() != 2 || sec.len() != 2 || ms.is_empty() || ms.len() > 3 {
        // still allow 1-digit ms etc - be lenient like TS
        // but require digits
    }
    let extra = parts.next();
    if extra.is_some() {
        return None;
    }
    if !h.chars().all(|c| c.is_ascii_digit())
        || !m.chars().all(|c| c.is_ascii_digit())
        || !sec.chars().all(|c| c.is_ascii_digit())
        || !ms.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some((h, m, sec, ms))
}

/// Serialize cues to SRT format (index + timestamp + text).
pub fn serialize_srt(cues: &[SrtCue]) -> String {
    cues.iter()
        .enumerate()
        .map(|(i, c)| {
            let idx = c.index;
            // Use stored index if meaningful, else enumerate
            let display_idx = if idx == 0 { i + 1 } else { idx };
            format!(
                "{}\n{} --> {}\n{}\n",
                display_idx,
                srt_time(c.start_us),
                srt_time(c.end_us),
                c.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render helper that takes generic cues (like reference renderSrt).
pub fn render_srt(cues: &[(i64, i64, String)]) -> String {
    cues.iter()
        .enumerate()
        .map(|(i, (s, e, t))| format!("{}\n{} --> {}\n{}\n", i + 1, srt_time(*s), srt_time(*e), t))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_simple() {
        let cues = parse_srt("1\n00:00:01,000 --> 00:00:02,000\nHello").unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_us, 1_000_000);
        assert_eq!(cues[0].end_us, 2_000_000);
        assert_eq!(cues[0].text, "Hello");
    }
    #[test]
    fn parse_dot_separator() {
        let cues = parse_srt("00:00:01.500 --> 00:00:02.000\nHi").unwrap();
        assert_eq!(cues[0].start_us, 1_500_000);
    }
    #[test]
    fn parse_no_index() {
        let cues = parse_srt("00:00:01,000 --> 00:00:02,000\nHello").unwrap();
        assert_eq!(cues.len(), 1);
    }
}
