//! Purpose:
//! Builds a normalized compiler token stream directly from native scanner tokens.
//!
//! Called from:
//! - `crate::syntax::tokenizer_validation::validate_in_scope` on the default
//!   `TOKEN_PARSE` path, with the text parser view as automatic fallback.
//!
//! Key details:
//! - Supported native tokens are projected directly into source-spanned compiler tokens.
//! - Normalization-sensitive shapes fall back without allocating a second parser view.
//! - Tokens keep source spans, so no view byte map is needed.
//! - Both views feed the same AST parser; this module never owns a second grammar.

mod projection;

use std::cell::OnceCell;

use crate::syntax::lexer::{Cursor, SpannedToken, Token, TokenMetadata};
use crate::syntax::span::Span;

use super::next_significant_index;
use self::projection::{
    is_cast_token, project, project_cast, project_literal, project_qualified_name,
    project_qualified_name_fast, Projected,
};
use super::parser_view::{
    arrow_starts_function_body, ellipsis_is_first_class_callable, is_reference_ampersand,
};
use super::prechecks::delimiter_pairs;
use super::prechecks::matching_brace_end;
use super::reference_scan::{
    reference_chain_end, reference_continuation_is_valid, reference_target_starts_at,
};
use super::{tok, SourceToken};

/// A normalized token stream ready for the shared AST parser.
pub(super) struct TokenProjection {
    /// Compiler tokens with source spans.
    pub(super) tokens: Vec<SpannedToken>,
}

/// Builds a source-spanned compiler token stream, or returns `None` when a token
/// needs the text parser view.
pub(super) fn build<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    reference_delimiters: &OnceCell<Vec<Option<usize>>>,
) -> Option<TokenProjection> {
    if !early_supported(tokens) {
        return None;
    }
    let text = std::str::from_utf8(source).ok()?;
    if text.as_bytes().contains(&b'\r') {
        return None;
    }
    // ASCII source makes byte offsets equal character columns, so one line index derives every
    // token span. Non-ASCII source keeps the cursor path, which counts characters for PHP columns.
    let fast_spans = text.is_ascii();
    let line_starts = if fast_spans {
        super::line_starts(text)
    } else {
        Vec::new()
    };
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE;
    let mut cursor = Cursor::new_in_source(text, 0, profile);
    let mut literal_cursor = Cursor::new_in_source(text, 0, profile);
    let mut projected = Vec::with_capacity(tokens.len() + 2);
    let mut previous_significant = tok::T_OPEN_TAG;
    let mut pending_label = false;
    let mut parens: Vec<usize> = Vec::new();
    let mut brackets: Vec<usize> = Vec::new();
    let mut braces: Vec<usize> = Vec::new();
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
        // The text view blanks the marker of `name(...)`, so first-class callables keep using
        // that path instead of exposing parser shapes that only accept a zero-argument call.
        if id == tok::T_ELLIPSIS && ellipsis_is_first_class_callable(source, tokens, index) {
            return None;
        }
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
    Some(TokenProjection { tokens: projected })
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
    if !reference_target_starts_at(tokens, target) {
        return false;
    }
    let Some(end) = reference_chain_end(source, tokens, target, pairs) else {
        return false;
    };
    let Some(after) = next_significant_index(tokens, end) else {
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

/// Rejects unsupported tokens before allocating the full projection.
///
/// This is an ID-only pass over the complete token stream: an unsupported token late in a
/// large source must not pay the source scan, line index and projection allocation before
/// falling back to the text view.
fn early_supported<T: SourceToken>(tokens: &[T]) -> bool {
    tokens.iter().all(|token| {
        let id = token.id();
        if id < 256 {
            let byte = u8::try_from(id).unwrap_or(0);
            return !matches!(byte, b'$' | b'\\');
        }
        !matches!(id, tok::T_HALT_COMPILER)
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
