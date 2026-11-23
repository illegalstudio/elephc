//! Purpose:
//! Bridges TOKEN_PARSE validation to elephc's AST parser and applies Zend identifier feedback.
//!
//! Called from:
//! - `crate::syntax::php_tokenizer::tokenize` when the TOKEN_PARSE flag is set.
//!
//! Key details:
//! - The native lexer owns all token boundaries; this module never re-lexes PHP grammar.
//! - Parser diagnostics are mapped to PHP's parser-facing messages and source lines.
//! - Lexical errors found while scanning win when they occur before the parser's first error.

use crate::syntax::tokenizer_validation::{validate_tokens, TokenView};
use elephc_builtin_contract::token_const_id as id;
use crate::scanner::{attach_warning_lines, newline_count, LexError, LexWarning};

use super::{ParseError, Token, Warning};

/// Validates PHP source and rewrites accepted keyword identifiers to T_STRING.
pub(super) fn validate(
    source: &[u8],
    tokens: &mut [Token],
    lexical: &[LexError],
) -> Result<Vec<LexWarning>, ParseError> {
    let views: Vec<TokenView> = tokens
        .iter()
        .map(|token| TokenView {
            id: token.id,
            pos: token.pos,
            len: token.len,
            line: token.line,
        })
        .collect();
    let feedback = validate_tokens(source, &views);
    // Warnings the validator raised (declare(encoding), casts) exist even when the parse fails.
    let validation_warnings: Vec<LexWarning> = feedback
        .warnings
        .iter()
        .map(|warning| LexWarning {
            message: warning.message.clone(),
            pos: warning.pos,
            severity: warning.severity,
            parse_only: true,
        })
        .collect();
    let first_lexical = lexical.first();
    let first_parser = feedback.error.as_ref();
    if let Some(parser) = first_parser {
        if let Some(lexical) = first_lexical {
            if parser.lexical || lexical.pos <= parser.pos || same_token(tokens, lexical.pos, parser.pos) {
                let mut error = parse_error(source, lexical.pos, lexical.message.clone());
                error.warnings = warning_list(source, &validation_warnings);
                return Err(error);
            }
        }
        let mut error = parse_error(source, parser.pos, parser.message.clone());
        // Zend reports the last line consumed by a multiline `yield from` token. Its
        // byte offset still identifies the start of the token for diagnostic ordering.
        let yield_from = const { id("T_YIELD_FROM") };
        if let Some(token) = tokens.iter().find(|token| token.id == yield_from && token.pos == error.pos) {
            error.line += newline_count(&source[token.pos..token.pos + token.len]);
        }
        error.warnings = warning_list(source, &validation_warnings);
        return Err(error);
    }
    if let Some(error) = first_lexical {
        let mut error = parse_error(source, error.pos, error.message.clone());
        error.warnings = warning_list(source, &validation_warnings);
        return Err(error);
    }
    let string = id("T_STRING");
    let string_varname = id("T_STRING_VARNAME");
    let mut token_index = 0;
    for offset in feedback.names {
        while token_index < tokens.len() && tokens[token_index].pos < offset {
            token_index += 1;
        }
        let Some(token) = tokens.get_mut(token_index) else {
            break;
        };
        if token.pos == offset && token.id >= 256 && token.id != string_varname {
            token.id = string;
        }
    }
    Ok(validation_warnings)
}

/// Assigns one-based lines to the validator diagnostics a failed parse carries.
///
/// Reuses the successful path's single forward pass; rescanning the prefix per warning made
/// a file with many `declare(encoding=...)` directives quadratic under TOKEN_PARSE.
fn warning_list(source: &[u8], warnings: &[LexWarning]) -> Vec<Warning> {
    attach_warning_lines(source, warnings.to_vec())
}

/// Returns whether two byte offsets fall in the same tokenizer token.
fn same_token(tokens: &[Token], left: usize, right: usize) -> bool {
    tokens.iter().any(|token| {
        token.pos <= left && left < token.pos + token.len
            && token.pos <= right && right < token.pos + token.len
    })
}

/// Builds a catchable ParseError with the line derived from one source byte offset.
fn parse_error(source: &[u8], pos: usize, message: String) -> ParseError {
    let pos = pos.min(source.len());
    ParseError {
        message,
        line: 1 + newline_count(&source[..pos]),
        pos,
        warnings: Vec::new(),
    }
}
