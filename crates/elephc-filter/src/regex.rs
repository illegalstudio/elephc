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
//! - Compiled programs fail with an explicit missing-`pcre2` diagnostic when regexp
//!   validation reaches the bridge without the managed matcher; unit tests keep the
//!   pure-Rust fallback.

use std::sync::OnceLock;

use fancy_regex::{Regex, RegexBuilder};
use regex::bytes::RegexBuilder as BytesRegexBuilder;

/// php-src `regexp1`, the ASCII email validator.
const EMAIL_ASCII: &str = r#"/^(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){255,})(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){65,}@)(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F]|(?:\x5C[\x00-\x7F]))*\x22))(?:\.(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F]|(?:\x5C[\x00-\x7F]))*\x22)))*@(?:(?:(?!.*[^.]{64,})(?:(?:(?:xn--)?[a-z0-9]+(?:-+[a-z0-9]+)*\.){1,126}){1,}(?:(?:[a-z][a-z0-9]*)|(?:(?:xn--)[a-z0-9]+))(?:-+[a-z0-9]+)*)|(?:\[(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){7})|(?:(?!(?:.*[a-f0-9][:\]]){7,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?)))|(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){5}:)|(?:(?!(?:.*[a-f0-9]:){5,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3}:)?)))?(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))(?:\.(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))){3}))\]))$/iD"#;

/// php-src `regexp0`, the Unicode email validator.
const EMAIL_UNICODE: &str = r#"/^(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){255,})(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){65,}@)(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E\pL\pN]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F\pL\pN]|(?:\x5C[\x00-\x7F]))*\x22))(?:\.(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E\pL\pN]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F\pL\pN]|(?:\x5C[\x00-\x7F]))*\x22)))*@(?:(?:(?!.*[^.]{64,})(?:(?:(?:xn--)?[a-z0-9]+(?:-+[a-z0-9]+)*\.){1,126}){1,}(?:(?:[a-z][a-z0-9]*)|(?:(?:xn--)[a-z0-9]+))(?:-+[a-z0-9]+)*)|(?:\[(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){7})|(?:(?!(?:.*[a-f0-9][:\]]){7,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?)))|(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){5}:)|(?:(?!(?:.*[a-f0-9]:){5,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3}:)?)))?(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))(?:\.(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))){3}))\]))$/iDu"#;

/// Splits a PHP `/pattern/mods` literal into its body and modifier string.
///
/// PHP uses the first unescaped closing delimiter; everything after it must be a known PCRE
/// modifier. Paired delimiters (`()`, `{}`, `[]`, `<>`) keep their matching nesting.
fn split_php_pattern(pattern: &str) -> Result<(&str, &str), String> {
    let pattern = pattern.trim_start();
    let delimiter = pattern.chars().next().ok_or_else(|| "Empty regular expression".to_string())?;
    if delimiter.is_ascii_alphanumeric() || delimiter == '\\' || delimiter == '\0' || delimiter.is_whitespace() {
        return Err("Delimiter must not be alphanumeric, backslash, or NUL byte".to_string());
    }
    let paired = match delimiter {
        '(' => Some(')'),
        '{' => Some('}'),
        '[' => Some(']'),
        '<' => Some('>'),
        _ => None,
    };
    let mut depth = 1usize;
    let mut escaped = false;
    let closing = (|| {
        for (index, ch) in pattern.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            match paired {
                Some(_) if ch == delimiter => depth += 1,
                Some(close) if ch == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                None if ch == delimiter => return Some(index),
                _ => {}
            }
        }
        None
    })()
    .ok_or_else(|| match paired {
        Some(close) => format!("No ending matching delimiter '{close}' found"),
        None => format!("No ending delimiter '{delimiter}' found"),
    })?;
    let body_start = delimiter.len_utf8();
    let close_len = paired.map_or(delimiter.len_utf8(), |close| close.len_utf8());
    let modifiers = &pattern[closing + close_len..];
    if let Some(flag) = modifiers
        .chars()
        .find(|flag| !"imsxADSXUJunr".contains(*flag) && !matches!(*flag, ' ' | '\n' | '\r'))
    {
        return Err(format!("Unknown modifier '{flag}'"));
    }
    Ok((&pattern[body_start..closing], modifiers))
}

/// Byte-oriented sibling of [`split_php_pattern`] for patterns that are not valid UTF-8.
///
/// PHP keeps non-`u` patterns byte-exact, but their delimiters and modifiers are ASCII. This
/// validates the spelling before the native matcher sees it, so an unknown modifier still
/// reports `Unknown modifier 'q'` instead of being silently ignored by the runtime scan.
fn split_php_pattern_bytes(pattern: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let mut start = 0usize;
    while start < pattern.len() && pattern[start].is_ascii_whitespace() { start += 1; }
    let pattern = &pattern[start..];
    let Some(&delimiter) = pattern.first() else {
        return Err("Empty regular expression".to_string());
    };
    if delimiter.is_ascii_alphanumeric() || delimiter == b'\\' || delimiter == 0 || delimiter.is_ascii_whitespace() {
        return Err("Delimiter must not be alphanumeric, backslash, or NUL byte".to_string());
    }
    let paired = match delimiter {
        b'(' => Some(b')'),
        b'{' => Some(b'}'),
        b'[' => Some(b']'),
        b'<' => Some(b'>'),
        _ => None,
    };
    let mut depth = 1usize;
    let mut escaped = false;
    let mut closing = None;
    for (index, &byte) in pattern.iter().enumerate().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        match paired {
            Some(_) if byte == delimiter => depth += 1,
            Some(close) if byte == close => {
                depth -= 1;
                if depth == 0 {
                    closing = Some(index);
                    break;
                }
            }
            None if byte == delimiter => {
                closing = Some(index);
                break;
            }
            _ => {}
        }
    }
    let closing = closing.ok_or_else(|| match paired {
        Some(close) => format!("No ending matching delimiter '{}' found", close as char),
        None => format!("No ending delimiter '{}' found", delimiter as char),
    })?;
    let close_len = if paired.is_some() { 1 } else { 1 };
    let modifiers = &pattern[closing + close_len..];
    for &flag in modifiers {
        // PHP permits spaces, LF and CR between the closing delimiter and modifiers; a tab is
        // still an unknown modifier and is reported like any other byte.
        if matches!(flag, b' ' | b'\n' | b'\r') {
            continue;
        }
        let known = matches!(flag, b'i' | b'm' | b's' | b'x' | b'A' | b'D' | b'S' | b'X' | b'U' | b'J' | b'u' | b'n' | b'r');
        if !known {
            return Err(format!("Unknown modifier '{}'", flag as char));
        }
    }
    Ok((&pattern[1..closing], modifiers))
}

/// Rewrites PCRE's `$` anchor to its precise end-of-subject semantics.
///
/// PCRE `$` matches at the end or immediately before a final newline; `D` restricts it to the
/// end. Rust's regex engine implements only the strict-end form, so the body needs this pass.
/// Inline modifier groups can change `m` for a scope (`(?-m:$)`) or for the rest of the
/// enclosing group (`(?m)`), so the anchor decision follows those scopes instead of only the
/// outer modifier list.
fn rewrite_dollar_anchors(body: &str, dollar_end_only: bool, multiline: bool) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    let mut escaped = false;
    let mut in_class = false;
    // The current and enclosing groups' multiline state. A setting group updates the current
    // entry; a scoped group pushes the state its closing parenthesis restores.
    let mut scopes = vec![multiline];
    while let Some(ch) = chars.next() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        if in_class {
            match ch {
                '\\' => {
                    out.push(ch);
                    escaped = true;
                }
                ']' => {
                    in_class = false;
                    out.push(ch);
                }
                _ => out.push(ch),
            }
            continue;
        }
        match ch {
            '\\' => {
                out.push(ch);
                escaped = true;
            }
            '[' => {
                in_class = true;
                out.push(ch);
            }
            '$' => {
                if *scopes.last().expect("multiline scope") {
                    out.push_str("(?=\\n|\\z)");
                } else if dollar_end_only {
                    out.push_str("\\z");
                } else {
                    out.push_str("(?=\\n?\\z)");
                }
            }
            '(' => {
                let current = *scopes.last().expect("multiline scope");
                if let Some((suffix, scoped, changed)) = inline_flag_group(&mut chars, current) {
                    out.push('(');
                    out.push_str(&suffix);
                    if scoped {
                        scopes.push(changed);
                    } else {
                        *scopes.last_mut().expect("multiline scope") = changed;
                    }
                } else {
                    out.push('(');
                    scopes.push(current);
                }
            }
            ')' => {
                if scopes.len() > 1 {
                    scopes.pop();
                }
                out.push(')');
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Reads an inline PCRE modifier group starting after `(`.
///
/// Returns the consumed `?flags` suffix with its `:` or `)` terminator, whether the group is
/// scoped (`:`), and the multiline state its flags select. A group that is not a plain inline
/// modifier setting returns `None` and is treated as an ordinary nesting group.
fn inline_flag_group(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    current_multiline: bool,
) -> Option<(String, bool, bool)> {
    let mut look = chars.clone();
    if look.next()? != '?' {
        return None;
    }
    let mut suffix = String::from("?");
    let mut multiline = current_multiline;
    let mut negating = false;
    let mut saw_flag = false;
    loop {
        let flag = look.next()?;
        match flag {
            ')' | ':' => {
                if !saw_flag {
                    return None;
                }
                suffix.push(flag);
                break;
            }
            '-' => {
                if negating {
                    return None;
                }
                negating = true;
                suffix.push(flag);
            }
            'i' | 'm' | 's' | 'x' | 'U' | 'J' | 'X' | 'n' | 'r' => {
                saw_flag = true;
                if flag == 'm' {
                    multiline = !negating;
                }
                suffix.push(flag);
            }
            _ => return None,
        }
    }
    for _ in 0..suffix.chars().count() {
        chars.next();
    }
    let scoped = suffix.ends_with(':');
    Some((suffix, scoped, multiline))
}

/// Builds one fancy-regex from a PHP pattern body and its modifiers.
fn build_fancy(body: &str, modifiers: &str) -> Option<Regex> {
    let source = build_source(body, modifiers);
    RegexBuilder::new(&source).build().ok()
}

/// Builds one fancy-regex for byte-mode patterns that need lookarounds.
///
/// The caller maps every input byte to one Latin-1 scalar before matching, so `.`,
/// character classes and byte escapes keep their per-byte width.
fn build_mapped_fancy(body: &str, modifiers: &str) -> Option<Regex> {
    let source = build_source(body, modifiers);
    let source = rewrite_byte_mode_classes(&source);
    RegexBuilder::new(&source).build().ok()
}

/// Replaces PHP's byte-mode shorthand classes with their ASCII equivalents.
///
/// `build_mapped_fancy` maps each input byte to one Latin-1 scalar, so the regex engine's
/// default `\w`, `\d`, `\s` and `\b` would use Unicode classes (for example `\w` would
/// match `0xE9`). PCRE without the `u` modifier uses ASCII classes, and this rewrite keeps
/// that contract on the lookaround path. Escapes inside a character class use their
/// class-body spelling, while the negated forms are only rewritten outside a class, where a
/// whole negated class is the direct ASCII equivalent.
fn rewrite_byte_mode_classes(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_class = false;
    // A `]` (or `^` before it) is a literal at the start of a class, and POSIX bracket
    // expressions such as `[:alpha:]` contain a `]` that does not close the outer class.
    let mut class_first = false;
    while let Some(ch) = chars.next() {
        if in_class {
            if class_first {
                class_first = false;
                if ch == '^' {
                    out.push(ch);
                    class_first = true;
                    continue;
                }
                if ch == ']' {
                    out.push(ch);
                    continue;
                }
            }
            if ch == '\\' {
                let Some(next) = chars.next() else {
                    out.push('\\');
                    break;
                };
                match next {
                    'd' => out.push_str("0-9"),
                    'w' => out.push_str("A-Za-z0-9_"),
                    's' => out.push_str(" \\t\\n\\r\\f\\v"),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
                continue;
            }
            if ch == '[' {
                if let Some(kind @ (':' | '.' | '=')) = chars.peek().copied() {
                    out.push('[');
                    out.push(chars.next().expect("peeked bracket-expression kind"));
                    while let Some(inner) = chars.next() {
                        out.push(inner);
                        if inner == kind && chars.peek() == Some(&']') {
                            out.push(chars.next().expect("peeked bracket-expression end"));
                            break;
                        }
                    }
                    continue;
                }
            }
            if ch == ']' {
                in_class = false;
            }
            out.push(ch);
            continue;
        }
        match ch {
            '\\' => {
                let Some(next) = chars.next() else {
                    out.push('\\');
                    break;
                };
                match next {
                    'd' => out.push_str("[0-9]"),
                    'D' => out.push_str("[^0-9]"),
                    'w' => out.push_str("[A-Za-z0-9_]"),
                    'W' => out.push_str("[^A-Za-z0-9_]"),
                    's' => out.push_str("[ \\t\\n\\r\\f\\v]"),
                    'S' => out.push_str("[^ \\t\\n\\r\\f\\v]"),
                    'b' => out.push_str(
                        "(?:(?<=[A-Za-z0-9_])(?![A-Za-z0-9_])|(?<![A-Za-z0-9_])(?=[A-Za-z0-9_]))",
                    ),
                    'B' => out.push_str(
                        "(?:(?<=[A-Za-z0-9_])(?=[A-Za-z0-9_])|(?<![A-Za-z0-9_])(?![A-Za-z0-9_]))",
                    ),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            }
            '[' => {
                in_class = true;
                class_first = true;
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Builds one byte-oriented regex for PHP's non-`u` byte semantics.
fn build_bytes(body: &str, modifiers: &str) -> Option<regex::bytes::Regex> {
    let source = build_source(body, modifiers);
    BytesRegexBuilder::new(&source).unicode(false).build().ok()
}

/// Applies PHP's modifier flags and `$` rewrite to a pattern body.
fn build_source(body: &str, modifiers: &str) -> String {
    let mut inline = String::new();
    for flag in ['i', 'm', 's', 'x'] {
        if modifiers.contains(flag) {
            inline.push(flag);
        }
    }
    let dollar_end_only = modifiers.contains('D');
    let multiline = modifiers.contains('m');
    let mut source = rewrite_dollar_anchors(body, dollar_end_only, multiline);
    if modifiers.contains('A') {
        // `A` anchors the WHOLE pattern. Prepending `\A` bare would bind only the first
        // alternative (`\Afoo|bar`), letting `/foo|bar/A` match `evilbar`.
        source = format!("\\A(?:{source})");
    }
    if !inline.is_empty() {
        source = format!("(?{inline}){source}");
    }
    source
}

// PCRE2 diagnostics and matching provided by the generated runtime when regex is linked.
#[cfg(not(test))]
extern "C" {
    fn __rt_filter_pcre2_compile_check(
        pattern: *const u8,
        length: u64,
        options: u32,
        message: *mut u8,
        capacity: u64,
        offset_out: *mut u64,
    ) -> i32;

    fn __rt_filter_preg_match(
        pattern: *const u8,
        pattern_length: u64,
        subject: *const u8,
        subject_length: u64,
    ) -> i32;
}

/// Test-only fallback: unit tests do not link the generated program runtime.
#[cfg(test)]
unsafe fn __rt_filter_pcre2_compile_check(
    _pattern: *const u8,
    _length: u64,
    _options: u32,
    _message: *mut u8,
    _capacity: u64,
    _offset_out: *mut u64,
) -> i32 {
    -1
}

/// Test-only fallback: unit tests do not link the generated program runtime.
#[cfg(test)]
unsafe fn __rt_filter_preg_match(
    _pattern: *const u8,
    _pattern_length: u64,
    _subject: *const u8,
    _subject_length: u64,
) -> i32 {
    -1
}

/// Maps PHP's PCRE modifiers onto the compile flags needed for diagnostics.
///
/// Byte-oriented so the same mapping serves UTF-8 and raw-byte patterns.
fn pcre2_options_bytes(modifiers: &[u8]) -> u32 {
    let mut options = 0u32;
    if modifiers.contains(&b'i') { options |= 0x0000_0008; }
    if modifiers.contains(&b'm') { options |= 0x0000_0400; }
    if modifiers.contains(&b's') { options |= 0x0000_0020; }
    if modifiers.contains(&b'x') { options |= 0x0000_0080; }
    if modifiers.contains(&b'A') { options |= 0x8000_0000; }
    if modifiers.contains(&b'D') { options |= 0x0000_0010; }
    if modifiers.contains(&b'U') { options |= 0x0004_0000; }
    if modifiers.contains(&b'J') { options |= 0x0000_0040; }
    if modifiers.contains(&b'n') { options |= 0x0000_2000; }
    if modifiers.contains(&b'u') { options |= 0x0008_0000 | 0x0002_0000; }
    // `r` is PCRE2_EXTRA_CASELESS_RESTRICT, carried through a private bit the shim
    // translates into the compile context's extra options.
    if modifiers.contains(&b'r') { options |= 0x0800_0000; }
    options
}

/// PCRE2's answer for one pattern body when the native provider is linked.
enum Pcre2Check {
    Unavailable,
    Valid,
    Invalid(String),
}

/// Runs PCRE2's compile-only check and formats PHP's `Compilation failed` suffix.
fn pcre2_check(body: &str, modifiers: &str) -> Pcre2Check {
    pcre2_check_bytes(body.as_bytes(), modifiers.as_bytes())
}

/// Byte-oriented [`pcre2_check`] for patterns that are not valid UTF-8.
fn pcre2_check_bytes(body: &[u8], modifiers: &[u8]) -> Pcre2Check {
    let mut message = [0u8; 256];
    let mut offset = 0u64;
    let length = unsafe {
        __rt_filter_pcre2_compile_check(
            body.as_ptr(),
            body.len() as u64,
            pcre2_options_bytes(modifiers),
            message.as_mut_ptr(),
            message.len() as u64,
            &mut offset,
        )
    };
    if length < 0 {
        return Pcre2Check::Unavailable;
    }
    if length == 0 {
        return Pcre2Check::Valid;
    }
    let text = String::from_utf8_lossy(&message[..length as usize]);
    Pcre2Check::Invalid(format!("Compilation failed: {text} at offset {offset}"))
}

/// Matches one value and reports PHP-level pattern spelling or compilation errors separately.
pub(crate) fn php_pattern_with_error(
    value: &[u8],
    pattern: &[u8],
) -> (bool, Option<String>) {
    // PHP keeps non-`u` patterns byte-exact. A raw byte pattern cannot go through the
    // UTF-8-based fallback splitter, but its delimiter and modifiers are still ASCII and must
    // be validated before PCRE2's lenient runtime scan sees them.
    let Ok(pattern) = std::str::from_utf8(pattern) else {
        // The byte splitter strips only leading ASCII whitespace before it reads the
        // delimiter; trailing bytes stay for the modifier checker.
        let pattern = pattern.trim_ascii_start();
        let (body, modifiers) = match split_php_pattern_bytes(pattern) {
            Ok(parts) => parts,
            Err(error) => return (false, Some(error)),
        };
        if let Pcre2Check::Invalid(error) = pcre2_check_bytes(body, modifiers) {
            return (false, Some(error));
        }
        return match preg_match_via_runtime(pattern, value) {
            Some(matched) => (matched, None),
            None => (false, pcre2_missing_error()),
        };
    };
    // PHP accepts whitespace before the opening delimiter. The UTF-8 splitter strips only
    // that leading run, so hand the runtime the same normalized pattern.
    let pattern = pattern.trim_start();
    let (body, modifiers) = match split_php_pattern(pattern) {
        Ok(parts) => parts,
        Err(error) => return (false, Some(error)),
    };
    let pcre2 = pcre2_check(body, modifiers);
    if let Pcre2Check::Invalid(error) = &pcre2 {
        return (false, Some(error.clone()));
    }
    if let Some(matched) = preg_match_via_runtime(pattern.as_bytes(), value) {
        return (matched, None);
    }
    let pcre2_unavailable = matches!(pcre2, Pcre2Check::Unavailable);
    if pcre2_unavailable && !cfg!(test) {
        return (false, pcre2_missing_error());
    }
    if modifiers.contains('u') {
        if std::str::from_utf8(value).is_err() {
            return (false, None);
        }
        let value = String::from_utf8_lossy(value);
        return match build_fancy(body, modifiers) {
            Some(regex) => (regex.is_match(&value).unwrap_or(false), None),
            None => (false, pcre2_unavailable.then(|| "Compilation failed".to_string())),
        };
    }
    if let Some(regex) = build_bytes(body, modifiers) {
        return (regex.is_match(value), None);
    }
    // Lookarounds and backreferences need fancy-regex. Map each byte to one Latin-1 scalar
    // so its string engine still applies PHP's non-`u` byte semantics (`..` matches two
    // bytes of a UTF-8 sequence instead of one replacement character).
    if let Some(regex) = build_mapped_fancy(body, modifiers) {
        let text: String = value.iter().map(|byte| char::from(*byte)).collect();
        return (regex.is_match(&text).unwrap_or(false), None);
    }
    let value = String::from_utf8_lossy(value);
    match build_fancy(body, modifiers) {
        Some(regex) => (regex.is_match(&value).unwrap_or(false), None),
        None => (false, pcre2_unavailable.then(|| "Compilation failed".to_string())),
    }
}

/// Returns the explicit diagnostic for a production filter without the managed PCRE2 path.
///
/// Unit tests deliberately exercise the pure-Rust fallback, so they keep it through
/// `cfg!(test)`. A compiled program that reaches `FILTER_VALIDATE_REGEXP` through opaque eval
/// without static selection gets a clear failure instead of silently matching through a
/// different engine.
fn pcre2_missing_error() -> Option<String> {
    if cfg!(test) {
        None
    } else {
        Some(
            "regexp validation requires the managed pcre2 package; install it with `elephc native add pcre2`"
                .to_string(),
        )
    }
}

/// Runs PHP's shared PCRE2 `preg_match()` runtime when the managed provider is linked.
///
/// Returns `None` when the generated runtime reports its unavailable sentinel, which
/// keeps the pure-Rust fallback for programs that did not link the managed pcre2 package.
fn preg_match_via_runtime(pattern: &[u8], value: &[u8]) -> Option<bool> {
    let matched = unsafe {
        __rt_filter_preg_match(
            pattern.as_ptr(),
            pattern.len() as u64,
            value.as_ptr(),
            value.len() as u64,
        )
    };
    match matched {
        1 => Some(true),
        0 => Some(false),
        _ => None,
    }
}

/// Matches one value against a PHP-style regexp literal (test helper).
#[cfg(test)]
pub(crate) fn php_pattern(value: &[u8], pattern: &[u8]) -> bool {
    php_pattern_with_error(value, pattern).0
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
    let (body, modifiers) = split_php_pattern(pattern).ok()?;
    build_fancy(body, modifiers)
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

    /// Leading whitespace before the delimiter is normalized for matching, as PHP does.
    #[test]
    fn leading_whitespace_before_delimiter_is_accepted() {
        assert!(php_pattern(b"foo", b" /^foo$/"));
        assert!(php_pattern(b"foo", b"\t/^foo$/"));
        assert!(!php_pattern(b"bar", b" /^foo$/"));
    }

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
        assert!(php_pattern(b"DATA", b"/^data$/ i"));
        assert!(php_pattern(b"DATA", b"/^data$/\ni"));
        assert!(!php_pattern(b"DATA", b"/^data$/\ti"));
        assert!(!php_pattern(b"data\n", b"/^data$/D"));
        assert!(!php_pattern(b"data", b"/\\[/"));
    }

    /// Verifies PCRE's newline-sensitive `$` rules and first-delimiter parsing.
    #[test]
    fn regexp_dollar_and_delimiters_match_pcre() {
        assert!(php_pattern(b"foo\n", b"/^foo$/"));
        assert!(php_pattern(b"foo\n", b"/foo/D"));
        assert!(!php_pattern(b"foo\n", b"/^foo$/D"));
        assert!(!php_pattern(b"foo\r\n", b"/^foo$/"));
        assert!(!php_pattern(b"foo/bar/", b"/foo/bar/"));
        assert!(!php_pattern(b"foo", b"/foo/q"));
        assert!(php_pattern(b"a/b", b"/a\\/b/"));
        assert!(php_pattern(b"foo\nbar", b"/^bar$/m"));
        assert!(php_pattern(b"a", b" /a/ \n"));
        // Non-`u` patterns run in byte mode: two bytes match `.` twice.
        assert!(php_pattern(b"\xc3\xa9", b"/^..$/"));
        assert!(php_pattern(b"12", b"/^\\d+$/"));
    }

    /// `A` anchors every alternative, and `$` stays zero-width before a following atom.
    #[test]
    fn regexp_anchors_cover_alternatives_and_trailing_newlines() {
        assert!(!php_pattern(b"evilbar", b"/foo|bar/A"));
        assert!(php_pattern(b"bar", b"/foo|bar/A"));
        assert!(php_pattern(b"foo\n", b"/foo$\n/"));
        assert!(!php_pattern(b"foo\nbar", b"/foo$\n/"));
    }

    /// A scoped inline modifier changes the `$` rewrite for its own group only.
    #[test]
    fn regexp_scoped_multiline_dollar_matches_pcre() {
        assert!(php_pattern(b"foo\n", b"/\\Afoo(?-m:$)/m"));
        assert!(!php_pattern(b"foo\nbar", b"/\\Afoo(?-m:$)/m"));
        assert!(php_pattern(b"foo\nbar", b"/\\Afoo(?m:$)/"));
    }

    /// The lookaround fallback keeps PHP's non-`u` ASCII shorthand classes.
    #[test]
    fn regexp_byte_mode_shorthands_stay_ascii() {
        assert!(php_pattern(b"abc_123", b"/^\\w+$/"));
        assert!(!php_pattern(b"\xE9", b"/^\\w+$/"));
        assert!(php_pattern(b"\xE9", b"/^\\W$/"));
        assert!(php_pattern(b" ", b"/^\\s+$/"));
        assert!(!php_pattern(b"\xA0", b"/^\\s+$/"));
        assert!(php_pattern(b"foo", b"/\\bfoo\\b$/"));
        assert!(php_pattern(b"\xE9foo", b"/\\bfoo\\b$/"));
        // POSIX bracket expressions keep their inner `]` from closing the outer class.
        assert!(php_pattern(b"abc", b"/(?=.)[[:alpha:]\\w]+$/"));
        assert!(!php_pattern(b"\xE9", b"/(?=.)[[:alpha:]\\w]+$/"));
        assert!(php_pattern(b"]", b"/(?=.)[]]+$/"));
    }

    /// PHP names the CLOSING character of an unterminated paired-delimiter pattern.
    #[test]
    fn unterminated_paired_delimiter_names_the_closing_character() {
        for (pattern, close) in [("(abc", ')'), ("{abc", '}'), ("[abc", ']'), ("<abc", '>')] {
            let (matched, error) = php_pattern_with_error(b"abc", pattern.as_bytes());
            assert!(!matched);
            assert_eq!(error.as_deref(), Some(format!("No ending matching delimiter '{close}' found").as_str()));
        }
        let (matched, error) = php_pattern_with_error(b"abc", b"(ab\xff");
        assert!(!matched);
        assert_eq!(error.as_deref(), Some("No ending matching delimiter ')' found"));
    }

    /// A single-character delimiter keeps PHP's non-paired wording.
    #[test]
    fn unterminated_single_delimiter_keeps_its_wording() {
        let (matched, error) = php_pattern_with_error(b"abc", b"/abc");
        assert!(!matched);
        assert_eq!(error.as_deref(), Some("No ending delimiter '/' found"));
    }
}
