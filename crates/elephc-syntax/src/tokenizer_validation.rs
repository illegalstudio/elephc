//! Purpose:
//! Provides TOKEN_PARSE syntax validation and Zend's semi-reserved-word feedback through
//! elephc's own lexer, parser and AST.
//!
//! Called from:
//! - `elephc-tokenizer-engine` after native tokenization when PHP's TOKEN_PARSE flag is set.
//!
//! Key details:
//! - No separate PHP grammar lives here: the source is normalized into elephc's AST parser view.
//! - Inline HTML, open/close tags and `__halt_compiler` payloads are blanked while line numbers
//!   stay stable, so parser diagnostics keep PHP's source lines.
//! - Accepted keyword identifiers are mapped back to byte offsets for token reclassification.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::span::Span;


/// Compile-time PHP 8.5 token IDs used by the validation view.
///
/// Every value is derived from the shared contract catalog; a typo produces `-1`, which the
/// tests fail loudly on. Resolving them at compile time removes ~18 catalog hash lookups per
/// source token from `parser_view`.
mod tok {
    use elephc_builtin_contract::token_const_id as id;
    pub const T_ABSTRACT: i64 = id("T_ABSTRACT");
    pub const T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG: i64 = id("T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG");
    pub const T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG: i64 = id("T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG");
    pub const T_ARRAY: i64 = id("T_ARRAY");
    pub const T_AS: i64 = id("T_AS");
    pub const T_CASE: i64 = id("T_CASE");
    pub const T_CLASS: i64 = id("T_CLASS");
    pub const T_CLOSE_TAG: i64 = id("T_CLOSE_TAG");
    pub const T_COMMENT: i64 = id("T_COMMENT");
    pub const T_CONST: i64 = id("T_CONST");
    pub const T_CONSTANT_ENCAPSED_STRING: i64 = id("T_CONSTANT_ENCAPSED_STRING");
    pub const T_DNUMBER: i64 = id("T_DNUMBER");
    pub const T_DOC_COMMENT: i64 = id("T_DOC_COMMENT");
    pub const T_DOLLAR_OPEN_CURLY_BRACES: i64 = id("T_DOLLAR_OPEN_CURLY_BRACES");
    pub const T_DOUBLE_ARROW: i64 = id("T_DOUBLE_ARROW");
    pub const T_DOUBLE_COLON: i64 = id("T_DOUBLE_COLON");
    pub const T_ELLIPSIS: i64 = id("T_ELLIPSIS");
    pub const T_ENUM: i64 = id("T_ENUM");
    pub const T_FN: i64 = id("T_FN");
    pub const T_FUNCTION: i64 = id("T_FUNCTION");
    pub const T_GOTO: i64 = id("T_GOTO");
    pub const T_HALT_COMPILER: i64 = id("T_HALT_COMPILER");
    pub const T_INLINE_HTML: i64 = id("T_INLINE_HTML");
    pub const T_INTERFACE: i64 = id("T_INTERFACE");
    pub const T_LIST: i64 = id("T_LIST");
    pub const T_NEW: i64 = id("T_NEW");
    pub const T_NULLSAFE_OBJECT_OPERATOR: i64 = id("T_NULLSAFE_OBJECT_OPERATOR");
    pub const T_NUM_STRING: i64 = id("T_NUM_STRING");
    pub const T_OPEN_TAG: i64 = id("T_OPEN_TAG");
    pub const T_OBJECT_OPERATOR: i64 = id("T_OBJECT_OPERATOR");
    pub const T_OPEN_TAG_WITH_ECHO: i64 = id("T_OPEN_TAG_WITH_ECHO");
    pub const T_PRIVATE: i64 = id("T_PRIVATE");
    pub const T_PROTECTED: i64 = id("T_PROTECTED");
    pub const T_PUBLIC: i64 = id("T_PUBLIC");
    pub const T_READONLY: i64 = id("T_READONLY");
    pub const T_RETURN: i64 = id("T_RETURN");
    pub const T_START_HEREDOC: i64 = id("T_START_HEREDOC");
    pub const T_STATIC: i64 = id("T_STATIC");
    pub const T_STRING: i64 = id("T_STRING");
    pub const T_STRING_CAST: i64 = id("T_STRING_CAST");
    pub const T_TRAIT: i64 = id("T_TRAIT");
    pub const T_UNSET_CAST: i64 = id("T_UNSET_CAST");
    pub const T_USE: i64 = id("T_USE");
    pub const T_VARIABLE: i64 = id("T_VARIABLE");
    pub const T_VOID_CAST: i64 = id("T_VOID_CAST");
    pub const T_WHITESPACE: i64 = id("T_WHITESPACE");
    /// PHP's parser does not use this token; preserving the old fallback keeps the comparison.
    pub const T_IDENTIFIER: i64 = -1;
}

const _: () = {
    assert!(tok::T_ABSTRACT != -1, "T_ABSTRACT missing from the tokenizer catalog");
    assert!(tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG != -1, "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG missing from the tokenizer catalog");
    assert!(tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG != -1, "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG missing from the tokenizer catalog");
    assert!(tok::T_ARRAY != -1, "T_ARRAY missing from the tokenizer catalog");
    assert!(tok::T_AS != -1, "T_AS missing from the tokenizer catalog");
    assert!(tok::T_CASE != -1, "T_CASE missing from the tokenizer catalog");
    assert!(tok::T_CLASS != -1, "T_CLASS missing from the tokenizer catalog");
    assert!(tok::T_CLOSE_TAG != -1, "T_CLOSE_TAG missing from the tokenizer catalog");
    assert!(tok::T_COMMENT != -1, "T_COMMENT missing from the tokenizer catalog");
    assert!(tok::T_CONST != -1, "T_CONST missing from the tokenizer catalog");
    assert!(tok::T_CONSTANT_ENCAPSED_STRING != -1, "T_CONSTANT_ENCAPSED_STRING missing from the tokenizer catalog");
    assert!(tok::T_DNUMBER != -1, "T_DNUMBER missing from the tokenizer catalog");
    assert!(tok::T_DOC_COMMENT != -1, "T_DOC_COMMENT missing from the tokenizer catalog");
    assert!(tok::T_DOLLAR_OPEN_CURLY_BRACES != -1, "T_DOLLAR_OPEN_CURLY_BRACES missing from the tokenizer catalog");
    assert!(tok::T_DOUBLE_ARROW != -1, "T_DOUBLE_ARROW missing from the tokenizer catalog");
    assert!(tok::T_DOUBLE_COLON != -1, "T_DOUBLE_COLON missing from the tokenizer catalog");
    assert!(tok::T_ELLIPSIS != -1, "T_ELLIPSIS missing from the tokenizer catalog");
    assert!(tok::T_ENUM != -1, "T_ENUM missing from the tokenizer catalog");
    assert!(tok::T_FN != -1, "T_FN missing from the tokenizer catalog");
    assert!(tok::T_FUNCTION != -1, "T_FUNCTION missing from the tokenizer catalog");
    assert!(tok::T_GOTO != -1, "T_GOTO missing from the tokenizer catalog");
    assert!(tok::T_HALT_COMPILER != -1, "T_HALT_COMPILER missing from the tokenizer catalog");
    assert!(tok::T_INLINE_HTML != -1, "T_INLINE_HTML missing from the tokenizer catalog");
    assert!(tok::T_INTERFACE != -1, "T_INTERFACE missing from the tokenizer catalog");
    assert!(tok::T_LIST != -1, "T_LIST missing from the tokenizer catalog");
    assert!(tok::T_NEW != -1, "T_NEW missing from the tokenizer catalog");
    assert!(tok::T_NULLSAFE_OBJECT_OPERATOR != -1, "T_NULLSAFE_OBJECT_OPERATOR missing from the tokenizer catalog");
    assert!(tok::T_NUM_STRING != -1, "T_NUM_STRING missing from the tokenizer catalog");
    assert!(tok::T_OPEN_TAG != -1, "T_OPEN_TAG missing from the tokenizer catalog");
    assert!(tok::T_OBJECT_OPERATOR != -1, "T_OBJECT_OPERATOR missing from the tokenizer catalog");
    assert!(tok::T_OPEN_TAG_WITH_ECHO != -1, "T_OPEN_TAG_WITH_ECHO missing from the tokenizer catalog");
    assert!(tok::T_PRIVATE != -1, "T_PRIVATE missing from the tokenizer catalog");
    assert!(tok::T_PROTECTED != -1, "T_PROTECTED missing from the tokenizer catalog");
    assert!(tok::T_PUBLIC != -1, "T_PUBLIC missing from the tokenizer catalog");
    assert!(tok::T_READONLY != -1, "T_READONLY missing from the tokenizer catalog");
    assert!(tok::T_RETURN != -1, "T_RETURN missing from the tokenizer catalog");
    assert!(tok::T_START_HEREDOC != -1, "T_START_HEREDOC missing from the tokenizer catalog");
    assert!(tok::T_STATIC != -1, "T_STATIC missing from the tokenizer catalog");
    assert!(tok::T_STRING != -1, "T_STRING missing from the tokenizer catalog");
    assert!(tok::T_STRING_CAST != -1, "T_STRING_CAST missing from the tokenizer catalog");
    assert!(tok::T_TRAIT != -1, "T_TRAIT missing from the tokenizer catalog");
    assert!(tok::T_UNSET_CAST != -1, "T_UNSET_CAST missing from the tokenizer catalog");
    assert!(tok::T_USE != -1, "T_USE missing from the tokenizer catalog");
    assert!(tok::T_VARIABLE != -1, "T_VARIABLE missing from the tokenizer catalog");
    assert!(tok::T_VOID_CAST != -1, "T_VOID_CAST missing from the tokenizer catalog");
    assert!(tok::T_WHITESPACE != -1, "T_WHITESPACE missing from the tokenizer catalog");
};

/// The token metadata this validator needs, implemented by each embedder's token type.
///
/// The trait keeps `TOKEN_PARSE` allocation-free for callers that already hold a token
/// vector: validation reads `id`/`pos`/`len` in place instead of copying 24 bytes per token.
pub trait SourceToken {
    /// Returns the PHP token identifier.
    fn id(&self) -> i64;
    /// Returns the zero-based byte offset in the original source.
    fn pos(&self) -> usize;
    /// Returns the byte length in the original source.
    fn len(&self) -> usize;
}

/// One tokenizer token reduced to the metadata this validator needs.
#[derive(Clone, Copy, Debug)]
pub struct SyntaxToken {
    /// PHP token identifier.
    pub id: i64,
    /// Zero-based byte offset in the original source.
    pub pos: usize,
    /// Byte length in the original source.
    pub len: usize,
}

impl SourceToken for SyntaxToken {
    /// Returns the PHP token identifier.
    fn id(&self) -> i64 { self.id }
    /// Returns the zero-based byte offset in the original source.
    fn pos(&self) -> usize { self.pos }
    /// Returns the byte length in the original source.
    fn len(&self) -> usize { self.len }
}

/// One TOKEN_PARSE failure with PHP-compatible text and a source byte offset.
#[derive(Clone, Debug)]
pub struct ValidationError {
    /// PHP-style parser message without a filename suffix.
    pub message: String,
    /// Byte offset used to derive the ParseError line.
    pub pos: usize,
    /// Whether elephc's parser-view lexer produced this diagnostic.
    pub lexical: bool,
}

/// The result of validating one source for TOKEN_PARSE.
#[derive(Clone, Debug, Default)]
pub struct ValidationFeedback {
    /// Byte offsets of keyword tokens Zend must rewrite to T_STRING.
    pub names: Vec<usize>,
    /// The first parser error, if any.
    pub error: Option<ValidationError>,
}

/// Validates PHP source and returns identifier feedback or the first PHP-style parse error.
pub fn validate<T: SourceToken>(source: &[u8], tokens: &[T]) -> ValidationFeedback {
    let (view, map) = parser_view(source, tokens);
    let view_tokens = match crate::lexer::tokenize(&view) {
        Ok(tokens) => tokens,
        Err(error) => {
            let view_pos = view_offset(&view, &line_starts(&view), error.span.line, error.span.col);
            let source_pos = map.get(view_pos).copied().map(|pos| pos as usize).unwrap_or(source.len()).min(source.len());
            return ValidationFeedback {
                names: Vec::new(),
                error: Some(ValidationError { message: error.message, pos: source_pos, lexical: true }),
            };
        }
    };
    let (result, names) = crate::parser::parse_with_tokenizer_feedback(&view_tokens);
    match result {
        Ok(_) => {
            let error = reserved_list_error(source, tokens);
            let names = if error.is_some() || names.is_empty() {
                Vec::new()
            } else {
                mapped_names(&view, &map, &line_starts(&view), names, source)
            };
            ValidationFeedback { names, error }
        },
        Err(error) => ValidationFeedback {
            names: Vec::new(),
            error: Some(parser_error(&view, &map, &line_starts(&view), source, tokens, &error)),
        },
    }
}

/// Builds the PHP-only parser view and a view-byte to source-byte map.
fn parser_view<T: SourceToken>(source: &[u8], tokens: &[T]) -> (String, Vec<u32>) {
    let mut view = Vec::with_capacity(source.len() + 8);
    let mut map = Vec::with_capacity(source.len() + 8);
    append_mapped(&mut view, &mut map, b"<?php ", 0);
    let mut index = 0;
    let mut previous_significant = 0i64;
    let mut blank_indices = HashSet::new();
    mark_parser_view_adjustments(tokens, &mut blank_indices);
    while index < tokens.len() {
        let token = &tokens[index];
        let text = &source[token.pos()..token.pos() + token.len()];
        if blank_indices.remove(&index) {
            append_blank(&mut view, &mut map, text, token.pos());
            index += 1;
            continue;
        }
        if is_reference_ampersand(tokens, index, previous_significant) {
            append_mapped(&mut view, &mut map, b" ", token.pos());
            index += 1;
            continue;
        }
        if token.id() == tok::T_GOTO {
            let mut end = index;
            while end < tokens.len() && tokens[end].id() != i64::from(b';') {
                end += 1;
            }
            if end < tokens.len() {
                end += 1;
            }
            for skipped in &tokens[index..end] {
                let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                append_blank(&mut view, &mut map, text, skipped.pos());
            }
            index = end;
            previous_significant = i64::from(b';');
            continue;
        }
        if is_label_declaration(tokens, index, previous_significant) {
            let text = &source[token.pos()..token.pos() + token.len()];
            append_blank(&mut view, &mut map, text, token.pos());
            let colon = next_significant_index(tokens, index).expect("label colon follows label");
            append_blank(&mut view, &mut map, b":", tokens[colon].pos());
            index = colon + 1;
            continue;
        }
        if token.id() == i64::from(b'$')
            && next_significant_index(tokens, index).is_some_and(|next| {
                let id = tokens[next].id();
                id == i64::from(b'$')
                    || id == tok::T_VARIABLE
                    || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
            })
        {
            append_blank(&mut view, &mut map, b"$", token.pos());
            index += 1;
            continue;
        }
        if token.id() == i64::from(b'$')
            && next_significant_index(tokens, index).is_some_and(|next| tokens[next].id() == i64::from(b'{'))
        {
            append_mapped(&mut view, &mut map, b"$x", token.pos());
            let brace = next_significant_index(tokens, index).unwrap();
            append_blank(&mut view, &mut map, b"{", tokens[brace].pos());
            let mut end = brace + 1;
            let mut depth = 1usize;
            while end < tokens.len() {
                if tokens[end].id() == i64::from(b'{') {
                    depth += 1;
                } else if tokens[end].id() == i64::from(b'}') {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                end += 1;
            }
            for skipped in &tokens[brace + 1..end.min(tokens.len())] {
                let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                append_blank(&mut view, &mut map, text, skipped.pos());
            }
            if end < tokens.len() {
                append_blank(&mut view, &mut map, b"}", tokens[end].pos());
            }
            index = end.saturating_add(1);
            continue;
        }
        if token.id() == tok::T_DOLLAR_OPEN_CURLY_BRACES {
            append_mapped(&mut view, &mut map, b"$x", token.pos());
            let mut end = index + 1;
            let mut depth = 1usize;
            while end < tokens.len() && depth > 0 {
                if tokens[end].id() == i64::from(b'{') {
                    depth += 1;
                } else if tokens[end].id() == i64::from(b'}') {
                    depth -= 1;
                }
                if depth == 0 {
                    break;
                }
                end += 1;
            }
            for skipped in &tokens[index + 1..end.min(tokens.len())] {
                let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                append_blank(&mut view, &mut map, text, skipped.pos());
            }
            if end < tokens.len() {
                append_blank(&mut view, &mut map, b"}", tokens[end].pos());
            }
            index = end.saturating_add(1);
            continue;
        }
        if token.id() == tok::T_ELLIPSIS
            && matches!(
                previous_significant,
                t if t == i64::from(b')')
                    || t == i64::from(b']')
                    || t == i64::from(b'(')
                    || t == tok::T_VARIABLE
            )
            && next_significant_index(tokens, index)
                .is_some_and(|next| tokens[next].id() == i64::from(b')'))
        {
            append_blank(&mut view, &mut map, b"...", token.pos());
            index += 1;
            continue;
        }
        if matches!(
            token.id(),
            t if t == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                || t == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
        ) {
            let previous = previous_significant_index(tokens, index).map(|index| tokens[index].id());
            let next = next_significant_index(tokens, index).map(|index| tokens[index].id());
            if previous == Some(tok::T_STRING) && next == Some(tok::T_STRING) {
                append_mapped(&mut view, &mut map, b"|", token.pos());
                index += 1;
                continue;
            }
        }
        if token.id() == tok::T_DOUBLE_COLON {
            let Some(previous) = previous_significant_index(tokens, index) else {
                append_mapped(&mut view, &mut map, b"::", token.pos());
                index += 1;
                continue;
            };
            let before_previous = previous_significant_index(tokens, previous);
            let chained = before_previous.is_some_and(|before| {
                let id = tokens[before].id();
                id == tok::T_DOUBLE_COLON
                    || id == tok::T_OBJECT_OPERATOR
                    || id == tok::T_NULLSAFE_OBJECT_OPERATOR
            });
            if !chained
                && tokens[previous].id() == tok::T_VARIABLE
                && next_significant_index(tokens, index)
                    .is_some_and(|next| tokens[next].id() != tok::T_CLASS)
            {
                append_mapped(&mut view, &mut map, b"->", token.pos());
                index += 1;
                continue;
            }
            if chained && tokens[previous].id() == tok::T_VARIABLE {
                append_blank(&mut view, &mut map, b"::", token.pos());
                if let Some(next) = next_significant_index(tokens, index) {
                    if tokens[next].id() == tok::T_VARIABLE || tokens[next].id() == tok::T_STRING {
                        let text = &source[tokens[next].pos()..tokens[next].pos() + tokens[next].len()];
                        append_blank(&mut view, &mut map, text, tokens[next].pos());
                        index = next + 1;
                        continue;
                    }
                }
                index += 1;
                continue;
            }
            if chained {
                append_mapped(&mut view, &mut map, b"->", token.pos());
                index += 1;
                continue;
            }
        }
        if token.id() == i64::from(b'{')
            && previous_significant == tok::T_DOUBLE_COLON
        {
            append_blank(&mut view, &mut map, b"{", token.pos());
            let mut end = index + 1;
            let mut depth = 1usize;
            while end < tokens.len() && depth > 0 {
                if tokens[end].id() == i64::from(b'{') {
                    depth += 1;
                } else if tokens[end].id() == i64::from(b'}') {
                    depth -= 1;
                }
                if depth == 0 {
                    break;
                }
                let text = &source[tokens[end].pos()..tokens[end].pos() + tokens[end].len()];
                if tokens[end].id() == tok::T_CONSTANT_ENCAPSED_STRING
                    && text.len() >= 2
                {
                    let mut normalized = text.to_vec();
                    normalized[0] = b' ';
                    let last = normalized.len() - 1;
                    normalized[last] = b' ';
                    append_mapped(&mut view, &mut map, &normalized, tokens[end].pos());
                } else {
                    append_mapped(&mut view, &mut map, text, tokens[end].pos());
                }
                end += 1;
            }
            if end < tokens.len() {
                append_blank(&mut view, &mut map, b"}", tokens[end].pos());
            }
            index = end.saturating_add(1);
            continue;
        }
        if token.id() == tok::T_NEW
            && next_significant_index(tokens, index).is_some_and(|next| tokens[next].id() == i64::from(b'('))
        {
            let open = next_significant_index(tokens, index).unwrap();
            if let Some(close) = matching_paren(tokens, open) {
                append_blank(&mut view, &mut map, b"(", tokens[open].pos());
                blank_indices.insert(close);
                index = open + 1;
                continue;
            }
        }
        if token.id() == i64::from(b'`') {
            index = append_backtick_string(&mut view, &mut map, source, tokens, index);
            previous_significant = token.id();
            index += 1;
            continue;
        }
        if token.id() == tok::T_OPEN_TAG || token.id() == tok::T_INLINE_HTML {
            append_blank(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_OPEN_TAG_WITH_ECHO {
            append_mapped(&mut view, &mut map, b"echo ", token.pos());
        } else if token.id() == tok::T_CLOSE_TAG {
            append_mapped(&mut view, &mut map, b";", token.pos());
            append_blank(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_HALT_COMPILER {
            index = skip_halt_compiler(&mut view, &mut map, source, tokens, index);
        } else if matches!(token.id(), tok::T_UNSET_CAST | tok::T_VOID_CAST | tok::T_STRING_CAST) {
            append_safe_cast(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_CONSTANT_ENCAPSED_STRING {
            append_string(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_DNUMBER && text.ends_with(b".") {
            let mut normalized = text.to_vec();
            if let Some(last) = normalized.last_mut() {
                *last = b' ';
            }
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if token.id() == tok::T_START_HEREDOC
            && text.len() >= 4
            && matches!(text[0], b'b' | b'B')
            && &text[1..4] == b"<<<"
        {
            let mut normalized = text.to_vec();
            normalized[0] = b' ';
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if token.id() == tok::T_NUM_STRING && !text.iter().all(u8::is_ascii_digit) {
            let normalized = vec![b'0'; text.len()];
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if token.id() == tok::T_STRING && text.eq_ignore_ascii_case(b"enum") {
            append_mapped(&mut view, &mut map, b"zenum", token.pos());
        } else if text.len() >= 2
            && matches!(text[0], b'b' | b'B')
            && matches!(text[1], b'\'' | b'"')
        {
            let mut normalized = text.to_vec();
            normalized[0] = b' ';
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else {
            append_mapped(&mut view, &mut map, text, token.pos());
        }
        if !is_trivia_token(token.id()) {
            previous_significant = token.id();
        }
        index += 1;
    }
    normalize_parse_newlines(&mut view);
    (sanitize_view(view), map)
}

/// Replaces each invalid UTF-8 byte with one ASCII placeholder, preserving byte offsets.
fn sanitize_view(view: Vec<u8>) -> String {
    match String::from_utf8(view) {
        Ok(text) => return text,
        Err(error) => return sanitize_invalid_utf8(error.into_bytes()),
    }
}

/// Replaces invalid UTF-8 bytes with ASCII placeholders while preserving byte offsets.
fn sanitize_invalid_utf8(view: Vec<u8>) -> String {
    let mut out = String::with_capacity(view.len());
    let mut offset = 0;
    while offset < view.len() {
        match std::str::from_utf8(&view[offset..]) {
            Ok(valid) => {
                out.push_str(valid);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                out.push_str(std::str::from_utf8(&view[offset..offset + valid])
                    .expect("valid_up_to always ends on a UTF-8 boundary"));
                let invalid = error.error_len().unwrap_or(view.len() - offset - valid);
                for _ in 0..invalid {
                    out.push('x');
                }
                offset += valid + invalid;
            }
        }
    }
    out
}

/// Marks every parser-view normalization that needs surrounding context in one token pass.
///
/// Combines static multi-declarations, DNF parentheses and abstract properties so the
/// validation view scans the token stream once instead of three times, and matches
/// parentheses with a stack instead of rescanning from every `(`.
fn mark_parser_view_adjustments<T: SourceToken>(tokens: &[T], blank: &mut HashSet<usize>) {
    let mut parens: Vec<(usize, u32)> = Vec::new();
    let mut ampersands = 0u32;
    for (index, token) in tokens.iter().enumerate() {
        let id = token.id();
        if id == i64::from(b'(') {
            parens.push((index, ampersands));
        } else if id == i64::from(b')') {
            let Some((open, open_ampersands)) = parens.pop() else { continue; };
            if ampersands == open_ampersands {
                continue;
            }
            let Some(after) = next_significant_index(tokens, index) else { continue; };
            let after_id = tokens[after].id();
            if after_id == i64::from(b'|')
                || after_id == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
                || after_id == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
            {
                blank.insert(open);
                blank.insert(index);
            }
        } else if id == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
            || id == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
        {
            ampersands += 1;
        } else if id == tok::T_STATIC {
            mark_static_multi_declaration(tokens, index, blank);
        } else if id == tok::T_ABSTRACT {
            mark_abstract_property(tokens, index, blank);
        }
    }
}

/// Marks the declarations after the first variable in one `static $a = ..., $b;` statement.
fn mark_static_multi_declaration<T: SourceToken>(
    tokens: &[T],
    index: usize,
    blank: &mut HashSet<usize>,
) {
    let Some(next) = next_significant_index(tokens, index) else { return; };
    if tokens[next].id() != tok::T_VARIABLE {
        return;
    }
    let mut depth = 0usize;
    let mut comma = None;
    let mut cursor = next + 1;
    while cursor < tokens.len() {
        match tokens[cursor].id() {
            id if id == i64::from(b'(') || id == i64::from(b'[') || id == i64::from(b'{') => depth += 1,
            id if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') => depth = depth.saturating_sub(1),
            id if id == i64::from(b',') && depth == 0 => { comma = Some(cursor); break; }
            id if id == i64::from(b';') && depth == 0 => break,
            _ => {}
        }
        cursor += 1;
    }
    if comma.is_none() {
        return;
    }
    while cursor < tokens.len() && tokens[cursor].id() != i64::from(b';') {
        blank.insert(cursor);
        cursor += 1;
    }
}

/// Marks one abstract property-hook declaration the parser does not model.
fn mark_abstract_property<T: SourceToken>(
    tokens: &[T],
    index: usize,
    blank: &mut HashSet<usize>,
) {
    let mut cursor = index + 1;
    while cursor < tokens.len() {
        let id = tokens[cursor].id();
        if id == tok::T_VARIABLE {
            break;
        }
        if [tok::T_FUNCTION, tok::T_FN, i64::from(b';')].contains(&id) {
            cursor = tokens.len();
            break;
        }
        cursor += 1;
    }
    if cursor >= tokens.len() || tokens[cursor].id() != tok::T_VARIABLE {
        return;
    }
    while cursor < tokens.len() && tokens[cursor].id() != i64::from(b'{') {
        if tokens[cursor].id() == i64::from(b';') {
            break;
        }
        cursor += 1;
    }
    if cursor >= tokens.len() || tokens[cursor].id() != i64::from(b'{') {
        return;
    }
    let Some(close) = matching_brace(tokens, cursor) else { return; };
    let start = previous_significant_index(tokens, index).filter(|previous| {
        matches!(
            tokens[*previous].id(),
            id if id == tok::T_PUBLIC
                || id == tok::T_PROTECTED
                || id == tok::T_PRIVATE
                || id == tok::T_STATIC
                || id == tok::T_READONLY
        )
    }).unwrap_or(index);
    for skipped in start..=close {
        blank.insert(skipped);
    }
}

/// Finds the closing brace matching the opening brace at `open`.
fn matching_brace<T: SourceToken>(tokens: &[T], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..tokens.len() {
        if tokens[index].id() == i64::from(b'{') {
            depth += 1;
        } else if tokens[index].id() == i64::from(b'}') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Returns the next non-trivia token index after `index`.
fn next_significant_index<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
    let mut cursor = index + 1;
    while cursor < tokens.len() {
        if !is_trivia_token(tokens[cursor].id()) {
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

/// Returns whether the token at `index` is a statement label (`name:`).
fn is_label_declaration<T: SourceToken>(tokens: &[T], index: usize, previous_significant: i64) -> bool {
    let Some(next) = next_significant_index(tokens, index) else {
        return false;
    };
    if tokens[next].id() != i64::from(b':') {
        return false;
    }
    if previous_significant != 0
        && previous_significant != i64::from(b';')
        && previous_significant != i64::from(b'{')
        && previous_significant != i64::from(b'}')
    {
        return false;
    }
    tokens[index].id() == tok::T_STRING
}

/// Finds the closing parenthesis matching the opening parenthesis at `open`.
fn matching_paren<T: SourceToken>(tokens: &[T], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..tokens.len() {
        if tokens[index].id() == i64::from(b'(') {
            depth += 1;
        } else if tokens[index].id() == i64::from(b')') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Rejects `list` where PHP's grammar reserves it, while keeping member/declaration uses.
///
/// elephc's lexer keeps `list` as an identifier, so the parser accepts it in expression and
/// declaration positions where reference PHP reports a ParseError. The allowed contexts below
/// are exactly the semi-reserved ones: member names, method/class-constant names, enum cases,
/// named arguments, trait adaptations, and the `list(...)` construct.
fn reserved_list_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    let list = tok::T_LIST;
    if !tokens.iter().any(|token| token.id() == list) {
        return None;
    }
    let enclosing = enclosing_braces(tokens);
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != list {
            continue;
        }
        let previous = previous_significant_index(tokens, index);
        let next = next_significant_index(tokens, index);
        let previous_id = previous.map_or(0, |index| tokens[index].id());
        let next_id = next.map_or(0, |index| tokens[index].id());
        let in_class = enclosing[index] != u32::MAX
            && enclosing_brace_is_class(tokens, enclosing[index] as usize);
        let allowed = if next_id == i64::from(b'(') {
            previous_id != tok::T_FUNCTION || in_class
        } else if previous_id == tok::T_FUNCTION {
            in_class
        } else if previous_id == tok::T_CONST {
            in_class
        } else if previous_id == tok::T_CASE {
            in_class
        } else if next_id == i64::from(b':') {
            true
        } else if [
            tok::T_OBJECT_OPERATOR,
            tok::T_NULLSAFE_OBJECT_OPERATOR,
            tok::T_DOUBLE_COLON,
        ]
        .contains(&previous_id)
        {
            true
        } else {
            next_id == tok::T_AS
        };
        if !allowed {
            let text = &source[token.pos()..token.pos() + token.len()];
            return Some(ValidationError {
                message: format!(
                    "syntax error, unexpected token \"{}\"",
                    String::from_utf8_lossy(text)
                ),
                pos: token.pos(),
                lexical: false,
            });
        }
    }
    None
}

/// Precomputes the nearest open brace for every token in one forward pass.
///
/// `u32::MAX` means "no enclosing brace". The engine caps source length at `u32::MAX`, so a
/// token index always fits.
fn enclosing_braces<T: SourceToken>(tokens: &[T]) -> Vec<u32> {
    let mut stack: Vec<u32> = Vec::new();
    let mut enclosing = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        enclosing.push(stack.last().copied().unwrap_or(u32::MAX));
        if token.id() == i64::from(b'{') {
            stack.push(u32::try_from(index).unwrap_or(u32::MAX));
        } else if token.id() == i64::from(b'}') {
            stack.pop();
        }
    }
    enclosing
}

/// Returns whether one precomputed enclosing brace opens a class-like body.
fn enclosing_brace_is_class<T: SourceToken>(tokens: &[T], open: usize) -> bool {
    let mut cursor = open;
    while cursor > 0 {
        cursor -= 1;
        let id = tokens[cursor].id();
        if [
            tok::T_CLASS,
            tok::T_INTERFACE,
            tok::T_TRAIT,
            tok::T_ENUM,
        ]
        .contains(&id)
        {
            return true;
        }
        if [tok::T_FUNCTION, tok::T_FN].contains(&id)
            || [i64::from(b';'), i64::from(b'{'), i64::from(b'}')].contains(&id)
        {
            return false;
        }
    }
    false
}

/// Converts a lone CR into LF so elephc's line-based parser sees Zend's line breaks.
fn normalize_parse_newlines(view: &mut [u8]) {
    for index in 0..view.len() {
        if view[index] == b'\r' && view.get(index + 1) != Some(&b'\n') {
            view[index] = b'\n';
        }
    }
}

/// Returns whether one ampersand is a PHP reference marker the parser view should remove.
///
/// Only declaration, capture, foreach and literal contexts accept a reference marker. A call
/// argument such as `foo(&$a)` is a PHP ParseError, so its `&` must stay in the view and let the
/// parser reject it. The same rule keeps `($a & $b)` as the bitwise operator.
fn is_reference_ampersand<T: SourceToken>(tokens: &[T], index: usize, previous_significant: i64) -> bool {
    let ampersand = tokens[index].id() == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
        || tokens[index].id() == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG;
    if !ampersand {
        return false;
    }
    if [
        tok::T_DOUBLE_ARROW,
        tok::T_FUNCTION,
        tok::T_AS,
        tok::T_RETURN,
    ]
    .contains(&previous_significant)
        || previous_significant == i64::from(b'=')
        || previous_significant == i64::from(b'[')
    {
        return true;
    }
    if previous_significant != i64::from(b'(') && previous_significant != i64::from(b',') {
        return false;
    }
    let Some(open) = enclosing_paren(tokens, index) else {
        return false;
    };
    let before = previous_significant_index(tokens, open);
    let Some(before) = before else {
        return false;
    };
    if [
        tok::T_FUNCTION,
        tok::T_FN,
        tok::T_USE,
        tok::T_ARRAY,
        tok::T_LIST,
    ]
    .contains(&tokens[before].id())
    {
        return true;
    }
    // A named function or method declaration: `function foo(&$x)`.
    if tokens[before].id() == tok::T_STRING
        || tokens[before].id() == tok::T_IDENTIFIER
    {
        return previous_significant_index(tokens, before)
            .is_some_and(|prior| tokens[prior].id() == tok::T_FUNCTION);
    }
    false
}

/// Finds the opening parenthesis that encloses the token at `index`.
fn enclosing_paren<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        let id = tokens[cursor].id();
        if id == i64::from(b')') {
            depth += 1;
        } else if id == i64::from(b'(') {
            if depth == 0 {
                return Some(cursor);
            }
            depth -= 1;
        }
    }
    None
}

/// Returns the index of the previous non-trivia token before `index`.
fn previous_significant_index<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        if !is_trivia_token(tokens[cursor].id()) {
            return Some(cursor);
        }
    }
    None
}

/// Returns whether a token is trivia for previous-significant-token tracking.
fn is_trivia_token(id: i64) -> bool {
    id == tok::T_WHITESPACE || id == tok::T_COMMENT || id == tok::T_DOC_COMMENT
}

/// Appends a backtick expression as a plain double-quoted string for parser validation.
fn append_backtick_string<T: SourceToken>(
    view: &mut Vec<u8>,
    map: &mut Vec<u32>,
    source: &[u8],
    tokens: &[T],
    open: usize,
) -> usize {
    let close = tokens[open + 1..]
        .iter()
        .position(|token| token.id() == i64::from(b'`'))
        .map(|offset| open + 1 + offset);
    append_mapped(view, map, b"\"", tokens[open].pos());
    let end = close.unwrap_or(tokens.len());
    for token in &tokens[open + 1..end] {
        let text = &source[token.pos()..token.pos() + token.len()];
        for (offset, byte) in text.iter().copied().enumerate() {
            view.push(if matches!(byte, b'\n' | b'\r') {
                byte
            } else if matches!(byte, b'"' | b'\\') {
                b' '
            } else {
                byte
            });
            map.push(u32::try_from(token.pos() + offset).unwrap_or(u32::MAX));
        }
    }
    if let Some(close) = close {
        append_mapped(view, map, b"\"", tokens[close].pos());
        return close;
    }
    open
}

/// Appends a valid cast in the same number of view bytes, preserving offsets.
fn append_safe_cast(view: &mut Vec<u8>, map: &mut Vec<u32>, text: &[u8], source_pos: usize) {
    let safe = b"(int)";
    for (offset, byte) in text.iter().copied().enumerate() {
        view.push(if offset < safe.len() {
            safe[offset]
        } else if matches!(byte, b'\n' | b'\r') {
            byte
        } else {
            b' '
        });
        map.push(u32::try_from(source_pos + offset).unwrap_or(u32::MAX));
    }
}

/// Appends a string literal with any binary prefix blanked for parser validation.
fn append_string(view: &mut Vec<u8>, map: &mut Vec<u32>, text: &[u8], source_pos: usize) {
    if text.len() >= 2 && matches!(text[0], b'b' | b'B') && matches!(text[1], b'\'' | b'"') {
        let mut normalized = text.to_vec();
        normalized[0] = b' ';
        append_mapped(view, map, &normalized, source_pos);
    } else {
        append_mapped(view, map, text, source_pos);
    }
}

/// Blanks one token while preserving CR/LF bytes for line numbering.
fn append_blank(view: &mut Vec<u8>, map: &mut Vec<u32>, text: &[u8], source_pos: usize) {
    for (offset, byte) in text.iter().copied().enumerate() {
        view.push(if matches!(byte, b'\n' | b'\r') { byte } else { b' ' });
        map.push(u32::try_from(source_pos + offset).unwrap_or(u32::MAX));
    }
}

/// Appends bytes mapped to one source position.
fn append_mapped(view: &mut Vec<u8>, map: &mut Vec<u32>, bytes: &[u8], source_pos: usize) {
    view.extend_from_slice(bytes);
    map.resize(map.len() + bytes.len(), u32::try_from(source_pos).unwrap_or(u32::MAX));
}

/// Blanks a valid `__halt_compiler ( ) ;` header and returns the last consumed token index.
fn skip_halt_compiler<T: SourceToken>(
    view: &mut Vec<u8>,
    map: &mut Vec<u32>,
    source: &[u8],
    tokens: &[T],
    start: usize,
) -> usize {
    let mut index = start;
    let mut significant = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        let text = &source[token.pos()..token.pos() + token.len()];
        append_blank(view, map, text, token.pos());
        if !is_trivia(token.id()) {
            significant += 1;
            if significant == 3 {
                return index;
            }
        }
        index += 1;
    }
    tokens.len().saturating_sub(1)
}

/// Returns identifier feedback mapped from parser view spans to original source offsets.
fn mapped_names(
    view: &str, map: &[u32], lines: &[usize], names: Vec<(Span, String)>, source: &[u8],
) -> Vec<usize> {
    let mut result: Vec<usize> = names
        .into_iter()
        .filter_map(|(span, spelling)| {
            let view_pos = view_offset(view, lines, span.line, span.col);
            let source_pos = map.get(view_pos).copied().map(|pos| pos as usize).unwrap_or(source.len()).min(source.len());
            let text = source.get(source_pos..)?;
            text.get(..spelling.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(spelling.as_bytes()))
                .then_some(source_pos)
        })
        .collect();
    result.sort_unstable();
    result.dedup();
    result
}

/// Builds a PHP-style parse error from the AST parser's first diagnostic.
fn parser_error<T: SourceToken>(
    view: &str,
    map: &[u32],
    lines: &[usize],
    source: &[u8],
    tokens: &[T],
    error: &CompileError,
) -> ValidationError {
    let view_pos = view_offset(view, lines, error.span.line, error.span.col);
    let source_pos = map.get(view_pos).copied().map(|pos| pos as usize).unwrap_or(source.len());
    if source_pos >= source.len() {
        return ValidationError { message: "syntax error, unexpected end of file".into(), pos: source.len(), lexical: false };
    }
    let token = tokens.iter().find(|token| token.pos() <= source_pos && source_pos < token.pos() + token.len());
    let message = match token {
        None => "syntax error, unexpected end of file".into(),
        Some(token) => {
            let text = &source[token.pos()..token.pos() + token.len()];
            if token.id() == tok::T_STRING {
                format!("syntax error, unexpected identifier \"{}\"", String::from_utf8_lossy(text))
            } else if text.len() <= 3 {
                format!("syntax error, unexpected token \"{}\"", String::from_utf8_lossy(text))
            } else {
                format!(
                    "syntax error, unexpected token \"{}\"",
                    token_name(token.id()).unwrap_or("UNKNOWN")
                )
            }
        }
    };
    ValidationError { message, pos: source_pos, lexical: false }
}

/// Returns the canonical `T_*` name for one token identifier on the cold diagnostic path.
fn token_name(id: i64) -> Option<&'static str> {
    elephc_builtin_contract::constants().iter().find_map(|constant| {
        if constant.module != elephc_builtin_contract::PhpModule::Tokenizer
            || !constant.name.starts_with("T_")
            || constant.name == "T_PAAMAYIM_NEKUDOTAYIM"
        {
            return None;
        }
        match constant.value {
            elephc_builtin_contract::ConstValue::Int(value) if value == id => Some(constant.name),
            _ => None,
        }
    })
}

/// Returns the byte offset of one one-based line/column in a UTF-8 parser view.
fn view_offset(view: &str, lines: &[usize], line: u32, col: u32) -> usize {
    let Some(&start) = lines.get(line.saturating_sub(1) as usize) else {
        return view.len();
    };
    if col <= 1 {
        return start;
    }
    view[start..]
        .char_indices()
        .nth((col - 1) as usize)
        .map_or(view.len(), |(offset, _)| start + offset)
}

/// Precomputes the byte offset of every one-based parser-view line.
fn line_starts(view: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(view.bytes().enumerate().filter_map(|(index, byte)| {
        (byte == b'\n').then_some(index + 1)
    }));
    starts
}

/// Returns whether a token is trivia for halt-compiler header accounting.
fn is_trivia(id: i64) -> bool {
    id == tok::T_WHITESPACE || id == tok::T_COMMENT || id == tok::T_DOC_COMMENT
}
