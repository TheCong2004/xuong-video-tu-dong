use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Một mẫu subject-score tại một thời điểm (giây) trên một cột ngang (x pixel, nguồn).
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnSample {
    pub time: f64,
    pub x: u32,
    pub score: f64,
}

/// Cửa sổ crop ngang đã chọn cho một khoảng thời gian: [start, end) -> x offset (pixel nguồn).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CropWindow {
    pub start: f64,
    pub end: f64,
    pub x: u32,
}

#[derive(Debug, Clone)]
pub struct ReframeOptions {
    /// Chiều rộng vùng đo mỗi cột (pixel nguồn). Mặc định 320.
    pub col_w: u32,
    /// Số cột quét ngang. Mặc định 8.
    pub cols: u32,
    /// Bước thời gian giữa các mẫu (giây). Mặc định 2.0.
    pub step: f64,
    /// Tốc độ pan tối đa (pixel nguồn / giây). Mặc định 400.
    pub max_pan_per_sec: f64,
    /// Làm mượt EMA alpha (0..1]. Mặc định 0.8 (bám face nhanh: alpha 0.5 để quán tính EMA kéo cửa sổ cắt 63px má trái ở 13 f22).
    pub ema_alpha: f64,
    pub ffmpeg_cmd: String,
}

impl Default for ReframeOptions {
    fn default() -> Self {
        Self {
            col_w: 320,
            cols: 8,
            step: 2.0,
            max_pan_per_sec: 400.0,
            ema_alpha: 0.8,
            ffmpeg_cmd: "ffmpeg".to_string(),
        }
    }
}

/// Parse YAVG/SATAVG từ output metadata=print:file=- .
pub fn parse_score(txt: &str) -> f64 {
    let mut y = 0.0;
    let mut s = 0.0;
    let mut h = 0.0;
    for ln in txt.lines() {
        if ln.contains("YAVG=") {
            y = ln
                .split("YAVG=")
                .nth(1)
                .and_then(|r| r.split_whitespace().next())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
        if ln.contains("SATAVG=") {
            s = ln
                .split("SATAVG=")
                .nth(1)
                .and_then(|r| r.split_whitespace().next())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
        if ln.contains("HUEAVG=") {
            h = ln
                .split("HUEAVG=")
                .nth(1)
                .and_then(|r| r.split_whitespace().next())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
    }
    skin_score(y, s, h)
}

/// Trọng số ưu tiên người: Y làm nền, SAT vừa phải cộng thêm; vùng bão hòa cực cao
/// (neon/greenscreen/tường màu) bị phạt vì signalstats SATAVG đếm cả nền — đây là
/// nguyên nhân centroid/best_window kéo cửa sổ ra khỏi mặt người (15 f22, 13 f22).
/// Trong so uu tien nguoi: Y lam nen + SAT vua phai; vung bao hoa cuc cao
/// (neon/greenscreen SAT>110) bi phat vi signalstats SATAVG dem ca nen.
/// Ghi chu do that (src t=25.5): da mat HUE~152, tuong xanh ~135 — cung dai nen
/// KHONG dung HUEAVG phan biet o cap cot; muon tach da/xanh can face detector (Pha 1).
pub fn skin_score(y: f64, sat: f64, _hue: f64) -> f64 {
    let sat_pen = if sat > 110.0 { (sat - 110.0) * 2.0 } else { 0.0 };
    (y + 0.5 * sat - sat_pen).max(0.0)
}

/// Trích điểm saliency (skin-weighted YAVG/SATAVG/HUEAVG) của một cột tại một mốc thời gian.
pub fn sample_one_column(
    video: &Path,
    col_w: u32,
    src_h: u32,
    x: u32,
    t: f64,
    ffmpeg_cmd: &str,
) -> f64 {
    let vf = format!("crop={col_w}:{src_h}:{x}:0,signalstats,metadata=print:file=-");
    let r = Command::new(ffmpeg_cmd)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{t}"),
            "-i",
            &video.to_string_lossy(),
            "-frames:v",
            "1",
            "-vf",
            &vf,
            "-f",
            "null",
            "-",
        ])
        .output();
    let mut score = 0.0;
    if let Ok(o) = r {
        let txt = String::from_utf8_lossy(&o.stdout).into_owned()
            + &String::from_utf8_lossy(&o.stderr);
        score = parse_score(&txt);
    }
    score
}

/// Chạy ffmpeg trích score trên từng cột dọc tại từng mốc thời gian.
/// Trả về Vec<(time, Vec<(x, score)>)> (độ dài = cols).
pub fn sample_columns(
    video: &Path,
    src_w: u32,
    src_h: u32,
    times: &[f64],
    opts: &ReframeOptions,
) -> Vec<(f64, Vec<(u32, f64)>)> {
    let mut out = Vec::new();
    let cols = opts.cols.max(1);
    let denom = cols.saturating_sub(1).max(1) as f64;
    for &t in times {
        let mut row = Vec::new();
        for c in 0..cols {
            let x =
                ((src_w.saturating_sub(opts.col_w)) as f64 * c as f64 / denom).round() as u32;
            let score = sample_one_column(video, opts.col_w, src_h, x, t, &opts.ffmpeg_cmd);
            row.push((x, score));
        }
        out.push((t, row));
    }
    out
}

/// Chọn tâm crop (x offset của cửa sổ 9:16) từ scores các cột:
/// trung bình trọng số vị trí cột, sau đó clamp vào [0, src_w - crop_w].
pub fn weighted_center(samples: &[(u32, f64)], crop_w: u32, src_w: u32) -> u32 {
    let max_x = src_w.saturating_sub(crop_w);
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for (x, s) in samples {
        let cx = (*x as f64 + crop_w as f64 / 2.0).min(src_w as f64);
        num += cx * s.max(0.0);
        den += s.max(0.0);
    }
    if den <= 0.0 {
        return max_x / 2;
    }
    ((num / den - crop_w as f64 / 2.0).round().max(0.0).min(max_x as f64)) as u32
}

/// Chọn x cửa sổ crop maximize tổng saliency nằm TRONG cửa sổ (overlap-weighted).
/// Khác weighted_center (center-of-mass bị kéo ra xa subject khi có 2 vùng nóng 2 bên),
/// cái này đảm bảo subject rơi trọn trong khung -> tránh cắt lẹm nhân vật (15 f22 facecam mép trái).
/// Chon x cua so crop: can giua cot nong nhat (argmax), clamp vao [0, src_w - crop_w].
/// Thay coverage-max (vong 5): coverage-max bi tuong sang ben canh face keo cua so
/// lech (13 f22: face nguon x~472-850 nhung cua so [472,1282] day mat ra mep trai,
/// nua phai khung toan vung den cua layout Zoom goc). Argmax-center dat subject
/// giua khung — dung muc tieu "focus nhan vat", hanh vi don gian, de doan.
/// col_w: chieu rong vung do cua moi mau (de tinh tam cot).
pub fn best_window(samples: &[(u32, f64)], crop_w: u32, src_w: u32, col_w: u32) -> u32 {
    let max_x = src_w.saturating_sub(crop_w);
    if samples.is_empty() || max_x == 0 {
        return max_x / 2;
    }
    // Cot nong nhat; hoa diem thi lay cot gan giua nguon hon (giam pan thua).
    let mut bi = 0usize;
    let mut bv = f64::MIN;
    for (i, (x, s)) in samples.iter().enumerate() {
        let cx = *x as f64 + col_w as f64 / 2.0;
        let score = s.max(0.0) - (cx - src_w as f64 / 2.0).abs() * 0.0001;
        if score > bv + 1e-9 {
            bv = score;
            bi = i;
        }
    }
    if bv <= 0.0 {
        return max_x / 2;
    }
    let center = samples[bi].0 as f64 + col_w as f64 / 2.0;
    (center - crop_w as f64 / 2.0).round().clamp(0.0, max_x as f64) as u32
}

/// theo từng khoảng [times[i], times[i+1]).
pub fn smooth_path(
    times: &[f64],
    xs: &[u32],
    opts: &ReframeOptions,
    max_x: u32,
) -> Vec<CropWindow> {
    let mut wins = Vec::new();
    if times.is_empty() || xs.is_empty() || times.len() != xs.len() {
        return wins;
    }
    let alpha = opts.ema_alpha.clamp(0.05, 1.0);
    let mut prev = xs[0] as f64;
    let mut sx = Vec::with_capacity(xs.len());
    for (i, &x) in xs.iter().enumerate() {
        let e = if i == 0 {
            x as f64
        } else {
            alpha * x as f64 + (1.0 - alpha) * prev
        };
        let dt = if i == 0 { 0.0 } else { (times[i] - times[i - 1]).max(0.01) };
        let max_step = opts.max_pan_per_sec * dt;
        let clamped = if i == 0 {
            e
        } else {
            (e - prev).clamp(-max_step, max_step) + prev
        };
        let clamped = clamped.clamp(0.0, max_x as f64);
        sx.push(clamped.round() as u32);
        prev = clamped;
    }
    for i in 0..times.len() {
        let end = if i + 1 < times.len() {
            times[i + 1]
        } else {
            times[i] + opts.step
        };
        wins.push(CropWindow {
            start: times[i],
            end,
            x: sx[i],
        });
    }
    wins
}

/// Ước lượng toàn bộ crop path cho một đoạn [seg_start, seg_end) của source.
pub fn estimate_crop_path(
    video: &Path,
    src_w: u32,
    src_h: u32,
    seg_start: f64,
    seg_end: f64,
    opts: &ReframeOptions,
) -> Vec<CropWindow> {
    let crop_w = ((src_h as f64 * 9.0 / 16.0).round() as u32).min(src_w);
    let max_x = src_w.saturating_sub(crop_w);
    let mut times = Vec::new();
    let mut t = seg_start;
    while t < seg_end {
        times.push(t);
        t += opts.step;
    }
    if times.is_empty() {
        times.push(seg_start);
    }
    let samples = sample_columns(video, src_w, src_h, &times, opts);
    let xs: Vec<u32> = samples
        .iter()
        .map(|(_, row)| best_window(row, crop_w, src_w, opts.col_w))
        .collect();
    smooth_path(&times, &xs, opts, max_x)
}

#[cfg(test)]
mod reframe_tests {
    use super::*;

    #[test]
    fn weighted_center_prefers_hot_column() {
        let rows = vec![(0u32, 10.0), (850u32, 10.0), (1700u32, 100.0)];
        let x = weighted_center(&rows, 810, 2560);
        assert!(x > 800, "x={x} phai huong ve cot nong ben phai");
        assert!(x <= 1750);
    }

    #[test]
    fn weighted_center_fallback_middle_on_flat() {
        let rows = vec![(0u32, 0.0), (850u32, 0.0)];
        assert_eq!(weighted_center(&rows, 810, 2560), 875);
    }

    #[test]
    fn best_window_centers_hottest_column() {
        // Do that src t=25.5 (13 f22): face o cot 640 (score cao nhat), tuong xanh
        // cot 960 ke ben. Argmax-center can giua cot face: tam cot 640+160=800,
        // x = 800-405 = 395 — face nguon [472,850] nam giua khung, khong mep trai.
        let rows = vec![
            (0u32, 29.0),
            (320u32, 44.0),
            (640u32, 116.0),
            (960u32, 107.0),
            (1280u32, 43.0),
            (1600u32, 39.0),
            (1920u32, 62.0),
            (2240u32, 61.0),
        ];
        let x = best_window(&rows, 810, 2560, 320);
        assert!((x as i64 - 395).abs() <= 5, "x={x} phai can giua cot face 640");
    }

    #[test]
    fn best_window_fallback_middle_on_flat() {
        let rows = vec![(0u32, 0.0), (850u32, 0.0)];
        assert_eq!(best_window(&rows, 810, 2560, 850), 875);
    }

    #[test]
    fn skin_score_penalizes_neon_saturation() {
        // SAT>110 (neon/greenscreen) bi phat du Y bang nhau.
        let face = skin_score(116.0, 24.0, 152.0);
        let wall = skin_score(116.0, 150.0, 135.0);
        assert!(face > wall, "face={face} wall={wall}: neon khong duong thang mat");
    }

    #[test]
    fn smooth_path_clamps_pan_velocity() {
        let opts = ReframeOptions {
            max_pan_per_sec: 100.0,
            ema_alpha: 1.0,
            ..Default::default()
        };
        let w = smooth_path(&[0.0, 1.0, 2.0], &[0, 1750, 0], &opts, 1750);
        assert_eq!(w.len(), 3);
        assert!((w[1].x as i64 - w[0].x as i64).abs() <= 100);
        assert!((w[2].x as i64 - w[1].x as i64).abs() <= 100);
    }

    #[test]
    fn smooth_path_windows_cover_samples() {
        let opts = ReframeOptions::default();
        let w = smooth_path(&[5.0, 7.0], &[300, 500], &opts, 1750);
        assert_eq!(
            w[0],
            CropWindow {
                start: 5.0,
                end: 7.0,
                x: 300
            }
        );
        assert_eq!(w[1].start, 7.0);
    }

    #[test]
    fn parse_score_sums_yavg_and_satavg() {
        let txt =
            "frame:0 pts:0 pts_time:0\nlavfi.signalstats.YAVG=111.925\nlavfi.signalstats.SATAVG=29.45\n";
        // Khong co HUEAVG -> hue=0, khong bonus.
        assert!((parse_score(txt) - (111.925 + 0.5 * 29.45)).abs() < 0.1);
    }
}
