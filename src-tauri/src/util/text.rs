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

/// Return whether `value` is a non-empty ASCII path-safe identifier (`[A-Za-z0-9_-]`).
pub fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Truncate `value` safely on a UTF-8 boundary and append an ellipsis trailer if truncated.
pub fn truncate_utf8_with_ellipsis(value: &str, max_bytes: usize) -> String {
    let prefix = truncate_utf8(value, max_bytes);
    if prefix.len() == value.len() {
        value.to_string()
    } else {
        format!("{prefix}\n…[truncated]")
    }
}

/// ASCII case-insensitive substring check without allocating lowercased copies.
pub fn contains_ascii_ignore_case(value: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let needle = needle.as_bytes();
    value
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_safe_identifier_rejects_traversal_and_empty() {
        assert!(is_safe_identifier("task_01-abc"));
        assert!(!is_safe_identifier(""));
        assert!(!is_safe_identifier("../escape"));
        assert!(!is_safe_identifier("foo/bar"));
        assert!(!is_safe_identifier("foo\\bar"));
        assert!(!is_safe_identifier("a.b"));
    }

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
