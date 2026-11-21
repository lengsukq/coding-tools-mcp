/// Truncate `text` to at most `max_bytes` without splitting a UTF-8 code point.
pub fn truncate_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Truncate `text` to at most `max_chars` Unicode scalar values without allocation.
pub fn truncate_chars(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

/// Return the trailing slice containing at most `max_chars` Unicode scalar values in a single reverse pass.
pub fn tail_chars(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }
    match text.char_indices().rev().nth(max_chars - 1) {
        Some((idx, _)) => &text[idx..],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_utf8_respects_multibyte_boundaries() {
        let text = "你好世界abc";
        assert_eq!(truncate_utf8(text, 0), "");
        assert_eq!(truncate_utf8(text, 2), "");
        assert_eq!(truncate_utf8(text, 3), "你");
        assert_eq!(truncate_utf8(text, 5), "你");
        assert_eq!(truncate_utf8(text, 6), "你好");
        assert_eq!(truncate_utf8(text, 100), text);
    }

    #[test]
    fn truncate_chars_and_tail_chars_handle_unicode() {
        let text = "a你b好c";
        assert_eq!(truncate_chars(text, 0), "");
        assert_eq!(truncate_chars(text, 3), "a你b");
        assert_eq!(truncate_chars(text, 10), text);

        assert_eq!(tail_chars(text, 0), "");
        assert_eq!(tail_chars(text, 3), "b好c");
        assert_eq!(tail_chars(text, 5), text);
        assert_eq!(tail_chars(text, 10), text);
    }
}
