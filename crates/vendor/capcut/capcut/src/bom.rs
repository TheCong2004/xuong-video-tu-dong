/// Loại bỏ BOM UTF-8 (U+FEFF) ở đầu chuỗi nếu có.
///
/// Windows PowerShell `Set-Content` và một số editor chèn BOM vào đầu file text;
/// `serde_json::from_str` sẽ lỗi nếu không strip. Hàm này được gọi ở mọi đường
/// đọc text/JSON do người dùng cung cấp (draft, preset, subtitle, …).
/// Khi ghi file, CLI không bao giờ tạo BOM.
pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{FEFF}').unwrap_or(text)
}

/// Tương tự [`strip_bom`] nhưng trả về `String` sở hữu (owned).
pub fn strip_bom_owned(mut text: String) -> String {
    if text.starts_with('\u{FEFF}') {
        text.drain(..'\u{FEFF}'.len_utf8());
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_bom_present() {
        assert_eq!(strip_bom("\u{FEFF}hello"), "hello");
    }

    #[test]
    fn strip_bom_absent() {
        assert_eq!(strip_bom("hello"), "hello");
    }

    #[test]
    fn strip_bom_owned_present() {
        assert_eq!(strip_bom_owned("\u{FEFF}hello".to_string()), "hello");
    }

    #[test]
    fn strip_bom_empty() {
        assert_eq!(strip_bom(""), "");
    }
}
