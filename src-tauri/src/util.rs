/// Truncate a string at a UTF-8 safe char boundary.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorter_than_max_returns_full_string() {
        assert_eq!(safe_truncate("hello", 10), "hello");
    }

    #[test]
    fn exactly_max_returns_full_string() {
        assert_eq!(safe_truncate("hello", 5), "hello");
    }

    #[test]
    fn longer_than_max_truncates() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
    }

    #[test]
    fn multibyte_utf8_does_not_split_mid_char() {
        // "héllo wörld" — 'é' is 2 bytes, 'ö' is 2 bytes
        let s = "héllo wörld";
        // Truncate at 2 bytes: 'h' is 1 byte, 'é' starts at byte 1 and is 2 bytes.
        // max_bytes=2 lands inside 'é' (byte 2 is not a char boundary), so back up to 1.
        assert_eq!(safe_truncate(s, 2), "h");
        // max_bytes=3 captures 'h' + 'é' (bytes 0..3)
        assert_eq!(safe_truncate(s, 3), "hé");
    }

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(safe_truncate("", 5), "");
    }

    #[test]
    fn max_zero_returns_empty() {
        assert_eq!(safe_truncate("hello", 0), "");
    }
}
