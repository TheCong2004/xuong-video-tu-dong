use crate::error::CoreError;
use crate::timeline::Us;

/// Số micro giây trong một giây.
pub const US_PER_SEC: i64 = 1_000_000;
/// Số micro giây trong một mili giây.
pub const US_PER_MS: i64 = 1_000;
/// FPS mặc định khi không truyền tham số.
pub const DEFAULT_FPS: f64 = 30.0;

/// Chuẩn hoá fps: chỉ nhận số hữu hạn > 0, ngược lại dùng [`DEFAULT_FPS`].
fn frame_rate(fps: Option<f64>) -> f64 {
    match fps {
        Some(v) if v.is_finite() && v > 0.0 => v,
        _ => DEFAULT_FPS,
    }
}

fn frames_at_rate(us: Us, rate: f64) -> i64 {
    let frames = ((us as f64 / US_PER_SEC as f64) * rate).round() as i64;
    // Math.round trả về -0 cho đầu vào âm rất nhỏ; chuẩn hoá về 0 thuần.
    // Trong Rust, round không tạo -0 cho i64, nhưng giữ logic tương đương:
    // nếu frames == 0 thì phân biệt us > 0 -> 1, ngược lại 0.
    if frames == 0 {
        if us > 0 { 1 } else { 0 }
    } else {
        frames
    }
}

/// Số frame cho một khoảng thời gian `us` ở `fps` cho trước.
///
/// Khoảng dương luôn chiếm ít nhất một frame; khoảng âm giữ làm tròn có dấu.
pub fn frames_for(us: Us, fps: Option<f64>) -> i64 {
    frames_at_rate(us, frame_rate(fps))
}

/// Lượng tử hoá `us` về frame gần nhất mà không làm khoảng dương co về 0.
pub fn quantize_to_frame(us: Us, fps: Option<f64>) -> Us {
    let rate = frame_rate(fps);
    let frames = frames_at_rate(us, rate) as f64;
    ((frames * US_PER_SEC as f64) / rate).round() as i64
}

/// Đổi giây (số thực) sang micro giây, làm tròn.
pub fn seconds_to_us(s: f64) -> Us {
    (s * US_PER_SEC as f64).round() as i64
}

/// Định dạng `us` thành `h:mm:ss.xx` hoặc `m:ss.xx` (giống TS `formatTime`).
pub fn format_time(us: Us) -> String {
    let total_sec = us as f64 / US_PER_SEC as f64;
    let h = (total_sec / 3600.0).floor() as i64;
    let m = ((total_sec % 3600.0) / 60.0).floor() as i64;
    let s = total_sec % 60.0;
    if h > 0 {
        format!("{}:{}:{}", h, pad(m), pad2(s))
    } else {
        format!("{}:{}", m, pad2(s))
    }
}

/// Định dạng `us` thành chuỗi rút gọn cho người đọc (giống TS `formatDuration`).
pub fn format_duration(us: Us) -> String {
    let s = us as f64 / US_PER_SEC as f64;
    if s < 1.0 {
        format!("{}ms", (us as f64 / US_PER_MS as f64).round() as i64)
    } else if s < 60.0 {
        format!("{:.2}s", s)
    } else {
        let m = (s / 60.0).floor() as i64;
        let rem = s % 60.0;
        format!("{}m{:.1}s", m, rem)
    }
}

/// Phân tích chuỗi thời gian thành micro giây.
///
/// Hỗ trợ các dạng:
/// - số giây trần: `"1.5"`, `"+0.5"`, `"-1"`
/// - hậu tố `s`/`ms`: `"1.5s"`, `"+500ms"`, `"-1s"`
/// - dạng `mm:ss` và `hh:mm:ss` với phần giây có thể là số thực: `"1:30"`, `"0:05.5"`, `"1:02:03.456"`
pub fn parse_time_input(input: &str) -> Result<Us, CoreError> {
    if input.is_empty() {
        return Err(CoreError::InvalidArgument(format!("Invalid time: {input}")));
    }
    let negative = input.starts_with('-');
    let clean = input
        .strip_prefix('+')
        .or_else(|| input.strip_prefix('-'))
        .unwrap_or(input);

    if clean.is_empty() {
        return Err(CoreError::InvalidArgument(format!("Invalid time: {input}")));
    }

    if let Some(num_str) = clean.strip_suffix("ms") {
        let val: f64 = num_str
            .parse()
            .map_err(|_| CoreError::InvalidArgument(format!("Invalid time: {input}")))?;
        let us = (val * US_PER_MS as f64).round() as i64;
        return Ok(if negative { -us } else { us });
    }
    if let Some(num_str) = clean.strip_suffix('s') {
        let val: f64 = num_str
            .parse()
            .map_err(|_| CoreError::InvalidArgument(format!("Invalid time: {input}")))?;
        let us = seconds_to_us(val);
        return Ok(if negative { -us } else { us });
    }
    if clean.contains(':') {
        let parts: Vec<&str> = clean.split(':').collect();
        let total_sec: f64 = if parts.len() == 3 {
            let h: f64 = parts[0]
                .parse()
                .map_err(|_| CoreError::InvalidArgument(format!("Invalid time: {input}")))?;
            let m: f64 = parts[1]
                .parse()
                .map_err(|_| CoreError::InvalidArgument(format!("Invalid time: {input}")))?;
            let s: f64 = parts[2]
                .parse()
                .map_err(|_| CoreError::InvalidArgument(format!("Invalid time: {input}")))?;
            h * 3600.0 + m * 60.0 + s
        } else if parts.len() == 2 {
            let m: f64 = parts[0]
                .parse()
                .map_err(|_| CoreError::InvalidArgument(format!("Invalid time: {input}")))?;
            let s: f64 = parts[1]
                .parse()
                .map_err(|_| CoreError::InvalidArgument(format!("Invalid time: {input}")))?;
            m * 60.0 + s
        } else {
            return Err(CoreError::InvalidArgument(format!("Invalid time: {input}")));
        };
        let us = seconds_to_us(total_sec);
        return Ok(if negative { -us } else { us });
    }
    // số trần = giây
    let val: f64 = clean
        .parse()
        .map_err(|_| CoreError::InvalidArgument(format!("Invalid time: {input}")))?;
    let us = seconds_to_us(val);
    Ok(if negative { -us } else { us })
}

fn pad(n: i64) -> String {
    format!("{n:02}")
}

fn pad2(n: f64) -> String {
    let whole = n.floor() as i64;
    let frac = n - whole as f64;
    if frac == 0.0 {
        format!("{whole:02}")
    } else {
        // (whole+frac).toFixed(2).padStart(5,"0")
        let s = format!("{:.2}", whole as f64 + frac);
        format!("{s:0>5}")
    }
}

/// Định dạng `us` thành timestamp SRT `HH:MM:SS,mmm`.
pub fn srt_time(us: Us) -> String {
    // Làm tròn về mili giây trước để tránh ",1000"
    let total_ms = (us as f64 / US_PER_MS as f64).round() as i64;
    // Xử lý giá trị âm: giữ logic làm tròn, nhưng định dạng theo giá trị tuyệt đối
    // giống TS (toán tử % trong JS giữ dấu). Để đơn giản và nhất quán, dùng
    // div_euclid / rem_euclid cho trường hợp âm sẽ cho kết quả hợp lệ.
    // Tuy nhiên TS không xử lý âm cho srtTime; ta dùng giá trị đã làm tròn và
    // tính toán trên total_ms không âm hoá nếu có thể.
    // Giữ cách tính chia lấy dư theo kiểu JS cho số dương; với số âm trả về
    // chuỗi có dấu trừ phía trước (không có trong TS nhưng hữu ích).
    let sign = if total_ms < 0 { "-" } else { "" };
    let abs_ms = total_ms.abs();
    let h = abs_ms / 3_600_000;
    let m = (abs_ms % 3_600_000) / 60_000;
    let s = (abs_ms % 60_000) / 1000;
    let ms = abs_ms % 1000;
    format!("{sign}{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

/// Định dạng `us` thành timestamp WebVTT `HH:MM:SS.mmm`.
pub fn vtt_time(us: Us) -> String {
    srt_time(us).replace(',', ".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_for_positive_small() {
        // us rất nhỏ dương -> ít nhất 1 frame ở 30fps
        assert_eq!(frames_for(1, Some(30.0)), 1);
        assert_eq!(frames_for(0, Some(30.0)), 0);
    }

    #[test]
    fn seconds_to_us_round() {
        assert_eq!(seconds_to_us(1.5), 1_500_000);
    }

    #[test]
    fn parse_bare_seconds() {
        assert_eq!(parse_time_input("1.5").unwrap(), 1_500_000);
    }

    #[test]
    fn parse_ms_suffix() {
        assert_eq!(parse_time_input("500ms").unwrap(), 500_000);
    }

    #[test]
    fn parse_colon() {
        assert_eq!(parse_time_input("1:30").unwrap(), 90_000_000);
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_time_input("abc").is_err());
    }

    #[test]
    fn srt_rounding() {
        assert_eq!(srt_time(0), "00:00:00,000");
        assert_eq!(srt_time(1_500_000), "00:00:01,500");
    }

    #[test]
    fn vtt_dot_separator() {
        assert_eq!(vtt_time(1_500_000), "00:00:01.500");
    }
}
