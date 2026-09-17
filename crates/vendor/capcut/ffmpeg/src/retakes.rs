const US: f64 = 1_000_000.0;

#[derive(Debug, Clone)]
pub struct RetakeCue {
    pub text: String,
    pub start_us: i64,
    pub end_us: i64,
}

#[derive(Debug, Clone)]
pub struct RetakeOptions {
    pub window: f64,
    pub similarity: f64,
    pub min_words: usize,
}

impl Default for RetakeOptions {
    fn default() -> Self {
        Self {
            window: 60.0,
            similarity: 0.8,
            min_words: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetakeSide {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub start_us: i64,
    pub end_us: i64,
}

#[derive(Debug, Clone)]
pub struct RetakePair {
    pub earlier: RetakeSide,
    pub later: RetakeSide,
    pub similarity: f64,
    pub words: usize,
}

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
pub struct RetakeReport {
    pub source: String,
    pub window: f64,
    pub similarity: f64,
    pub min_words: usize,
    pub cues: usize,
    pub duration: Option<f64>,
    pub duration_us: Option<i64>,
    pub retakes: Vec<RetakePair>,
    pub cuts: Vec<Segment>,
    pub keeps: Vec<Segment>,
}

/// Comparison form of a token: lower-case, letters/digits only.
pub fn normalize_token(token: &str) -> String {
    token
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

pub fn normalize_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(normalize_token)
        .filter(|t| !t.is_empty())
        .collect()
}

fn lcs_length(a: &[String], b: &[String]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let mut prev = vec![0usize; b.len() + 1];
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            if a[i - 1] == b[j - 1] {
                cur[j] = prev[j - 1] + 1;
            } else {
                cur[j] = prev[j].max(cur[j - 1]);
            }
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

pub fn sequence_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let lcs = lcs_length(a, b) as f64;
    let sim = (2.0 * lcs) / (a.len() + b.len()) as f64;
    (sim * 1000.0).round() / 1000.0
}

fn side(cue: &RetakeCue) -> RetakeSide {
    RetakeSide {
        text: cue.text.clone(),
        start: round6(cue.start_us as f64 / US),
        end: round6(cue.end_us as f64 / US),
        start_us: cue.start_us,
        end_us: cue.end_us,
    }
}

fn round6(n: f64) -> f64 {
    (n * 1_000_000.0).round() / 1_000_000.0
}

pub fn find_retakes(cues: &[RetakeCue], opts: &RetakeOptions) -> Vec<RetakePair> {
    let window_us = (opts.window * US).round() as i64;
    let floor = opts.similarity;
    let min_words = opts.min_words;
    let mut ordered: Vec<&RetakeCue> = cues.iter().collect();
    ordered.sort_by_key(|c| c.start_us);
    let tokens: Vec<Vec<String>> = ordered.iter().map(|c| normalize_words(&c.text)).collect();
    let mut pairs = Vec::new();
    for i in 0..ordered.len() {
        if tokens[i].len() < min_words {
            continue;
        }
        for j in (i + 1)..ordered.len() {
            if ordered[j].start_us - ordered[i].end_us > window_us {
                break;
            }
            if tokens[j].len() < min_words {
                continue;
            }
            let sim = sequence_similarity(&tokens[i], &tokens[j]);
            if sim >= floor {
                pairs.push(RetakePair {
                    earlier: side(ordered[i]),
                    later: side(ordered[j]),
                    similarity: sim,
                    words: tokens[i].len(),
                });
                break;
            }
        }
    }
    pairs
}

pub fn merge_spans(spans: &[SilenceSpan]) -> Vec<SilenceSpan> {
    let mut sorted = spans.to_vec();
    sorted.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    let mut merged: Vec<SilenceSpan> = Vec::new();
    for span in sorted {
        if let Some(last) = merged.last_mut() {
            if let Some(last_end) = last.end {
                if span.start <= last_end {
                    last.end = match span.end {
                        Some(b) => Some(last_end.max(b)),
                        None => None,
                    };
                    continue;
                }
            }
        }
        merged.push(span);
    }
    merged
}

fn build_keep_spans(silences: &[SilenceSpan], duration: Option<f64>) -> Vec<SilenceSpan> {
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

fn spans_to_segments(spans: &[SilenceSpan]) -> Vec<Segment> {
    spans
        .iter()
        .map(|s| {
            let duration = s.end.map(|e| round6(e - s.start));
            let start_us = (s.start * US).round() as i64;
            let end_us = s.end.map(|e| (e * US).round() as i64);
            let duration_us = end_us.map(|eu| eu - start_us);
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

pub fn build_retake_report(
    cues: &[RetakeCue],
    duration_us: Option<i64>,
    opts: &RetakeOptions,
    source: &str,
) -> RetakeReport {
    let retakes = find_retakes(cues, opts);
    let cuts_spans: Vec<SilenceSpan> = retakes
        .iter()
        .map(|p| SilenceSpan { start: p.earlier.start, end: Some(p.earlier.end) })
        .collect();
    let cuts = merge_spans(&cuts_spans);
    let bound = if let Some(du) = duration_us {
        if du > 0 { du as f64 / US } else { 0.0 }
    } else if cues.is_empty() {
        0.0
    } else {
        cues.iter().map(|c| c.end_us).max().unwrap() as f64 / US
    };
    let duration_opt = if bound > 0.0 { Some(round6(bound)) } else { None };
    let duration_us_opt = if bound > 0.0 { Some((bound * US).round() as i64) } else { None };
    let keeps = build_keep_spans(&cuts, duration_opt);

    RetakeReport {
        source: source.to_string(),
        window: opts.window,
        similarity: opts.similarity,
        min_words: opts.min_words,
        cues: cues.len(),
        duration: duration_opt,
        duration_us: duration_us_opt,
        retakes,
        cuts: spans_to_segments(&cuts),
        keeps: spans_to_segments(&keeps),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_words_basic() {
        let w = normalize_words("Hello, World! 123");
        assert_eq!(w, vec!["hello", "world", "123"]);
    }

    #[test]
    fn sequence_similarity_identical() {
        let a = vec!["hello".to_string(), "world".to_string()];
        let b = vec!["hello".to_string(), "world".to_string()];
        assert!((sequence_similarity(&a, &b) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn sequence_similarity_partial() {
        let a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let b = vec!["a".to_string(), "c".to_string()];
        // LCS=2, sim=2*2/5=0.8
        assert!((sequence_similarity(&a, &b) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn find_retakes_window_and_similarity() {
        let cues = vec![
            RetakeCue { text: "hello world this is test".to_string(), start_us: 0, end_us: 2_000_000 },
            RetakeCue { text: "hello world this is test".to_string(), start_us: 3_000_000, end_us: 5_000_000 },
            RetakeCue { text: "completely different sentence here".to_string(), start_us: 6_000_000, end_us: 8_000_000 },
        ];
        let opts = RetakeOptions { window: 60.0, similarity: 0.8, min_words: 4 };
        let pairs = find_retakes(&cues, &opts);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].words, 5);
    }

    #[test]
    fn merge_spans_overlapping() {
        let spans = vec![
            SilenceSpan { start: 1.0, end: Some(2.0) },
            SilenceSpan { start: 1.5, end: Some(3.0) },
            SilenceSpan { start: 5.0, end: Some(6.0) },
        ];
        let merged = merge_spans(&spans);
        assert_eq!(merged.len(), 2);
        assert!((merged[0].end.unwrap() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn build_report_duration_from_cues() {
        let cues = vec![
            RetakeCue { text: "hello world this is test".to_string(), start_us: 0, end_us: 1_000_000 },
            RetakeCue { text: "hello world this is test".to_string(), start_us: 2_000_000, end_us: 3_000_000 },
        ];
        let opts = RetakeOptions::default();
        let report = build_retake_report(&cues, None, &opts, "test");
        assert_eq!(report.cues, 2);
        assert_eq!(report.retakes.len(), 1);
        assert!(report.duration.is_some());
    }
}
