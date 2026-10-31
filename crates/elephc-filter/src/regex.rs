//! Purpose:
//! Implements `FILTER_VALIDATE_EMAIL` and `FILTER_VALIDATE_REGEXP` without a native PCRE2 link.
//!
//! Called from:
//! - `crate::elephc_filter_apply_email` and `crate::elephc_filter_apply_regexp`.
//!
//! Key details:
//! - The email patterns are the two php-src 8.5 regexes, compiled once per process with
//!   fancy-regex (lookaheads and Unicode classes included).
//! - A user regexp keeps PHP's delimiter/modifier spelling: the delimiter is stripped, and the
//!   `i`, `m`, `s` and `x` modifiers map onto the Rust engine. `D` is enforced by rejecting a
//!   final newline before matching.

use std::sync::OnceLock;

use fancy_regex::{Regex, RegexBuilder};

/// php-src `regexp1`, the ASCII email validator.
const EMAIL_ASCII: &str = r#"/^(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){255,})(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){65,}@)(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F]|(?:\x5C[\x00-\x7F]))*\x22))(?:\.(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F]|(?:\x5C[\x00-\x7F]))*\x22)))*@(?:(?:(?!.*[^.]{64,})(?:(?:(?:xn--)?[a-z0-9]+(?:-+[a-z0-9]+)*\.){1,126}){1,}(?:(?:[a-z][a-z0-9]*)|(?:(?:xn--)[a-z0-9]+))(?:-+[a-z0-9]+)*)|(?:\[(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){7})|(?:(?!(?:.*[a-f0-9][:\]]){7,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?)))|(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){5}:)|(?:(?!(?:.*[a-f0-9]:){5,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3}:)?)))?(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))(?:\.(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))){3}))\]))$/iD"#;

/// php-src `regexp0`, the Unicode email validator.
const EMAIL_UNICODE: &str = r#"/^(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){255,})(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){65,}@)(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E\pL\pN]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F\pL\pN]|(?:\x5C[\x00-\x7F]))*\x22))(?:\.(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E\pL\pN]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F\pL\pN]|(?:\x5C[\x00-\x7F]))*\x22)))*@(?:(?:(?!.*[^.]{64,})(?:(?:(?:xn--)?[a-z0-9]+(?:-+[a-z0-9]+)*\.){1,126}){1,}(?:(?:[a-z][a-z0-9]*)|(?:(?:xn--)[a-z0-9]+))(?:-+[a-z0-9]+)*)|(?:\[(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){7})|(?:(?!(?:.*[a-f0-9][:\]]){7,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?)))|(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){5}:)|(?:(?!(?:.*[a-f0-9]:){5,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3}:)?)))?(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))(?:\.(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))){3}))\]))$/iDu"#;

/// Splits a PHP `/pattern/mods` literal into its body and modifier string.
fn split_php_pattern(pattern: &str) -> Option<(&str, &str)> {
    let delimiter = pattern.chars().next()?;
    if delimiter.is_ascii_alphanumeric() || delimiter == '\\' || delimiter.is_whitespace() {
        return None;
    }
    let mut closing = None;
    let mut escaped = false;
    for (index, ch) in pattern.char_indices().skip(1) {
        if escaped { escaped = false; continue; }
        if ch == '\\' { escaped = true; continue; }
        if ch == delimiter { closing = Some(index); }
    }
    let closing = closing?;
    Some((&pattern[1..closing], &pattern[closing + 1..]))
}

/// Builds one fancy-regex from a PHP pattern body and its modifiers.
fn build(body: &str, modifiers: &str) -> Option<Regex> {
    let mut inline = String::new();
    for flag in ['i', 'm', 's', 'x'] {
        if modifiers.contains(flag) { inline.push(flag); }
    }
    let source = if inline.is_empty() { body.to_string() } else { format!("(?{inline}){body}") };
    RegexBuilder::new(&source).build().ok()
}

/// Matches one value against a PHP-style regexp literal.
pub(crate) fn php_pattern(value: &[u8], pattern: &[u8]) -> bool {
    let Ok(pattern) = std::str::from_utf8(pattern) else { return false; };
    let Some((body, modifiers)) = split_php_pattern(pattern) else { return false; };
    if modifiers.contains('D') && value.last().is_some_and(|byte| matches!(byte, b'\n' | b'\r')) {
        return false;
    }
    let value = String::from_utf8_lossy(value);
    let Some(regex) = build(body, modifiers) else { return false; };
    regex.is_match(&value).unwrap_or(false)
}

/// Returns php-src's compiled ASCII email validator.
fn email_ascii() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| compile_email(EMAIL_ASCII).expect("php-src ASCII email pattern must compile"))
}

/// Returns php-src's compiled Unicode email validator.
fn email_unicode() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| compile_email(EMAIL_UNICODE).expect("php-src Unicode email pattern must compile"))
}

/// Compiles one hard-coded php-src email pattern.
fn compile_email(pattern: &str) -> Option<Regex> {
    let (body, modifiers) = split_php_pattern(pattern)?;
    build(body, modifiers)
}

/// Runs `FILTER_VALIDATE_EMAIL` against one binary-safe value.
pub(crate) fn email(value: &[u8], unicode: bool) -> bool {
    if value.len() > 320 { return false; }
    if value.last().is_some_and(|byte| matches!(byte, b'\n' | b'\r')) { return false; }
    if unicode {
        let Ok(value) = std::str::from_utf8(value) else { return false; };
        email_unicode().is_match(value).unwrap_or(false)
    } else {
        let value = String::from_utf8_lossy(value);
        email_ascii().is_match(&value).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies php-src's representative email corpus.
    #[test]
    fn email_matches_php_corpus() {
        eprintln!("a@b.c => {:?}", email_ascii().is_match("a@b.c"));
        for value in ["a@b.c", "abuse@example.com", "firstname.lastname@employee.2something.com"] {
            assert!(email(value.as_bytes(), false), "{value}");
        }
        for value in ["test@@#$%^&*())).com", "test@.com", "test@com", "@", "foo@-.com"] {
            assert!(!email(value.as_bytes(), false), "{value}");
        }
        assert!(!email(b"test@example.com\n", false));
    }

    /// Verifies delimiter/modifier parsing and the `D` end anchor.
    #[test]
    fn regexp_matches_php_shape() {
        assert!(php_pattern(b"data", b"/^d(.*)/"));
        assert!(!php_pattern(b"data", b"/^b(.*)/"));
        assert!(php_pattern(b"DATA", b"/^data$/i"));
        assert!(!php_pattern(b"data\n", b"/^data$/D"));
        assert!(!php_pattern(b"data", b"/\\[/"));
    }
}
