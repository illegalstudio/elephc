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
    source.iter().take_while(|&&byte| is_label(byte)).count()
}

/// Returns the byte length of a run of digits, allowing underscores between digits.
#[inline]
pub fn digits_len(source: &[u8], radix: u32) -> usize {
    let valid = |byte: &u8| match radix {
        2 => matches!(byte, b'0' | b'1'),
        8 => matches!(byte, b'0'..=b'7'),
        16 => byte.is_ascii_hexdigit(),
        _ => byte.is_ascii_digit(),
    };
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

/// Identifies the first trivia item so PHP can preserve its original token boundary.
pub(crate) enum Trivia {
    Whitespace,
    LineComment,
    BlockComment,
}

/// Finds the first whitespace or comment run using the selected source rules.
#[inline]
fn trivia<const PHP: bool>(source: &[u8]) -> Option<(Trivia, usize)> {
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
                if byte == b'\n' || PHP && (byte == b'\r' || source[len..].starts_with(b"?>")) {
                    break;
                }
                len += 1;
            }
            Some((Trivia::LineComment, len))
        }
        _ => None,
    }
}

/// Returns the next trivia span using Zend line endings and closing-tag boundaries.
pub(crate) fn php_trivia(source: &[u8]) -> Option<(Trivia, usize)> {
    trivia::<true>(source)
}

/// Returns the leading trivia length with the compiler's source rules.
#[inline]
pub fn compiler_trivia_len(source: &[u8]) -> usize {
    let mut len = 0;
    while let Some(&byte) = source.get(len) {
        if byte.is_ascii_whitespace() {
            len += 1;
        } else if matches!(byte, b'/' | b'#') {
            let Some((_, step)) = trivia::<false>(&source[len..]) else {
                break;
            };
            len += step;
        } else {
            break;
        }
    }
    len
}
