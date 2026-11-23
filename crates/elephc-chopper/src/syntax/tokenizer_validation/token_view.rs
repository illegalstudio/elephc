//! Purpose:
//! Builds a normalized compiler token stream directly from native scanner tokens.
//!
//! Called from:
//! - `crate::syntax::tokenizer_validation::validate_in_scope` behind the
//!   `ELEPHC_TOKEN_VIEW` prototype switch.
//!
//! Key details:
//! - This is the prototype path for code tokens only; unsupported tokens fall back
//!   to the text parser view.
//! - Tokens keep source spans, so no view byte map is needed.
//! - Literal and normalization-sensitive tokens are deliberately unsupported here.

use crate::scanner::operator;
use crate::syntax::lexer::{
    byte_token, decode_backtick_body, decode_double_quoted_body, operator_token, scan_heredoc,
    single_quoted_token, word_token, Cursor, SpannedToken, Token, TokenMetadata,
};
use crate::syntax::span::Span;

use super::{tok, ReferenceError, SourceToken};

/// A normalized token stream ready for the shared AST parser.
pub(super) struct TokenProjection {
    /// Compiler tokens with source spans.
    pub(super) tokens: Vec<SpannedToken>,
    /// Reference diagnostics found while projecting; none in the code-token prototype.
    pub(super) reference_error: Option<ReferenceError>,
}

/// One projected native token: skipped, emitted, or unsupported.
enum Projected {
    /// Trivia, tags, or inline HTML with no compiler token.
    Skip,
    /// A compiler token plus whether source spelling metadata may be needed.
    Token(Token, bool),
}

/// Builds a source-spanned compiler token stream, or returns `None` when a token
/// needs the text parser view.
pub(super) fn build<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<TokenProjection> {
    if !early_supported(tokens) {
        return None;
    }
    let text = std::str::from_utf8(source).ok()?;
    let mut cursor = Cursor::new_in_source(text, 0);
    let mut projected = Vec::with_capacity(tokens.len() + 2);
    let mut previous_significant = 0i64;
    let mut pending_label = false;
    projected.push((
        Token::OpenTag,
        TokenMetadata::new(Span::new_in_source(1, 1, 0)),
    ));

    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.pos() < cursor.position() {
            return None;
        }
        if token.id() == tok::T_DOUBLE_COLON && next_significant_is_brace(tokens, index) {
            return None;
        }
        if pending_label {
            if token.id() == i64::from(b':') {
                return None;
            }
            pending_label = false;
        }
        if token.id() == tok::T_STRING && label_boundary(previous_significant) {
            pending_label = true;
        }
        cursor.skip_bytes(token.pos() - cursor.position());
        let start = cursor.span();
        let id = token.id();
        if matches!(
            id,
            tok::T_OPEN_TAG
                | tok::T_INLINE_HTML
                | tok::T_WHITESPACE
                | tok::T_COMMENT
                | tok::T_DOC_COMMENT
        ) {
            cursor.skip_bytes(token.len());
            index += 1;
            continue;
        }
        let token_text = text.get(token.pos()..token.pos() + token.len())?;
        if id == tok::T_CONSTANT_ENCAPSED_STRING
            || id == tok::T_START_HEREDOC
            || id == i64::from(b'"')
            || id == i64::from(b'`')
        {
            let parts = project_literal(id, token_text, &mut cursor, start)?;
            projected.extend(parts);
            while index < tokens.len() && tokens[index].pos() < cursor.position() {
                index += 1;
            }
            previous_significant = tok::T_STRING;
            continue;
        }
        if matches!(
            id,
            tok::T_NAME_FULLY_QUALIFIED | tok::T_NAME_QUALIFIED | tok::T_NAME_RELATIVE
        ) {
            let parts = project_qualified_name(id, token_text, &mut cursor)?;
            projected.extend(parts);
            previous_significant = tok::T_STRING;
            index += 1;
            continue;
        }
        let projected_token = project(id, token_text, start)?;
        cursor.skip_bytes(token.len());
        let end = cursor.span();
        let span = Span::with_end_from(start, end);
        match projected_token {
            Projected::Skip => {}
            Projected::Token(token, spelling) => {
                let metadata = if spelling && token.canonical_word_spelling() != Some(token_text) {
                    TokenMetadata::with_source_spelling(span, token_text)
                } else {
                    TokenMetadata::new(span)
                };
                projected.push((token, metadata));
            }
        }
        if !matches!(id, tok::T_WHITESPACE | tok::T_COMMENT | tok::T_DOC_COMMENT) {
            previous_significant = id;
        }
        index += 1;
    }

    if cursor.position() < text.len() {
        cursor.skip_bytes(text.len() - cursor.position());
    }
    projected.push((Token::Eof, TokenMetadata::new(cursor.span())));
    Some(TokenProjection {
        tokens: projected,
        reference_error: None,
    })
}

/// Projects one native token, or returns `None` when it is outside the prototype.
fn project(id: i64, text: &str, span: Span) -> Option<Projected> {
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
            if neutralized_name(text) {
                return None;
            }
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
        tok::T_DNUMBER
        | tok::T_CONSTANT_ENCAPSED_STRING
        | tok::T_START_HEREDOC
        | tok::T_END_HEREDOC
        | tok::T_ENCAPSED_AND_WHITESPACE
        | tok::T_NUM_STRING
        | tok::T_STRING_VARNAME
        | tok::T_CURLY_OPEN
        | tok::T_DOLLAR_OPEN_CURLY_BRACES
        | tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
        | tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
        | tok::T_GOTO
        | tok::T_ELLIPSIS
        | tok::T_HALT_COMPILER
        | tok::T_NS_SEPARATOR
        | tok::T_STRING_CAST
        | tok::T_UNSET_CAST
        | tok::T_VOID_CAST => return None,
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
fn project_literal(
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
fn project_qualified_name(
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

/// Rejects an early unsupported token before allocating the full projection.
///
/// A string-heavy source usually exposes a quote in its first statement; sampling the
/// prefix keeps the fallback cheap without scanning the complete source or token stream.
fn early_supported<T: SourceToken>(tokens: &[T]) -> bool {
    tokens.iter().take(32).all(|token| {
        let id = token.id();
        if id < 256 {
            let byte = u8::try_from(id).unwrap_or(0);
            return !matches!(byte, b'$' | b'\\');
        }
        !matches!(
            id,
            tok::T_DNUMBER
                | tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                | tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
                | tok::T_GOTO
                | tok::T_ELLIPSIS
                | tok::T_HALT_COMPILER
                | tok::T_NS_SEPARATOR
                | tok::T_STRING_CAST
                | tok::T_UNSET_CAST
                | tok::T_VOID_CAST
        )
    })
}

/// Returns whether a significant token can precede a statement label.
fn label_boundary(id: i64) -> bool {
    id == 0
        || id == i64::from(b';')
        || id == i64::from(b'{')
        || id == i64::from(b'}')
        || id == i64::from(b')')
        || id == i64::from(b':')
        || id == tok::T_OPEN_TAG
        || id == tok::T_CLOSE_TAG
        || id == tok::T_ELSE
        || id == tok::T_DO
        || id == tok::T_TRY
        || id == tok::T_FINALLY
}

/// Returns whether the next significant token after `index` is `{`.
fn next_significant_is_brace<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let mut next = index + 1;
    while tokens.get(next).is_some_and(|token| {
        matches!(
            token.id(),
            tok::T_WHITESPACE | tok::T_COMMENT | tok::T_DOC_COMMENT
        )
    }) {
        next += 1;
    }
    tokens
        .get(next)
        .is_some_and(|token| token.id() == i64::from(b'{'))
}

/// Returns whether a T_STRING spelling is neutralized by the text parser view.
fn neutralized_name(text: &str) -> bool {
    [
        "enum", "self", "parent", "true", "false", "null", "packed", "extern", "ifdef",
    ]
    .iter()
    .any(|name| text.eq_ignore_ascii_case(name))
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
