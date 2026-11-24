//! Purpose:
//! Recognizes string delimiters, interpolation expressions, and heredoc labels.
//!
//! Called from:
//! - The PHP tokenizer, compiler lexer, and Magician eval lexer.
//!
//! Key details:
//! - Offsets are byte lengths. Callers own token construction and source location reporting.
//! - Source profiles keep their existing newline and close-tag rules.

use super::keywords::{is_label, is_label_start};
use super::lexeme::identifier_len;

/// Returns the byte length through the closing quote, starting immediately after the opening quote.
pub fn quoted_len(source: &[u8], quote: u8) -> Option<usize> {
    let mut at = 0;
    while let Some(&byte) = source.get(at) {
        if byte == quote {
            return Some(at + 1);
        }
        at += if byte == b'\\' { 2 } else { 1 };
    }
    None
}

/// Decodes a single-quoted body into the compiler's byte-preserving literal representation.
pub fn decode_single_quoted(body: &str) -> String {
    use elephc_builtin_contract::string_literal::push_literal_char;

    let mut decoded = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && matches!(chars.peek(), Some('\\' | '\'')) {
            decoded.push(chars.next().expect("peeked escape"));
        } else {
            push_literal_char(ch, &mut decoded);
        }
    }
    decoded
}

/// The two brace-based interpolation spellings.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BracedInterpolation {
    /// The opening brace precedes the variable.
    CurlyDollar,
    /// The dollar precedes the opening brace.
    DollarCurly,
}

/// Recognizes a brace interpolation marker at the start of the byte slice.
#[inline]
pub fn braced_interpolation(source: &[u8]) -> Option<BracedInterpolation> {
    match source {
        [b'{', b'$', ..] => Some(BracedInterpolation::CurlyDollar),
        [b'$', b'{', ..] => Some(BracedInterpolation::DollarCurly),
        _ => None,
    }
}

/// How a line comment inside a braced expression treats a PHP closing tag.
#[derive(Clone, Copy)]
pub enum InterpolationComment {
    /// Leave the tag to the compiler's expression scanner.
    EndAtCloseTag,
    /// Reject a tag in interpreted fragments.
    RejectCloseTag,
    /// Count anything before the newline as comment text when measuring indentation.
    UntilNewline,
}

/// Why a braced expression could not be scanned.
#[derive(Clone, Copy, Debug)]
pub enum InterpolationError {
    Expression,
    Quote,
    BlockComment,
    CloseTag,
}

/// Returns the byte length through the matching brace, starting after the opening brace.
pub fn braced_expression_len(
    source: &[u8],
    comments: InterpolationComment,
) -> Result<usize, InterpolationError> {
    let mut at = 0;
    let mut depth = 1;
    while let Some(&byte) = source.get(at) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(at + 1);
                }
            }
            quote @ (b'\'' | b'"') => {
                at += 1 + quoted_len(&source[at + 1..], quote).ok_or(InterpolationError::Quote)?;
                continue;
            }
            b'/' if source[at..].starts_with(b"/*") => {
                // Eval's fragment lexer reports an overlapping opener and closer as an
                // unterminated comment after the interpolation capture.
                let opener = if matches!(comments, InterpolationComment::RejectCloseTag) { 1 } else { 2 };
                let body = &source[at + opener..];
                let closer = body.windows(2).position(|pair| pair == b"*/")
                    .ok_or(InterpolationError::BlockComment)?;
                at += opener + closer + 2;
                continue;
            }
            b'/' | b'#' if source[at..].starts_with(b"//")
                || byte == b'#' && !source[at..].starts_with(b"#[") =>
            {
                at += if byte == b'/' { 2 } else { 1 };
                while let Some(&comment_byte) = source.get(at) {
                    if comment_byte == b'?' && source.get(at + 1) == Some(&b'>') {
                        match comments {
                            InterpolationComment::EndAtCloseTag => break,
                            InterpolationComment::RejectCloseTag => return Err(InterpolationError::CloseTag),
                            InterpolationComment::UntilNewline => {}
                        }
                    }
                    at += 1;
                    if comment_byte == b'\n' {
                        break;
                    }
                }
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    Err(InterpolationError::Expression)
}

/// Line ending and end-of-input behavior for heredoc labels.
#[derive(Clone, Copy)]
pub enum HeredocProfile {
    /// Zend accepts CR alone after an opening label and needs a byte after the closing label.
    Zend,
    /// The compiler requires LF or CRLF at the opener and accepts a closing label at EOF.
    Compiler,
}

/// Source ranges for an opening heredoc or nowdoc label.
pub struct HeredocOpening<'a> {
    /// Raw label bytes between any quotes.
    pub label: &'a [u8],
    /// The full byte length through the opening line ending.
    pub len: usize,
    /// Whether interpolation is disabled by a single-quoted label.
    pub nowdoc: bool,
}

/// Why a heredoc opener could not be scanned.
#[derive(Debug)]
pub enum HeredocOpeningError {
    Label,
    Quote(u8),
    Newline,
}

/// Scans the opening label and line ending from immediately after the three less-than signs.
pub fn heredoc_opening(
    source: &[u8],
    profile: HeredocProfile,
) -> Result<HeredocOpening<'_>, HeredocOpeningError> {
    let indent = horizontal_space_len(source);
    let quote = source.get(indent).copied().filter(|byte| matches!(byte, b'\'' | b'"'));
    let start = indent + usize::from(quote.is_some());
    if !source.get(start).copied().is_some_and(is_label_start) {
        return Err(HeredocOpeningError::Label);
    }
    let end = start + identifier_len(&source[start..]);
    let mut after = end;
    if let Some(quote) = quote {
        if source.get(after) != Some(&quote) {
            return Err(HeredocOpeningError::Quote(quote));
        }
        after += 1;
    }
    let newline = match &source[after..] {
        [b'\r', b'\n', ..] => 2,
        [b'\n', ..] => 1,
        [b'\r', ..] if matches!(profile, HeredocProfile::Zend) => 1,
        _ => return Err(HeredocOpeningError::Newline),
    };
    Ok(HeredocOpening { label: &source[start..end], len: after + newline, nowdoc: quote == Some(b'\'') })
}

/// Finds a closing label at a line start and returns its full length and indentation.
pub fn heredoc_closing(
    source: &[u8],
    label: &[u8],
    profile: HeredocProfile,
) -> Option<(usize, usize)> {
    let indent = horizontal_space_len(source);
    let after = source[indent..].strip_prefix(label)?;
    if after.first().copied().is_some_and(is_label)
        || after.is_empty() && matches!(profile, HeredocProfile::Zend)
    {
        return None;
    }
    Some((indent + label.len(), indent))
}

/// Counts spaces and tabs before a heredoc label.
fn horizontal_space_len(source: &[u8]) -> usize {
    source.iter().take_while(|&&byte| matches!(byte, b' ' | b'\t')).count()
}
