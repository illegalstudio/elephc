//! Purpose:
//! Projects individual native scanner tokens into compiler tokens for the token view.
//!
//! Called from:
//! - `crate::syntax::tokenizer_validation::token_view::build`.
//!
//! Key details:
//! - Each helper returns `None` when a token spelling needs the text-view fallback.
//! - Projected tokens keep source spans, including the normalized cast spellings.

use crate::scanner::operator;
use crate::syntax::lexer::{
    byte_token, decode_backtick_body, decode_double_quoted_body, operator_token, scan_heredoc,
    single_quoted_token, word_token, Cursor, SpannedToken, Token, TokenMetadata,
};
use crate::syntax::span::Span;

use super::super::parser_view::void_cast_starts_statement;
use super::{tok, SourceToken};

/// One projected native token: skipped, emitted, or unsupported.
pub(super) enum Projected {
    /// Trivia, tags, or inline HTML with no compiler token.
    Skip,
    /// A compiler token plus whether source spelling metadata may be needed.
    Token(Token, bool),
}

/// Splits a qualified name using the token-start span and byte offsets.
pub(super) fn project_qualified_name_fast(id: i64, text: &str, start: Span) -> Option<Vec<SpannedToken>> {
    let mut parts = Vec::new();
    let mut offset = 0usize;
    if id == tok::T_NAME_FULLY_QUALIFIED {
        if text.as_bytes().first() != Some(&b'\\') {
            return None;
        }
        parts.push((
            Token::Backslash,
            TokenMetadata::new(offset_span(start, 0, 1)?),
        ));
        offset = 1;
    }
    let mut first = true;
    while offset < text.len() {
        let rest = &text[offset..];
        let segment_end = rest.find('\\').map_or(text.len(), |position| offset + position);
        let segment = &text[offset..segment_end];
        if segment.is_empty() {
            return None;
        }
        let segment_span = offset_span(start, offset, segment.len())?;
        let relative_prefix = id == tok::T_NAME_RELATIVE && first;
        let token = if relative_prefix {
            if !segment.eq_ignore_ascii_case("namespace") {
                return None;
            }
            Token::Namespace
        } else {
            Token::Identifier(segment.to_owned())
        };
        let metadata = if relative_prefix && token.canonical_word_spelling() != Some(segment) {
            TokenMetadata::with_source_spelling(segment_span, segment)
        } else {
            TokenMetadata::new(segment_span)
        };
        parts.push((token, metadata));
        first = false;
        offset = segment_end;
        if offset < text.len() {
            if text.as_bytes()[offset] != b'\\' {
                return None;
            }
            parts.push((
                Token::Backslash,
                TokenMetadata::new(offset_span(start, offset, 1)?),
            ));
            offset += 1;
        }
    }
    Some(parts)
}

/// Returns a span for a byte range starting at `start + offset` on one line.
fn offset_span(start: Span, offset: usize, len: usize) -> Option<Span> {
    let col = start.col.checked_add(u32::try_from(offset).ok()?)?;
    let end = col.checked_add(u32::try_from(len).ok()?)?;
    Some(Span::with_end_from(
        Span::new_in_source(start.line, col, 0),
        Span::new_in_source(start.line, end, 0),
    ))
}

/// Returns whether a native token is a cast form.
pub(super) fn is_cast_token(id: i64) -> bool {
    matches!(
        id,
        tok::T_INT_CAST
            | tok::T_DOUBLE_CAST
            | tok::T_BOOL_CAST
            | tok::T_ARRAY_CAST
            | tok::T_OBJECT_CAST
            | tok::T_STRING_CAST
            | tok::T_UNSET_CAST
            | tok::T_VOID_CAST
    )
}

/// Projects a cast token as `(` + normalized type identifier + `)`.
pub(super) fn project_cast<T: SourceToken>(
    id: i64,
    text: &str,
    start: Span,
    tokens: &[T],
    index: usize,
    previous_significant: i64,
) -> Option<Vec<SpannedToken>> {
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    let trimmed = inner.trim_matches(|byte: char| byte == ' ' || byte == '\t');
    if trimmed.is_empty() {
        return None;
    }
    let leading = inner.len() - inner.trim_start_matches(|byte: char| byte == ' ' || byte == '\t').len();
    let (normalized, identifier_offset, identifier_len, rparen_offset) = match id {
        // The text view rewrites these casts to `(int)`, so their emitted offsets follow
        // that five-byte normalized token rather than the source spelling.
        tok::T_STRING_CAST | tok::T_UNSET_CAST => ("int", 1, 3, 4),
        tok::T_VOID_CAST => {
            if void_cast_starts_statement(tokens, index, previous_significant) {
                ("int", 1, 3, 4)
            } else {
                return None;
            }
        }
        _ => (trimmed, 1 + leading, trimmed.len(), text.len() - 1),
    };
    let mut parts = Vec::with_capacity(3);
    parts.push((
        Token::LParen,
        TokenMetadata::new(offset_span(start, 0, 1)?),
    ));
    parts.push((
        Token::Identifier(normalized.to_owned()),
        TokenMetadata::new(offset_span(start, identifier_offset, identifier_len)?),
    ));
    parts.push((
        Token::RParen,
        TokenMetadata::new(offset_span(start, rparen_offset, 1)?),
    ));
    Some(parts)
}

/// Projects one native token, or returns `None` when it needs the text view.
pub(super) fn project(id: i64, text: &str, span: Span) -> Option<Projected> {
    match id {
        tok::T_OPEN_TAG | tok::T_INLINE_HTML => return Some(Projected::Skip),
        tok::T_WHITESPACE | tok::T_COMMENT | tok::T_DOC_COMMENT => {
            return Some(Projected::Skip);
        }
        tok::T_CLOSE_TAG => return Some(Projected::Token(Token::Semicolon, false)),
        tok::T_OPEN_TAG_WITH_ECHO => return Some(Projected::Token(Token::Echo, false)),
        tok::T_ATTRIBUTE => return Some(Projected::Token(Token::AttrOpen, false)),
        tok::T_VARIABLE => {
            let name = text.strip_prefix('$')?;
            if name.is_empty() {
                return None;
            }
            let token = if name == "this" {
                Token::This
            } else {
                Token::Variable(name.to_owned())
            };
            return Some(Projected::Token(token, false));
        }
        tok::T_STRING => {
            return Some(Projected::Token(Token::Identifier(text.to_owned()), false));
        }
        tok::T_LNUMBER => {
            if text.len() > 1 && text.starts_with('0') {
                return None;
            }
            return text
                .parse::<i64>()
                .ok()
                .map(|value| Projected::Token(Token::IntLiteral(value), false));
        }
        tok::T_DNUMBER => {
            return text
                .replace('_', "")
                .parse::<f64>()
                .ok()
                .map(|value| Projected::Token(Token::FloatLiteral(value), false));
        }
        tok::T_CONSTANT_ENCAPSED_STRING
        | tok::T_START_HEREDOC
        | tok::T_END_HEREDOC
        | tok::T_ENCAPSED_AND_WHITESPACE
        | tok::T_NUM_STRING
        | tok::T_STRING_VARNAME
        | tok::T_CURLY_OPEN
        | tok::T_DOLLAR_OPEN_CURLY_BRACES
        | tok::T_GOTO
        | tok::T_ELLIPSIS
        | tok::T_HALT_COMPILER
        | tok::T_NS_SEPARATOR => return None,
        _ => {}
    }

    if id < 256 {
        let byte = u8::try_from(id).ok()?;
        if matches!(byte, b'"' | b'\'' | b'`' | b'$' | b'\\') {
            return None;
        }
        return byte_token(byte, span)
            .ok()
            .map(|token| Projected::Token(token, false));
    }

    if let Some((operator, len)) = operator(text.as_bytes()) {
        if len == text.len() {
            return Some(Projected::Token(operator_token(operator), false));
        }
    }

    if is_word(text) {
        return Some(Projected::Token(word_token(text), true));
    }

    None
}

/// Projects one supported literal token through the existing literal decoders.
pub(super) fn project_literal(
    id: i64,
    text: &str,
    cursor: &mut Cursor,
    start: Span,
) -> Option<Vec<SpannedToken>> {
    if id == tok::T_CONSTANT_ENCAPSED_STRING {
        if text.starts_with('"') {
            cursor.skip_bytes(1);
            return decode_double_quoted_body(cursor, start).ok();
        }
        if text.starts_with('\'') {
            let token = single_quoted_token(text, text.ends_with('\''), start).ok()?;
            cursor.skip_bytes(text.len());
            let span = Span::with_end_from(start, cursor.span());
            return Some(vec![(token, TokenMetadata::new(span))]);
        }
        return None;
    }
    if id == tok::T_START_HEREDOC {
        if !text.starts_with("<<<") {
            return None;
        }
        cursor.skip_bytes(3);
        return scan_heredoc(cursor).ok();
    }
    if id == i64::from(b'"') {
        cursor.skip_bytes(1);
        return decode_double_quoted_body(cursor, start).ok();
    }
    if id == i64::from(b'`') {
        cursor.skip_bytes(1);
        return decode_backtick_body(cursor, start).ok();
    }
    None
}

/// Splits a native qualified-name token into Backslash and Identifier tokens.
pub(super) fn project_qualified_name(
    id: i64,
    text: &str,
    cursor: &mut Cursor,
) -> Option<Vec<SpannedToken>> {
    let mut parts = Vec::new();
    let mut offset = 0usize;
    if id == tok::T_NAME_FULLY_QUALIFIED {
        if text.as_bytes().first() != Some(&b'\\') {
            return None;
        }
        let start = cursor.span();
        cursor.skip_bytes(1);
        parts.push((
            Token::Backslash,
            TokenMetadata::new(Span::with_end_from(start, cursor.span())),
        ));
        offset = 1;
    }

    let mut first = true;
    while offset < text.len() {
        let rest = &text[offset..];
        let segment_end = rest.find('\\').map_or(text.len(), |position| offset + position);
        let segment = &text[offset..segment_end];
        if segment.is_empty() {
            return None;
        }
        let start = cursor.span();
        cursor.skip_bytes(segment.len());
        let segment_span = Span::with_end_from(start, cursor.span());
        let relative_prefix = id == tok::T_NAME_RELATIVE && first;
        let token = if relative_prefix {
            if !segment.eq_ignore_ascii_case("namespace") {
                return None;
            }
            Token::Namespace
        } else {
            Token::Identifier(segment.to_owned())
        };
        let metadata = if relative_prefix && token.canonical_word_spelling() != Some(segment) {
            TokenMetadata::with_source_spelling(segment_span, segment)
        } else {
            TokenMetadata::new(segment_span)
        };
        parts.push((token, metadata));
        first = false;
        offset = segment_end;
        if offset < text.len() {
            if text.as_bytes()[offset] != b'\\' {
                return None;
            }
            let start = cursor.span();
            cursor.skip_bytes(1);
            parts.push((
                Token::Backslash,
                TokenMetadata::new(Span::with_end_from(start, cursor.span())),
            ));
            offset += 1;
        }
    }
    Some(parts)
}

/// Returns whether `text` is a PHP label-shaped word.
fn is_word(text: &str) -> bool {
    let mut bytes = text.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_' || first >= 0x80)
        && bytes.all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
        })
}
