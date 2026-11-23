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

use std::cell::OnceCell;

use crate::scanner::operator;
use crate::syntax::lexer::{
    byte_token, decode_backtick_body, decode_double_quoted_body, operator_token, scan_heredoc,
    single_quoted_token, word_token, Cursor, SpannedToken, Token, TokenMetadata,
};
use crate::syntax::span::Span;

use super::next_significant_index;
use super::parser_view::{
    arrow_starts_function_body, is_reference_ampersand, void_cast_starts_statement,
};
use super::prechecks::delimiter_pairs;
use super::prechecks::matching_brace_end;
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
    if text.as_bytes().contains(&b'\r') {
        return None;
    }
    let fast_spans = text.is_ascii();
    let line_starts = if fast_spans {
        super::line_starts(text)
    } else {
        Vec::new()
    };
    let mut cursor = Cursor::new_in_source(text, 0);
    let mut literal_cursor = Cursor::new_in_source(text, 0);
    let mut projected = Vec::with_capacity(tokens.len() + 2);
    let mut previous_significant = tok::T_OPEN_TAG;
    let mut pending_label = false;
    let mut parens: Vec<usize> = Vec::new();
    let mut brackets: Vec<usize> = Vec::new();
    let mut braces: Vec<usize> = Vec::new();
    let reference_delimiters = OnceCell::new();
    projected.push((
        Token::OpenTag,
        TokenMetadata::new(Span::new_in_source(1, 1, 0)),
    ));

    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if !fast_spans && token.pos() < cursor.position() {
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
        let id = token.id();
        if id == i64::from(b'(') {
            parens.push(index);
        } else if id == i64::from(b')') {
            parens.pop();
        } else if id == i64::from(b'[') || id == tok::T_ATTRIBUTE {
            brackets.push(index);
        } else if id == i64::from(b']') {
            brackets.pop();
        } else if id == i64::from(b'{') {
            braces.push(index);
        } else if id == i64::from(b'}') {
            braces.pop();
        }
        let start = if fast_spans {
            span_start(token, &line_starts)?
        } else {
            cursor.skip_bytes(token.pos() - cursor.position());
            cursor.span()
        };
        if matches!(
            id,
            tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                | tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
        ) {
            let remove = is_reference_ampersand(
                tokens,
                index,
                id,
                previous_significant,
                brackets.last().copied(),
                parens.last().copied(),
                braces.last().copied(),
                |arrow| {
                    let pairs = reference_delimiters.get_or_init(|| delimiter_pairs(tokens));
                    if arrow.is_some_and(|arrow| {
                        arrow_starts_function_body(tokens, arrow, pairs)
                    }) {
                        return false;
                    }
                    reference_target_safe(
                        tokens,
                        index,
                        pairs,
                        source,
                        previous_significant == i64::from(b'='),
                    )
                },
            );
            if remove {
                if !fast_spans {
                    cursor.skip_bytes(token.len());
                }
                index += 1;
                continue;
            }
            if previous_significant != tok::T_FN
                && reference_context(previous_significant)
            {
                return None;
            }
            let span = if fast_spans {
                span_for_text("&", start)?
            } else {
                cursor.skip_bytes(token.len());
                Span::with_end_from(start, cursor.span())
            };
            projected.push((Token::Ampersand, TokenMetadata::new(span)));
            previous_significant = id;
            index += 1;
            continue;
        }
        if matches!(
            id,
            tok::T_OPEN_TAG
                | tok::T_INLINE_HTML
                | tok::T_WHITESPACE
                | tok::T_COMMENT
                | tok::T_DOC_COMMENT
        ) {
            if !fast_spans {
                cursor.skip_bytes(token.len());
            }
            if matches!(id, tok::T_OPEN_TAG | tok::T_INLINE_HTML) {
                previous_significant = id;
            }
            index += 1;
            continue;
        }
        if id == i64::from(b'{') && previous_significant == tok::T_DOUBLE_COLON {
            let close = matching_brace_end(tokens, index + 1, tokens.len())?;
            projected.push((
                Token::Identifier("x".to_owned()),
                TokenMetadata::new(start),
            ));
            if !fast_spans {
                let end = tokens[close].pos() + tokens[close].len();
                cursor.skip_bytes(end - cursor.position());
            }
            braces.pop();
            previous_significant = tok::T_STRING;
            index = close + 1;
            continue;
        }
        let token_text = text.get(token.pos()..token.pos() + token.len())?;
        if is_cast_token(id) {
            let parts = project_cast(
                id,
                token_text,
                start,
                tokens,
                index,
                previous_significant,
            )?;
            projected.extend(parts);
            if !fast_spans {
                cursor.skip_bytes(token.len());
            }
            previous_significant = id;
            index += 1;
            continue;
        }
        if id == tok::T_CONSTANT_ENCAPSED_STRING
            || id == tok::T_START_HEREDOC
            || id == i64::from(b'"')
            || id == i64::from(b'`')
        {
            let decoder_cursor = if fast_spans {
                literal_cursor.skip_bytes(token.pos() - literal_cursor.position());
                &mut literal_cursor
            } else {
                &mut cursor
            };
            let parts = project_literal(id, token_text, decoder_cursor, start)?;
            projected.extend(parts);
            let end_pos = decoder_cursor.position();
            while index < tokens.len() && tokens[index].pos() < end_pos {
                index += 1;
            }
            previous_significant = tok::T_STRING;
            continue;
        }
        if matches!(
            id,
            tok::T_NAME_FULLY_QUALIFIED | tok::T_NAME_QUALIFIED | tok::T_NAME_RELATIVE
        ) {
            let parts = if fast_spans {
                project_qualified_name_fast(id, token_text, start)?
            } else {
                project_qualified_name(id, token_text, &mut cursor)?
            };
            projected.extend(parts);
            previous_significant = tok::T_STRING;
            index += 1;
            continue;
        }
        let projected_token = project(id, token_text, start)?;
        let span = if fast_spans {
            span_for_text(token_text, start)?
        } else {
            cursor.skip_bytes(token.len());
            Span::with_end_from(start, cursor.span())
        };
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

    let eof_span = if fast_spans {
        eof_span(text, &line_starts)?
    } else {
        if cursor.position() < text.len() {
            cursor.skip_bytes(text.len() - cursor.position());
        }
        cursor.span()
    };
    projected.push((Token::Eof, TokenMetadata::new(eof_span)));
    Some(TokenProjection {
        tokens: projected,
        reference_error: None,
    })
}

/// Returns the source span at a token start using its native line and a line index.
fn span_start<T: SourceToken>(token: &T, line_starts: &[usize]) -> Option<Span> {
    let line = token.line();
    if line == 0 {
        return None;
    }
    let line_start = *line_starts.get(line - 1)?;
    let col = u32::try_from(token.pos().checked_sub(line_start)?.checked_add(1)?).ok()?;
    Some(Span::new_in_source(u32::try_from(line).ok()?, col, 0))
}

/// Extends a start span through ASCII token text, counting CR/LF as line breaks.
fn span_for_text(text: &str, start: Span) -> Option<Span> {
    let mut line = start.line;
    let mut col = start.col;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                line = line.checked_add(1)?;
                col = 1;
            }
            b'\r' => {
                line = line.checked_add(1)?;
                col = 1;
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
            }
            _ => col = col.checked_add(1)?,
        }
        index += 1;
    }
    Some(Span::with_end_from(
        start,
        Span::new_in_source(line, col, 0),
    ))
}

/// Returns the EOF span from the source line index.
fn eof_span(text: &str, line_starts: &[usize]) -> Option<Span> {
    let line = u32::try_from(line_starts.len()).ok()?;
    let last_start = *line_starts.last()?;
    let col = u32::try_from(text.len().checked_sub(last_start)?.checked_add(1)?).ok()?;
    Some(Span::new_in_source(line, col, 0))
}

/// Splits a qualified name using the token-start span and byte offsets.
fn project_qualified_name_fast(id: i64, text: &str, start: Span) -> Option<Vec<SpannedToken>> {
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

/// Returns whether an ampersand must stay on the text-view fallback.
fn reference_context(previous_significant: i64) -> bool {
    previous_significant == i64::from(b'=')
        || previous_significant == i64::from(b'[')
        || previous_significant == i64::from(b',')
        || previous_significant == i64::from(b'(')
        || previous_significant == tok::T_DOUBLE_ARROW
        || previous_significant == tok::T_USE
}

/// Accepts a reference target chain followed by a list/statement terminator.
fn reference_target_safe<T: SourceToken>(
    tokens: &[T],
    index: usize,
    pairs: &[Option<usize>],
    source: &[u8],
    allow_continuation: bool,
) -> bool {
    let Some(target) = next_significant_index(tokens, index) else {
        return false;
    };
    let target_id = tokens[target].id();
    let mut cursor = target;
    match target_id {
        tok::T_VARIABLE => {}
        tok::T_STATIC => {
            let Some(next) = next_significant_index(tokens, target) else {
                return false;
            };
            if tokens[next].id() != tok::T_DOUBLE_COLON {
                return false;
            }
        }
        tok::T_STRING | tok::T_IDENTIFIER | tok::T_NAME_FULLY_QUALIFIED
        | tok::T_NAME_QUALIFIED | tok::T_NAME_RELATIVE => {
            let Some(next) = next_significant_index(tokens, target) else {
                return false;
            };
            if !matches!(
                tokens[next].id(),
                id if id == i64::from(b'(')
                    || id == i64::from(b'[')
                    || id == tok::T_DOUBLE_COLON
                    || id == tok::T_OBJECT_OPERATOR
                    || id == tok::T_NULLSAFE_OBJECT_OPERATOR
            ) {
                return false;
            }
        }
        _ => return false,
    }

    loop {
        let Some(next) = next_significant_index(tokens, cursor) else {
            return false;
        };
        match tokens[next].id() {
            id if id == i64::from(b'[')
                || id == i64::from(b'(')
                || id == i64::from(b'{') =>
            {
                let Some(close) = pairs[next] else {
                    return false;
                };
                if contains_ampersand(tokens, next + 1, close) {
                    return false;
                }
                cursor = close;
            }
            id if id == tok::T_OBJECT_OPERATOR
                || id == tok::T_NULLSAFE_OBJECT_OPERATOR
                || id == tok::T_DOUBLE_COLON =>
            {
                let Some(member) = next_significant_index(tokens, next) else {
                    return false;
                };
                if tokens[member].id() == i64::from(b'{') {
                    let Some(close) = pairs[member] else {
                        return false;
                    };
                    if contains_ampersand(tokens, member + 1, close) {
                        return false;
                    }
                    cursor = close;
                } else if matches!(
                    tokens[member].id(),
                    tok::T_VARIABLE | tok::T_STRING | tok::T_IDENTIFIER
                ) || is_word_token(source, &tokens[member])
                {
                    cursor = member;
                } else {
                    return false;
                }
            }
            _ => break,
        }
    }

    let Some(after) = next_significant_index(tokens, cursor) else {
        return false;
    };
    if allow_continuation {
        reference_continuation_is_valid(source, &tokens[after])
    } else {
        matches!(
            tokens[after].id(),
            id if id == i64::from(b';')
                || id == i64::from(b',')
                || id == i64::from(b')')
                || id == i64::from(b']')
                || id == i64::from(b'}')
        )
    }
}

/// Returns whether a reference target continuation is not a write operator.
fn reference_continuation_is_valid<T: SourceToken>(source: &[u8], token: &T) -> bool {
    let Ok(text) = std::str::from_utf8(&source[token.pos()..token.pos() + token.len()]) else {
        return false;
    };
    !matches!(
        text,
        "=" | "+="
            | "-="
            | "*="
            | "/="
            | ".="
            | "%="
            | "&="
            | "|="
            | "^="
            | "<<="
            | ">>="
            | "**="
            | "??="
            | "++"
            | "--"
    )
}

/// Returns whether one native token's source text is a word.
fn is_word_token<T: SourceToken>(source: &[u8], token: &T) -> bool {
    std::str::from_utf8(&source[token.pos()..token.pos() + token.len()])
        .ok()
        .is_some_and(is_word)
}

/// Returns whether a token range contains another reference ampersand.
fn contains_ampersand<T: SourceToken>(tokens: &[T], start: usize, end: usize) -> bool {
    tokens[start..end].iter().any(|token| {
        matches!(
            token.id(),
            tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                | tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
        )
    })
}

/// Returns whether a native token is a cast form.
fn is_cast_token(id: i64) -> bool {
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
fn project_cast<T: SourceToken>(
    id: i64,
    text: &str,
    start: Span,
    tokens: &[T],
    index: usize,
    previous_significant: i64,
) -> Option<Vec<SpannedToken>> {
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    if inner.is_empty() {
        return None;
    }
    let normalized = match id {
        tok::T_STRING_CAST | tok::T_UNSET_CAST => "int",
        tok::T_VOID_CAST => {
            if void_cast_starts_statement(tokens, index, previous_significant) {
                "int"
            } else {
                return None;
            }
        }
        _ => inner,
    };
    let mut parts = Vec::with_capacity(3);
    parts.push((
        Token::LParen,
        TokenMetadata::new(offset_span(start, 0, 1)?),
    ));
    parts.push((
        Token::Identifier(normalized.to_owned()),
        TokenMetadata::new(offset_span(start, 1, normalized.len())?),
    ));
    parts.push((
        Token::RParen,
        TokenMetadata::new(offset_span(start, 1 + normalized.len(), 1)?),
    ));
    Some(parts)
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
            tok::T_GOTO
                | tok::T_ELLIPSIS
                | tok::T_HALT_COMPILER
                | tok::T_NS_SEPARATOR
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
