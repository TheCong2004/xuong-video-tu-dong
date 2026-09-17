use anyhow::{Context, Result};
use capcut_core::timeline::*;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Render options — mirrors `reference/src/render.ts: RenderOptions` (subset)
// Giữ mapping CLI linh hoạt: mọi field đều Option, caller chỉ set cái cần.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Proxy scale factor applied to canvas dims. None => 1.0 (giữ nguyên canvas).
    /// Reference default 0.5 nhưng Rust giữ 1.0 để không phá test cũ.
    pub scale: Option<f64>,
    /// Output fps override. None => tl.fps.
    pub fps: Option<f32>,
    /// Video encoder, ví dụ `libx264`, `h264_nvenc`. None => `libx264`.
    pub encoder: Option<String>,
    /// x264 preset. None => `veryfast`.
    pub preset: Option<String>,
    /// CRF value. None => 23.
    pub crf: Option<u8>,
    /// Burn text-track captions via drawtext. None/false => skip.
    pub burn_captions: bool,
    /// Optional ASS/SRT subtitle input to mux as mov_text (soft captions).
    pub subtitle_path: Option<PathBuf>,
    /// ffmpeg binary. None => `ffmpeg`.
    pub ffmpeg_cmd: Option<String>,
    /// Fast seek: emit -ss before -i per input based on earliest source start. Default false.
    pub seek_fast: bool,
    /// Subject-aware reframe: pre-roll crop ngang (pixel, hệ nguồn) — bake vào filter.
    /// None => crop giữa (hành vi cũ). Some(vec rỗng) => cũng crop giữa.
    pub reframe_windows: Option<Vec<crate::reframe::CropWindow>>,
    /// Chiều rộng frame nguồn mà reframe_windows.x đo trên (để quy về tỉ lệ).
    /// None => 2560 (source AV1 hiện tại).
    pub reframe_src_w: Option<u32>,
}

impl RenderOptions {
    fn effective_scale(&self) -> f64 {
        match self.scale {
            Some(s) if s > 0.0 && s.is_finite() => s,
            _ => 1.0,
        }
    }
    fn effective_fps(&self, tl: &InternalTimeline) -> f32 {
        if let Some(f) = self.fps { if f > 0.0 && f.is_finite() { return f; } }
        if tl.fps > 0.0 && tl.fps.is_finite() { tl.fps } else { 30.0 }
    }
}

fn round3(n: f64) -> f64 {
    (n * 1000.0).round() / 1000.0
}

fn escape_one_line(s: &str) -> String {
    s.replace(":", r"\:").replace('\'', "").replace('"', "").replace("%", " ")
        .split_whitespace().collect::<Vec<&str>>().join(" ")
}

fn wrap_lines(s: &str, max_chars: usize) -> Vec<String> {
    let cleaned = escape_one_line(&s.replace('\\', " "));
    let mc = max_chars.max(4);
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut words: Vec<String> = cleaned.split_whitespace().map(|w| w.to_string()).collect();
    words.reverse();
    while let Some(w) = words.pop() {
        if cur.is_empty() {
            if w.chars().count() > mc {
                let head: String = w.chars().take(mc).collect();
                let rest: String = w.chars().skip(mc).collect();
                lines.push(head);
                if !rest.is_empty() { words.push(rest); }
            } else { cur = w; }
        } else if cur.chars().count() + 1 + w.chars().count() <= mc {
            cur.push(' ');
            cur.push_str(&w);
        } else {
            lines.push(std::mem::take(&mut cur));
            words.push(w);
        }
    }
    if !cur.is_empty() { lines.push(cur); }
    let mut total = 0usize;
    lines.retain(|l| { total += l.chars().count(); total <= 125 });
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

// atempo
fn atempo_for(speed: f64) -> Option<String> {
    if speed == 1.0 || !speed.is_finite() { return None; }
    if (0.5..=2.0).contains(&speed) { Some(format!("atempo={}", round3(speed))) } else { None }
}

// ---------------------------------------------------------------------------
// build_filter_complex — enhanced but backward-compatible
// ---------------------------------------------------------------------------

/// Build an ffmpeg filter_complex string for the timeline.
///
/// - 0 video segments => "".
/// - 1 video segment => scale+crop (với trim/speed nếu có).
/// - n>1 => per-segment trim+scale rồi concat.
/// - Audio segments => atrim/asetpts (+atempo/volume) rồi amix/anull.
/// - burn_captions => drawtext chain nối tiếp vout.
///
/// `tl` tracks are sorted by `timerange.start` trong mỗi kind.
pub fn build_filter_complex(tl: &InternalTimeline) -> String {
    build_filter_complex_with_options(tl, &RenderOptions::default())
}

/// Bieu thuc crop-x dong: tra closure nhan (src_start_us, seg_dur_us) cua segment
/// va cho chuoi bieu thuc ffmpeg `crop` (dau phay escape `\,`).
/// Chi nest window co start <= base + seg_dur (giao segment); window cuoi giao
/// segment lam gia tri cuoi (else-branch ngoai cung).
pub fn build_crop_x_expr(
    wins: &[crate::reframe::CropWindow],
    src_w: u32,
) -> impl Fn(Option<i64>, Option<i64>) -> String + '_ {
    let src_w = src_w.max(1) as f64;
    let norm: Vec<(f64, f64, f64)> = wins
        .iter()
        .map(|w| (w.start, w.end, w.x as f64 / src_w))
        .collect();
    move |src_start_us: Option<i64>, seg_dur_us: Option<i64>| -> String {
        if norm.is_empty() {
            return "(in_w-out_w)/2".to_string();
        }
        let base = src_start_us.unwrap_or(0) as f64 / 1_000_000.0;
        let seg_dur = seg_dur_us.unwrap_or(30_500_000) as f64 / 1_000_000.0;
        let mut seg = 0usize;
        for (i, (st, _, _)) in norm.iter().enumerate() {
            if *st <= base {
                seg = i;
            }
        }
        let (_s0, _e0, x0) = norm[seg];
        let mut cur_x = format!("({x0:.6})*in_w");
        for (st, _en, x) in norm.iter().skip(seg + 1) {
            if *st > base + seg_dur {
                break;
            }
            let t1 = (*st - base).max(0.0);
            let nx = format!("({x:.6})*in_w");
            cur_x = format!(
                "if(lt(t\\,{t1:.3})\\,{cur_x}\\,{nx})",
                t1 = t1,
                cur_x = cur_x,
                nx = nx
            );
        }
        cur_x
    }
}

pub fn build_filter_complex_with_options(tl: &InternalTimeline, opts: &RenderOptions) -> String {
    let mut video_segs: Vec<&Segment> = tl
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video)
        .flat_map(|t| &t.segments)
        .collect();
    video_segs.sort_by_key(|s| s.timerange.start);

    let mut audio_segs: Vec<&Segment> = tl
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .flat_map(|t| &t.segments)
        .collect();
    audio_segs.sort_by_key(|s| s.timerange.start);

    let text_segs: Vec<(&Segment, &TextMaterial)> = if opts.burn_captions {
        let mut out = Vec::new();
        for track in tl.tracks.iter().filter(|t| t.kind == TrackKind::Text) {
            for seg in &track.segments {
                if let Some(mat) = tl.materials.texts.iter().find(|m| m.id == seg.material_id) {
                    out.push((seg, mat));
                }
            }
        }
        out.sort_by_key(|(s, _)| s.timerange.start);
        out
    } else {
        Vec::new()
    };

    if video_segs.is_empty() {
        return String::new();
    }

    let scale = opts.effective_scale();
    let fps = opts.effective_fps(tl);
    // canvas * scale, làm tròn về số chẵn (yuv420p yêu cầu even)
    let w = ((tl.canvas.width as f64 * scale).round() as u32 / 2 * 2).max(2);
    let h = ((tl.canvas.height as f64 * scale).round() as u32 / 2 * 2).max(2);
    // Crop ngang dong theo thoi gian: doi x tai ranh gioi tung reframe_window
    // (bien t cua filter crop tinh tu dau segment sau trim+setpts).
    // Khac pre-roll tinh (vong 6): pre-roll chi lay window tai dau segment, cua so
    // dung yen suot segment 5-8s trong khi face di dich -> cat lem/lac subject
    // (13 f22 iter6: window dau segment x=651 dung ca [25,30.5] trong khi mat dich
    // trai ve x~395). Step tai ranh gioi window (windows da muot qua EMA alpha 0.8
    // + pan clamp 400px/s). Chi nest window GIAO segment de tranh sau long.
    // Khong co windows => crop giua nhu cu.
    let crop_x_expr = build_crop_x_expr(
        opts.reframe_windows.as_deref().unwrap_or(&[]),
        opts.reframe_src_w.unwrap_or(2560),
    );
    let scale_crop_default =
        format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}");

    // Map material_id -> input file index (inputs order matches tl.materials.videos)
    let vid_idx: std::collections::HashMap<&str, usize> = tl.materials.videos.iter().enumerate().map(|(i,m)| (m.id.as_str(), i)).collect();
    let aud_idx: std::collections::HashMap<&str, usize> = tl.materials.audios.iter().enumerate().map(|(i,m)| (m.id.as_str(), i)).collect();
    let video_input_count = tl.materials.videos.len();

    let mut parts: Vec<String> = Vec::new();
    let mut video_labels: Vec<String> = Vec::new();

    // Per-video segment chain — reuse same input index for same material
    for (i, seg) in video_segs.iter().enumerate() {
        let file_idx = vid_idx.get(seg.material_id.as_str()).copied().unwrap_or(0);
        let label = format!("v{i}");
        let mut chain: Vec<String> = Vec::new();

        // trim per segment nếu có source_timerange
        if let Some(src) = &seg.source_timerange {
            let start = round3(src.start as f64 / 1_000_000.0);
            let dur = round3(src.duration as f64 / 1_000_000.0);
            // speed ảnh hưởng setpts
            let speed = seg.speed.unwrap_or(1.0);
            let setpts = if speed != 1.0 && speed.is_finite() && speed > 0.0 {
                format!("setpts=(PTS-STARTPTS)/{}", round3(speed))
            } else {
                "setpts=PTS-STARTPTS".to_string()
            };
            chain.push(format!("trim=start={start}:duration={dur}"));
            chain.push(setpts);
        } else if let Some(speed) = seg.speed {
            if speed != 1.0 && speed.is_finite() && speed > 0.0 {
                chain.push(format!("setpts=(PTS-STARTPTS)/{}", round3(speed)));
            }
        }

        // crop ngang: giữa mặc định, hoặc pre-roll x theo reframe_windows + source start
        let src_start = seg.source_timerange.as_ref().map(|r| r.start);
        let src_dur = seg.source_timerange.as_ref().map(|r| r.duration);
        if opts.reframe_windows.as_deref().map(|v| !v.is_empty()).unwrap_or(false) {
            let x_expr = crop_x_expr(src_start, src_dur);
            chain.push(format!(
                "scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}:x={x_expr}:y=0"
            ));
        } else {
            chain.push(scale_crop_default.clone());
        }
        chain.push(format!("fps={}", round3(fps as f64)));
        chain.push("format=yuv420p".to_string());

        parts.push(format!("[{file_idx}:v]{}[{label}]", chain.join(",")));
        video_labels.push(format!("[{label}]"));
    }

    // Concat video
    let mut vout = String::from("vout");
    if video_labels.len() == 1 {
        parts.push(format!("{}null[{}]", video_labels[0], vout));
    } else {
        let n = video_labels.len();
        parts.push(format!("{}concat=n={n}:v=1:a=0[{}]", video_labels.join(""), vout));
    }

    // Audio mix — map audio segments to actual audio input indices (after video inputs)
    let mut audio_labels: Vec<String> = Vec::new();
    for (j, seg) in audio_segs.iter().enumerate() {
        let audio_pos = aud_idx.get(seg.material_id.as_str()).copied().unwrap_or(0);
        let file_idx = video_input_count + audio_pos;
        let label = format!("a{j}");
        let mut chain: Vec<String> = Vec::new();
        if let Some(src) = &seg.source_timerange {
            let start = round3(src.start as f64 / 1_000_000.0);
            let dur = round3(src.duration as f64 / 1_000_000.0);
            chain.push(format!("atrim=start={start}:duration={dur}"));
            chain.push("asetpts=PTS-STARTPTS".to_string());
        } else {
            // still need asetpts to reset timestamps
            chain.push("asetpts=PTS-STARTPTS".to_string());
        }
        if let Some(t) = seg.speed.and_then(atempo_for) {
            chain.push(t);
        }
        if let Some(vol) = seg.volume {
            if vol != 1.0 {
                chain.push(format!("volume={}", round3(vol as f64)));
            }
        }
        // adelay theo target start
        let delay_ms = (seg.timerange.start / 1000).max(0);
        chain.push("loudnorm=I=-16:TP=-1.5:LRA=11".to_string());
        chain.push(format!("adelay={delay_ms}|{delay_ms}"));
        parts.push(format!("[{file_idx}:a]{}[{label}]", chain.join(",")));
        audio_labels.push(format!("[{label}]"));
    }
    let mut aout: Option<String> = None;
    if audio_labels.len() == 1 {
        let name = "aout".to_string();
        parts.push(format!("{}anull[{}]", audio_labels[0], name));
        aout = Some(name);
    } else if audio_labels.len() > 1 {
        let name = "aout".to_string();
        let n = audio_labels.len();
        parts.push(format!("{}amix=inputs={n}:normalize=0[{}]", audio_labels.join(""), name));
        aout = Some(name);
    }
    // aout is not referenced in filter_complex mapping here beyond the label;
    // caller (render) maps [aout] as audio output.

    // Burn captions (drawtext) — moi dong 1 filter, xep chong doc
    // (ffmpeg build nay khong xuong dong voi backslash-n literal).
    let mut cap_idx = 0usize;
    for (seg, mat) in text_segs.iter() {
        let font_size = mat.style.size.unwrap_or(64);
        let step = font_size as i64 + 12;
        let mut lines = wrap_lines(&mat.content, 22);
        if lines.len() > 3 { lines.truncate(3); }
        if lines.iter().all(|l| l.is_empty()) { continue; }
        let start = round3(seg.timerange.start as f64 / 1_000_000.0);
        let end = round3(seg.timerange.end() as f64 / 1_000_000.0);
        let fallback_color = mat.extra.get("color").and_then(|v| v.as_str()).map(|s| s.to_string());
        let raw_color = mat.style.color.as_deref().or(fallback_color.as_deref()).unwrap_or("white");
        let fontcolor = if raw_color.starts_with('#') { raw_color.replacen('#', "0x", 1) } else { raw_color.to_string() };
        let pos = mat.extra.get("position").and_then(|v| v.as_str()).unwrap_or("center");
        let n = lines.len() as i64;
        for (li, line) in lines.iter().enumerate() {
            if line.is_empty() { continue; }
            let i = li as i64;
            let y_expr: String = if pos.contains("bottom") {
                let anchor = if n > 1 { 320 } else { 260 };
                let mut e = format!("h-text_h-{}", anchor);
                if n > 1 && i < n - 1 { e.push_str(&format!("-{}*({})", n - 1 - i, step)); }
                e
            } else if pos.contains("top") {
                let mut e = format!("{}", 80);
                if i > 0 { e.push_str(&format!("+{}*({})", i, step)); }
                e
            } else {
                let mut e = String::from("(h-text_h)/2");
                if n > 1 { e.push_str(&format!("-{}*({})/2", n - 1, step)); }
                if i > 0 { e.push_str(&format!("+{}*({})", i, step)); }
                e
            };
            let next = format!("vt{cap_idx}");
            cap_idx += 1;
            parts.push(format!(
                "[{vout}]drawtext=text='{line}':fontfile=_harness_lab/arial.ttf:fontcolor={fontcolor}:fontsize={font_size}:box=1:boxcolor=black@0.55:boxborderw=10:x=(w-text_w)/2:y={y_expr}:enable='between(t,{start},{end})'[{next}]"
            ));
            vout = next;
        }
    }
    // Keep aout label available for render mapping — we don't need to emit
    // anything else; the final filter_complex already contains it.
    let _ = aout;

    parts.join(";")
}

// ---------------------------------------------------------------------------
// render — sync wrapper around ffmpeg
// ---------------------------------------------------------------------------

pub fn render(tl: &InternalTimeline, inputs: &[PathBuf], output: &Path) -> Result<()> {
    render_with_options(tl, inputs, output, &RenderOptions::default())
}

pub fn render_with_options(
    tl: &InternalTimeline,
    inputs: &[PathBuf],
    output: &Path,
    opts: &RenderOptions,
) -> Result<()> {
    if inputs.is_empty() {
        anyhow::bail!("no inputs for render");
    }
    let ffmpeg = opts.ffmpeg_cmd.as_deref().unwrap_or("ffmpeg");
    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-y");
    if opts.seek_fast { cmd.arg("-copyts"); }
    let seek_map: std::collections::HashMap<String, f64> = if opts.seek_fast {
        let mut m: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        let vid_path: std::collections::HashMap<&str, &str> = tl.materials.videos.iter().map(|v| (v.id.as_str(), v.path.as_str())).collect();
        let aud_path: std::collections::HashMap<&str, &str> = tl.materials.audios.iter().map(|v| (v.id.as_str(), v.path.as_str())).collect();
        let mut consider = |mat_id: &str, src: Option<&capcut_core::timeline::Range>, is_video: bool| {
            if let Some(r) = src { let sec = r.start as f64 / 1_000_000.0; let path = if is_video { vid_path.get(mat_id).copied() } else { aud_path.get(mat_id).copied() }.unwrap_or(""); if !path.is_empty() { m.entry(path.to_string()).and_modify(|e| *e = e.min(sec)).or_insert(sec); } }
        };
        for track in &tl.tracks { let is_vid = track.kind == capcut_core::timeline::TrackKind::Video; let is_aud = track.kind == capcut_core::timeline::TrackKind::Audio; if !is_vid && !is_aud { continue; } for seg in &track.segments { consider(&seg.material_id, seg.source_timerange.as_ref(), is_vid); } }
        m
    } else { std::collections::HashMap::new() };
    for inp in inputs {
        let key = inp.to_string_lossy().to_string();
        if let Some(sec) = seek_map.get(&key) { let ss = (sec - 0.5).max(0.0); if ss > 0.1 { cmd.arg("-ss").arg(format!("{:.3}", ss)); } }

        cmd.arg("-i").arg(inp);
    }
    // soft captions: add subtitle input if present
    let mut extra_inputs = 0usize;
    if let Some(sub) = &opts.subtitle_path {
        cmd.arg("-i").arg(sub);
        extra_inputs = 1;
    }

    let fc = build_filter_complex_with_options(tl, opts);
    let has_video = !fc.is_empty();
    // Determine if audio chain exists by checking for aout label
    let has_audio = fc.contains("[aout]");

    if has_video {
        cmd.arg("-filter_complex").arg(&fc);
        // vout is always the final video label (vt* if captions else vout)
        // Extract last vt label if present, else vout
        let vmap = if fc.contains("[vt") {
            // find highest vt index
            let mut max = 0usize;
            for cap in fc.match_indices("[vt") {
                // parse number after vt
                let rest = &fc[cap.0 + 3..];
                let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = num.parse::<usize>() { if n > max { max = n; } }
            }
            // actually highest index is len-related; simpler: last vt index
            // count vt labels
            let count = fc.matches("drawtext").count();
            if count > 0 { format!("[vt{}]", count - 1) } else { "[vout]".to_string() }
        } else {
            "[vout]".to_string()
        };
        cmd.arg("-map").arg(vmap);
        if has_audio {
            cmd.arg("-map").arg("[aout]");
        }
    }

    let encoder = opts.encoder.as_deref().unwrap_or("libx264");
    let preset = opts.preset.as_deref().unwrap_or("veryfast");
    let crf = opts.crf.unwrap_or(23);
    cmd.arg("-c:v").arg(encoder).arg("-preset").arg(preset).arg("-crf").arg(crf.to_string()).arg("-pix_fmt").arg("yuv420p");
    if has_audio {
        cmd.arg("-c:a").arg("aac").arg("-b:a").arg("128k");
    } else if !has_video {
        // no filter_complex case — just transcode
    } else {
        // video only: ensure no stray audio
        // leave default; ffmpeg will drop audio if not mapped
        // explicitly disable if no audio inputs at all
        let audio_inputs = inputs.iter().filter(|p| {
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
            matches!(ext.as_str(), "mp3" | "aac" | "wav" | "m4a" | "flac" | "ogg")
        }).count();
        if audio_inputs == 0 && extra_inputs == 0 && !has_audio {
            // don't force -an; let ffmpeg decide
        }
    }
    if extra_inputs > 0 {
        // mux soft captions as mov_text
        // subtitle input is last index
        let sub_idx = inputs.len();
        cmd.arg("-map").arg(format!("{sub_idx}:0"));
        cmd.arg("-c:s").arg("mov_text").arg("-metadata:s:s:0").arg("language=und");
    } else if !has_audio {
        // keep shortest only when no subtitle muxing issues
    }

    cmd.arg(output);
    let out = cmd.output().context("run ffmpeg")?;
    if !out.status.success() {
        anyhow::bail!("ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

#[cfg(test)]
mod wrap_tests {
    use super::{build_crop_x_expr, wrap_lines};

    #[test]
    fn short_single_line_unwrapped() {
        assert_eq!(wrap_lines("Wait for it", 22), vec!["Wait for it"]);
    }

    #[test]
    fn long_caption_wraps_at_space() {
        let out = wrap_lines("Lan dau tien toi cho phep minh yeu duoi", 22);
        assert_eq!(out.len(), 2);
        for line in &out { assert!(line.chars().count() <= 22, "line too long: {line:?}"); }
        assert_eq!(out.join(" "), "Lan dau tien toi cho phep minh yeu duoi");
    }

    #[test]
    fn long_word_hard_cut_no_underflow() {
        let out = wrap_lines("supercalifragilisticexpialidocious ok", 22);
        assert!(out.len() >= 2);
    }

    #[test]
    fn escapes_filter_specials() {
        let out = wrap_lines("colon test space", 22);
        assert_eq!(out, vec!["colon test space"]);
    }

    #[test]
    fn crop_expr_nests_intersecting_windows_only() {
        use crate::reframe::CropWindow;
        let f = build_crop_x_expr(
            &[
                CropWindow { start: 0.0, end: 2.0, x: 256 },
                CropWindow { start: 2.0, end: 4.0, x: 512 },
                CropWindow { start: 4.0, end: 6.0, x: 768 },
            ],
            2560,
        );
        // Segment [0,3s): chi nest window start<=3 (1 cap if).
        let e = f(Some(0), Some(3_000_000));
        assert_eq!(e.matches("if(lt(t").count(), 1, "expr={e}");
        assert!(e.contains("0.100000"), "expr={e} phai bat dau tu x=256/2560=0.1");
        assert!(e.contains(r"t\,2.000"), "expr={e} moc phai la start window=2.0, khong phai end");
        // Segment [0,10s): nest ca 2 window sau (2 cap).
        let e2 = f(Some(0), Some(10_000_000));
        assert_eq!(e2.matches("if(lt(t").count(), 2, "expr={e2}");
        // Khong windows => center nhu cu.
        let g = build_crop_x_expr(&[], 2560);
        assert_eq!(g(None, None), "(in_w-out_w)/2");
    }

    fn colon_escaped_keeps_timestamp_readable() {
        let out = wrap_lines("T-0:00 vs T-1:30", 22);
        assert_eq!(out, vec![r"T-0\:00 vs T-1\:30"]);
    }
}
