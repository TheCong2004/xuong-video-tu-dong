/// Ánh xạ offset của `styles[].range` trong text material CapCut.
///
/// CapCut lưu `range` theo đơn vị UTF-16 code units (tương đương `String::len`
/// trong JS). Trước v0.19.1 CLI đã nhầm là bytes UTF-16LE nên ghi offset gấp đôi
/// (#85). Module này là điểm duy nhất đọc/ghi offset, hiện tại là identity mapping
/// Port trực tiếp từ `reference/src/text-offsets.ts`.
/// Chỉ số code-unit JS -> offset CapCut lưu.
///
/// Kẹp trong `[0, text.len()]` giống cách app kẹp range.
pub fn to_stored_offset(text: &str, code_unit_idx: usize) -> usize {
    // text.length trong JS = số UTF-16 code units.
    // Trong Rust, đếm UTF-16 code units bằng `encode_utf16().count()`.
    let n = text.encode_utf16().count();
    code_unit_idx.clamp(0, n)
}

/// Offset đã lưu -> chỉ số code-unit JS.
pub fn from_stored_offset(text: &str, stored_offset: usize) -> usize {
    let n = text.encode_utf16().count();
    stored_offset.clamp(0, n)
}

/// Offset lưu trữ ngay sau ký tự cuối — end của full-span range.
pub fn stored_text_length(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `true` khi mọi range có dạng doubled (tiền 0.19.1).
///
/// Điều kiện hẹp để cho phép tự động sửa: `max(end) == 2*n`, mọi offset đều chẵn,
/// nằm trong `[0, 2n]` và là số nguyên. Draft do app tạo kết thúc ở `n` nên không
/// bao giờ bị nhận nhầm.
pub fn ranges_look_doubled(text: &str, ranges: &[[i64; 2]]) -> bool {
    let n = text.encode_utf16().count() as i64;
    if n == 0 || ranges.is_empty() {
        return false;
    }
    let mut max_end: i64 = 0;
    for r in ranges {
        let start = r[0];
        let end = r[1];
        if start % 2 != 0 || end % 2 != 0 {
            return false;
        }
        if start < 0 || end < start || end > n * 2 {
            return false;
        }
        if end > max_end {
            max_end = end;
        }
    }
    max_end == n * 2
}

/// Chia đôi các range bị doubled về code units. Trả về `None` nếu không cần sửa.
pub fn repair_doubled_ranges(text: &str, ranges: &[[i64; 2]]) -> Option<Vec<[i64; 2]>> {
    if !ranges_look_doubled(text, ranges) {
        return None;
    }
    Some(ranges.iter().map(|[s, e]| [s / 2, e / 2]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_mapping() {
        let t = "hello";
        assert_eq!(to_stored_offset(t, 2), 2);
        assert_eq!(from_stored_offset(t, 2), 2);
        assert_eq!(stored_text_length(t), 5);
    }

    #[test]
    fn clamp_past_end() {
        let t = "hi";
        assert_eq!(to_stored_offset(t, 10), 2);
    }

    #[test]
    fn doubled_detection() {
        // "hi" has 2 code units, doubled full-span would be [0,4]
        let t = "hi";
        assert!(ranges_look_doubled(t, &[[0, 4]]));
        assert!(!ranges_look_doubled(t, &[[0, 2]]));
    }

    #[test]
    fn repair() {
        let t = "hi";
        assert_eq!(repair_doubled_ranges(t, &[[0, 4]]), Some(vec![[0, 2]]));
        assert_eq!(repair_doubled_ranges(t, &[[0, 2]]), None);
    }

    #[test]
    fn empty_not_doubled() {
        assert!(!ranges_look_doubled("", &[[0, 0]]));
        assert!(!ranges_look_doubled("hi", &[]));
    }
}
