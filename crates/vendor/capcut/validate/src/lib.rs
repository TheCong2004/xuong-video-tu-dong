use capcut_core::timeline::*;
use std::path::Path;

// ---------------------------------------------------------------------------
// Public types — mirrors reference/src/lint.ts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

impl From<Severity> for Level {
    fn from(s: Severity) -> Self {
        match s {
            Severity::Error => Level::Error,
            Severity::Warning => Level::Warning,
            Severity::Info => Level::Warning,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Location {
    pub track: Option<String>,
    pub segment_id: Option<String>,
    pub material_id: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LintIssue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub fixable: bool,
    pub suggested_command: Option<String>,
    pub location: Option<Location>,
}

/// Legacy alias — original `Issue` kept for backwards compat.
#[derive(Debug, Clone)]
pub struct Issue {
    pub level: Level,
    pub message: String,
    pub code: Option<String>,
    pub fixable: bool,
}

impl From<LintIssue> for Issue {
    fn from(li: LintIssue) -> Self {
        Issue {
            level: Level::from(li.severity),
            message: li.message,
            code: Some(li.code),
            fixable: li.fixable,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LintOptions {
    pub max_chars_per_line: usize,
    pub max_cue_duration_us: i64,
    pub min_gap_between_captions_us: i64,
    pub check_local_paths: bool,
    /// Optional draft folder for media-outside-draft check.
    pub draft_dir: Option<String>,
    /// Reading-speed ceiling (chars/s) — not enforced as error, just cps check.
    pub max_chars_per_second: Option<f64>,
}

impl Default for LintOptions {
    fn default() -> Self {
        DEFAULT_LINT_OPTIONS.clone()
    }
}

pub const DEFAULT_LINT_OPTIONS: LintOptions = LintOptions {
    max_chars_per_line: 42,
    max_cue_duration_us: 7_000_000,
    min_gap_between_captions_us: 0,
    check_local_paths: false,
    draft_dir: None,
    max_chars_per_second: Some(20.0),
};

// Backwards-compat alias used in task description
pub type LintOptionsAlias = LintOptions;

pub const MIN_CAPTION_DURATION_US: i64 = 100_000;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn extract_text(content: &str) -> String {
    // TextMaterial.content is JSON like {"text":"hello","styles":[...]} or plain string.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
            return t.to_string();
        }
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
    }
    content.to_string()
}

fn can_fix_line_too_long(content: &str, max_chars: usize) -> bool {
    let text = extract_text(content);
    if text.is_empty() {
        return false;
    }
    // If re-wrapping would still leave a line over cap (e.g. single long word / CJK without spaces)
    // then not fixable. Simple heuristic: any word longer than cap => not fixable.
    for line in text.split('\n') {
        if line.len() <= max_chars {
            continue;
        }
        // check if wrapping at spaces could fix
        let mut fixed = true;
        for word in line.split(' ') {
            if word.chars().count() > max_chars {
                fixed = false;
                break;
            }
        }
        if !fixed {
            return false;
        }
    }
    true
}

fn file_exists(p: &str) -> bool {
    Path::new(p).exists()
}

// ---------------------------------------------------------------------------
// Core lint — operates on InternalTimeline
// ---------------------------------------------------------------------------

pub fn lint_timeline(tl: &InternalTimeline, opts: &LintOptions) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // ---- basic checks: empty timeline, duration<=0, missing material ----
    if tl.tracks.is_empty() {
        issues.push(LintIssue {
            severity: Severity::Warning,
            code: "empty-timeline".into(),
            message: "timeline has no tracks".into(),
            fixable: false,
            suggested_command: None,
            location: None,
        });
    }

    for track in &tl.tracks {
        for seg in &track.segments {
            if seg.timerange.duration <= 0 {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    code: "non-positive-duration".into(),
                    message: format!("segment {} has non-positive duration", seg.id),
                    fixable: false,
                    suggested_command: None,
                    location: Some(Location {
                        track: Some(track.name.clone()),
                        segment_id: Some(seg.id.clone()),
                        material_id: Some(seg.material_id.clone()),
                        path: None,
                    }),
                });
            }
            let mat_exists = match track.kind {
                TrackKind::Video => tl.materials.videos.iter().any(|m| m.id == seg.material_id),
                TrackKind::Audio => tl.materials.audios.iter().any(|m| m.id == seg.material_id),
                TrackKind::Text => tl.materials.texts.iter().any(|m| m.id == seg.material_id),
                _ => true,
            };
            if !mat_exists {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    code: "missing-material".into(),
                    message: format!(
                        "segment {} references missing material {}",
                        short_id(&seg.id),
                        short_id(&seg.material_id)
                    ),
                    fixable: false,
                    suggested_command: Some(format!("capcut remove <project> {}", short_id(&seg.id))),
                    location: Some(Location {
                        track: Some(track.name.clone()),
                        segment_id: Some(seg.id.clone()),
                        material_id: Some(seg.material_id.clone()),
                        path: None,
                    }),
                });
            }
        }
    }

    // ---- overlaps: per track sorted by start ----
    for track in &tl.tracks {
        let mut segs: Vec<&Segment> = track.segments.iter().collect();
        segs.sort_by_key(|s| s.timerange.start);
        for w in segs.windows(2) {
            let a = w[0];
            let b = w[1];
            let end_a = a.timerange.start + a.timerange.duration;
            let overlap = end_a - b.timerange.start;
            if overlap > 0 {
                let is_text = track.kind == TrackKind::Text;
                issues.push(LintIssue {
                    severity: Severity::Error,
                    code: if is_text { "caption-overlap".into() } else { "overlaps".into() },
                    message: format!(
                        "segments {} and {} overlap by {}ms on track \"{}\"",
                        short_id(&a.id),
                        short_id(&b.id),
                        overlap / 1000,
                        track.name
                    ),
                    fixable: true,
                    suggested_command: None,
                    location: Some(Location {
                        track: Some(track.name.clone()),
                        segment_id: Some(a.id.clone()),
                        material_id: None,
                        path: None,
                    }),
                });
            } else if opts.min_gap_between_captions_us > 0
                && track.kind == TrackKind::Text
                && overlap < 0
            {
                let gap = -overlap;
                if gap < opts.min_gap_between_captions_us {
                    let shrunk = a.timerange.duration - (opts.min_gap_between_captions_us - gap);
                    let fixable = shrunk >= MIN_CAPTION_DURATION_US;
                    issues.push(LintIssue {
                        severity: Severity::Warning,
                        code: "caption-gap-too-small".into(),
                        message: format!(
                            "captions {} and {} are {}ms apart (<{}ms)",
                            short_id(&a.id),
                            short_id(&b.id),
                            gap / 1000,
                            opts.min_gap_between_captions_us / 1000
                        ),
                        fixable,
                        suggested_command: None,
                        location: Some(Location {
                            track: Some(track.name.clone()),
                            segment_id: Some(a.id.clone()),
                            material_id: None,
                            path: None,
                        }),
                    });
                }
            }
        }
    }

    // ---- cue duration cap + line length cap (text tracks only) ----
    for track in &tl.tracks {
        if track.kind != TrackKind::Text {
            continue;
        }
        for seg in &track.segments {
            if seg.timerange.duration > opts.max_cue_duration_us {
                issues.push(LintIssue {
                    severity: Severity::Warning,
                    code: "cue-too-long".into(),
                    message: format!(
                        "caption {} runs {}ms (>{}s)",
                        short_id(&seg.id),
                        seg.timerange.duration / 1000,
                        opts.max_cue_duration_us / 1_000_000
                    ),
                    fixable: true,
                    suggested_command: None,
                    location: Some(Location {
                        track: Some(track.name.clone()),
                        segment_id: Some(seg.id.clone()),
                        material_id: None,
                        path: None,
                    }),
                });
            }

            // line length — need material content
            if let Some(mat) = tl.materials.texts.iter().find(|m| m.id == seg.material_id) {
                let text = extract_text(&mat.content);
                for line in text.split('\n') {
                    let len = line.chars().count();
                    if len > opts.max_chars_per_line {
                        issues.push(LintIssue {
                            severity: Severity::Warning,
                            code: "line-too-long".into(),
                            message: format!(
                                "caption {} has {}-char line (>{}): \"{}…\"",
                                short_id(&seg.id),
                                len,
                                opts.max_chars_per_line,
                                line.chars().take(50).collect::<String>()
                            ),
                            fixable: can_fix_line_too_long(&mat.content, opts.max_chars_per_line),
                            suggested_command: None,
                            location: Some(Location {
                                track: Some(track.name.clone()),
                                segment_id: Some(seg.id.clone()),
                                material_id: None,
                                path: None,
                            }),
                        });
                        break;
                    }
                }

                // reading speed check
                if let Some(cps) = opts.max_chars_per_second {
                    if cps > 0.0 && !text.is_empty() && seg.timerange.duration > 0 {
                        let visible = text.chars().filter(|c| !c.is_whitespace()).count();
                        let secs = seg.timerange.duration as f64 / 1_000_000.0;
                        let rate = visible as f64 / secs;
                        if visible > 0 && rate > cps {
                            issues.push(LintIssue {
                                severity: Severity::Warning,
                                code: "caption-too-fast".into(),
                                message: format!(
                                    "caption {} runs at {:.1} chars/s (>{}) — {} chars in {}ms",
                                    short_id(&seg.id),
                                    rate,
                                    cps as i64,
                                    visible,
                                    (secs * 1000.0) as i64
                                ),
                                fixable: false,
                                suggested_command: Some(format!(
                                    "capcut trim <project> {} <start> {}ms  # or shorten the text",
                                    seg.id,
                                    ((visible as f64 / cps) * 1000.0).ceil() as i64
                                )),
                                location: Some(Location {
                                    track: Some(track.name.clone()),
                                    segment_id: Some(seg.id.clone()),
                                    material_id: None,
                                    path: None,
                                }),
                            });
                        }
                    }
                }
            }
        }
    }

    // ---- main-track-gap (first video track) ----
    if let Some(main) = tl.tracks.iter().find(|t| t.kind == TrackKind::Video) {
        let mut segs: Vec<&Segment> = main.segments.iter().collect();
        segs.sort_by_key(|s| s.timerange.start);
        for w in segs.windows(2) {
            let a = w[0];
            let b = w[1];
            let end_a = a.timerange.start + a.timerange.duration;
            let gap = b.timerange.start - end_a;
            if gap > 0 {
                issues.push(LintIssue {
                    severity: Severity::Warning,
                    code: "main-track-gap".into(),
                    message: format!(
                        "main video track has a {}ms gap between segments {} and {} — CapCut closes it on open, shifting later content",
                        gap / 1000,
                        short_id(&a.id),
                        short_id(&b.id)
                    ),
                    fixable: false,
                    suggested_command: Some(format!("capcut shift <project> <segment> -{}ms", gap / 1000)),
                    location: Some(Location {
                        track: Some(main.name.clone()),
                        segment_id: Some(a.id.clone()),
                        material_id: None,
                        path: None,
                    }),
                });
            }
        }
    }

    // ---- unknown enum slug: check font ids against bundled enums via capcut-capcut crate ----
    {
        use capcut_capcut::enums::{slugs_for, Category, Namespace};
        let mut known_fonts = std::collections::HashSet::new();
        for ns in [Namespace::CapCut, Namespace::JianYing] {
            for s in slugs_for(Category::Fonts, ns) {
                known_fonts.insert(s.to_ascii_lowercase());
            }
        }
        if !known_fonts.is_empty() {
            for mat in &tl.materials.texts {
                if let Some(font) = &mat.style.font {
                    if !font.is_empty() && !known_fonts.contains(&font.to_ascii_lowercase()) {
                        issues.push(LintIssue {
                            severity: Severity::Info,
                            code: "unknown-effect-slug".into(),
                            message: format!(
                                "text material {} uses font id {} not in bundled enum table",
                                short_id(&mat.id),
                                font
                            ),
                            fixable: false,
                            suggested_command: None,
                            location: Some(Location {
                                track: None,
                                segment_id: None,
                                material_id: Some(mat.id.clone()),
                                path: None,
                            }),
                        });
                    }
                }
            }
        }
    }

    // ---- missing files / media-outside-draft ----
    if opts.check_local_paths {
        for mat in &tl.materials.videos {
            check_media_path(mat.id.clone(), mat.path.clone(), opts, &mut issues);
        }
        for mat in &tl.materials.audios {
            check_media_path(mat.id.clone(), mat.path.clone(), opts, &mut issues);
        }
    }

    issues
}
fn check_media_path(
    material_id: String,
    path: String,
    opts: &LintOptions,
    issues: &mut Vec<LintIssue>,
) {
    if path.is_empty() || path.starts_with("http://") || path.starts_with("https://") {
        return;
    }
    if !file_exists(&path) {
        issues.push(LintIssue {
            severity: Severity::Error,
            code: "missing-file".into(),
            message: format!("material {} references file that doesn't exist: {}", short_id(&material_id), path),
            fixable: false,
            suggested_command: Some("capcut relink <project> --dir <directory-containing-the-files>".into()),
            location: Some(Location { track: None, segment_id: None, material_id: Some(material_id.clone()), path: Some(path.clone()) }),
        });
        return;
    }
    if let Some(draft_dir) = &opts.draft_dir {
        let is_abs = path.starts_with('/') || path.starts_with('\\') || (path.len() >= 2 && path.chars().nth(1) == Some(':'));
        if is_abs {
            let under = {
                let p_norm = path.replace('\\', "/");
                let d_norm = draft_dir.replace('\\', "/");
                p_norm.starts_with(d_norm.trim_end_matches('/'))
            };
            if !under {
                issues.push(LintIssue {
                    severity: Severity::Info,
                    code: "media-outside-draft".into(),
                    message: format!("material {} references media outside draft folder: {}", short_id(&material_id), path),
                    fixable: false,
                    suggested_command: None,
                    location: Some(Location { track: None, segment_id: None, material_id: Some(material_id), path: Some(path) }),
                });
            }
        }
    }
}






#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub total: usize,
}

pub fn summarize(issues: &[LintIssue]) -> Summary {
    let mut s = Summary::default();
    s.total = issues.len();
    for i in issues {
        match i.severity {
            Severity::Error => s.errors += 1,
            Severity::Warning => s.warnings += 1,
            Severity::Info => s.info += 1,
        }
    }
    s
}

pub fn lint_exit_code(summary: &Summary) -> i32 {
    if summary.errors > 0 || summary.warnings > 0 { 1 } else { 0 }
}

// Legacy helpers operating on Issue (Level-based)
pub fn summarize_legacy(issues: &[Issue]) -> Summary {
    let mut s = Summary::default();
    s.total = issues.len();
    for i in issues {
        match i.level {
            Level::Error => s.errors += 1,
            Level::Warning => s.warnings += 1,
        }
    }
    s
}

// ---------------------------------------------------------------------------
// fix_timeline — mechanical repairs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct FixResult {
    pub fixed: Vec<LintIssue>,
    pub remaining: Vec<LintIssue>,
}

pub fn fix_timeline(tl: &mut InternalTimeline, opts: &LintOptions) -> FixResult {
    let before = lint_timeline(tl, opts);
    let mut fixed = Vec::new();

    // Pass 1: cap cue durations
    for track in &mut tl.tracks {
        if track.kind != TrackKind::Text { continue; }
        for seg in &mut track.segments {
            if seg.timerange.duration > opts.max_cue_duration_us {
                seg.timerange.duration = opts.max_cue_duration_us;
                if let Some(ref mut src) = seg.source_timerange {
                    // keep source in sync if it matched before
                    let _ = src;
                }
            }
        }
    }

    // Pass 2: resolve overlaps by trimming earlier segment
    for track in &mut tl.tracks {
        let mut segs_sorted: Vec<usize> = (0..track.segments.len()).collect();
        segs_sorted.sort_by_key(|&i| track.segments[i].timerange.start);
        for w in 0..segs_sorted.len().saturating_sub(1) {
            let a_idx = segs_sorted[w];
            let b_idx = segs_sorted[w + 1];
            let end_a = track.segments[a_idx].timerange.start + track.segments[a_idx].timerange.duration;
            let start_b = track.segments[b_idx].timerange.start;
            if end_a > start_b {
                let overlap = end_a - start_b;
                let new_dur = track.segments[a_idx].timerange.duration - overlap;
                track.segments[a_idx].timerange.duration = new_dur.max(0);
            }
        }
    }

    // Pass 3: widen small gaps (only if fixable — keep >= MIN_CAPTION_DURATION_US)
    if opts.min_gap_between_captions_us > 0 {
        for track in &mut tl.tracks {
            if track.kind != TrackKind::Text { continue; }
            let mut idxs: Vec<usize> = (0..track.segments.len()).collect();
            idxs.sort_by_key(|&i| track.segments[i].timerange.start);
            for w in 0..idxs.len().saturating_sub(1) {
                let a_idx = idxs[w];
                let b_idx = idxs[w + 1];
                let end_a = track.segments[a_idx].timerange.start + track.segments[a_idx].timerange.duration;
                let gap = track.segments[b_idx].timerange.start - end_a;
                if gap > 0 && gap < opts.min_gap_between_captions_us {
                    let need = opts.min_gap_between_captions_us - gap;
                    let new_dur = track.segments[a_idx].timerange.duration - need;
                    if new_dur >= MIN_CAPTION_DURATION_US {
                        track.segments[a_idx].timerange.duration = new_dur;
                    }
                }
            }
        }
    }

    let after = lint_timeline(tl, opts);
    // fixed = before - after (by code+segment)
    for b in &before {
        let still = after.iter().any(|a| a.code == b.code && a.location.as_ref().and_then(|l| l.segment_id.clone()) == b.location.as_ref().and_then(|l| l.segment_id.clone()));
        if !still && b.fixable {
            fixed.push(b.clone());
        }
    }

    FixResult { fixed, remaining: after }
}

// Keep alias for task spec
pub fn fix_draft_compat(tl: &mut InternalTimeline, opts: &LintOptions) -> FixResult {
    fix_timeline(tl, opts)
}

// ---------------------------------------------------------------------------
// Legacy validate() — now delegates to lint_timeline and maps to Issue
// ---------------------------------------------------------------------------

pub fn validate(tl: &InternalTimeline) -> Vec<Issue> {
    lint_timeline(tl, &LintOptions::default()).into_iter().map(Issue::from).collect()
}

pub fn validate_with_options(tl: &InternalTimeline, opts: &LintOptions) -> Vec<Issue> {
    lint_timeline(tl, opts).into_iter().map(Issue::from).collect()
}
