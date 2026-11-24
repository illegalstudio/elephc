//! Purpose:
//! Recognizes lexical boundaries shared by the compiler and PHP tokenizer.
//!
//! Called from:
//! - The native PHP byte scanner and the compiler lexer.
//!
//! Key details:
//! - Callers choose source-specific trivia rules and build their own token representations.

use super::keywords::{is_label, is_whitespace};

/// Returns the byte length of the leading identifier characters, including high bytes.
#[inline]
pub fn identifier_len(source: &[u8]) -> usize {
    let mut len = 0;
    while len < source.len() && is_label(source[len]) {
        len += 1;
    }
    len
}

/// Returns the byte length of a run of digits, allowing underscores between digits.
#[inline]
pub fn digits_len(source: &[u8], radix: u32) -> usize {
    let valid = |&byte: &u8| digit_in_radix(byte, radix);
    if !source.first().is_some_and(valid) {
        return 0;
    }
    let mut end = 1;
    loop {
        if source.get(end).is_some_and(valid) {
            end += 1;
        } else if source.get(end) == Some(&b'_') && source.get(end + 1).is_some_and(valid) {
            end += 2;
        } else {
            return end;
        }
    }
}

/// Recognizes a single ASCII digit for the PHP-supported bases.
#[inline]
pub(super) fn digit_in_radix(byte: u8, radix: u32) -> bool {
    match radix {
        2 => matches!(byte, b'0' | b'1'),
        8 => matches!(byte, b'0'..=b'7'),
        16 => byte.is_ascii_hexdigit(),
        _ => byte.is_ascii_digit(),
    }
}

/// Identifies the first trivia item so PHP can preserve its original token boundary.
pub(crate) enum Trivia {
    Whitespace,
    LineComment,
    BlockComment,
}

/// Finds the first whitespace or comment run using the selected source rules.
#[inline]
fn trivia<const PHP: bool>(source: &[u8], runtime: bool) -> Option<(Trivia, usize)> {
    if runtime {
        let mut len = 0;
        while let Some(ch) = leading_char(&source[len..]).filter(|ch| ch.is_whitespace()) {
            len += ch.len_utf8();
        }
        if len != 0 {
            return Some((Trivia::Whitespace, len));
        }
    }
    let space = |byte: u8| if PHP { is_whitespace(byte) } else { byte.is_ascii_whitespace() };
    let first = *source.first()?;
    if space(first) {
        let len = source.iter().take_while(|&&byte| space(byte)).count();
        return Some((Trivia::Whitespace, len));
    }
    match source {
        [b'/', b'*', rest @ ..] => {
            let len = rest.windows(2).position(|window| window == b"*/")
                .map_or(source.len(), |index| index + 4);
            Some((Trivia::BlockComment, len))
        }
        [b'/', b'/', ..] | [b'#', ..] if !source.starts_with(b"#[") => {
            let mut len = 0;
            while let Some(&byte) = source.get(len) {
                if byte == b'\n' || PHP && ((!runtime && byte == b'\r') || source[len..].starts_with(b"?>")) {
                    break;
                }
                len += 1;
            }
            Some((Trivia::LineComment, len))
        }
        _ => None,
    }
}

/// Decodes only the leading scalar, keeping runtime trivia scanning linear.
fn leading_char(source: &[u8]) -> Option<char> {
    let prefix = &source[..source.len().min(4)];
    let text = match std::str::from_utf8(prefix) {
        Ok(text) => text,
        Err(error) => std::str::from_utf8(&prefix[..error.valid_up_to()]).ok()?,
    };
    text.chars().next()
}

/// Returns the next trivia span using Zend line endings and closing-tag boundaries.
pub(crate) fn php_trivia(source: &[u8]) -> Option<(Trivia, usize)> {
    trivia::<true>(source, false)
}

/// Returns runtime trivia with LF line counting and support for Unicode whitespace.
pub(crate) fn runtime_trivia(source: &[u8]) -> Option<(Trivia, usize)> {
    trivia::<true>(source, true)
}

/// Returns the leading trivia length with the compiler's source rules.
#[inline]
pub fn compiler_trivia_len(source: &[u8]) -> usize {
    let mut len = 0;
    while let Some(&byte) = source.get(len) {
        if byte.is_ascii_whitespace() {
            len += 1;
        } else if matches!(byte, b'/' | b'#') {
            let Some((_, step)) = trivia::<false>(&source[len..], false) else {
                break;
            };
            len += step;
        } else {
            break;
        }
    }
    len
}
