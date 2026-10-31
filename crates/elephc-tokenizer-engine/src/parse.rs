//! Purpose:
//! Bridges TOKEN_PARSE validation to elephc's AST parser and applies Zend identifier feedback.
//!
//! Called from:
//! - `crate::tokenize` only when the TOKEN_PARSE flag is set.
//!
//! Key details:
//! - The native lexer owns all token boundaries; this module never re-lexes PHP grammar.
//! - Parser diagnostics are mapped to PHP's parser-facing messages and source lines.
//! - Lexical errors found while scanning win when they occur before the parser's first error.

use elephc_syntax_validate::tokenizer_validation::SourceToken;

use crate::{id, newline_count, LexError, ParseError, Token};

impl SourceToken for Token {
    /// Returns the PHP token identifier.
    fn id(&self) -> i64 { self.id }
    /// Returns the zero-based byte offset in the original source.
    fn pos(&self) -> usize { self.pos }
    /// Returns the byte length in the original source.
    fn len(&self) -> usize { self.len }
}

/// Validates PHP source and rewrites accepted keyword identifiers to T_STRING.
pub(super) fn validate(
    source: &[u8],
    tokens: &mut [Token],
    lexical: &[LexError],
) -> Result<(), ParseError> {
    let feedback = elephc_syntax_validate::tokenizer_validation::validate(source, tokens);
    let first_lexical = lexical.first();
    let first_parser = feedback.error.as_ref();
    if let Some(parser) = first_parser {
        if let Some(lexical) = first_lexical {
            if parser.lexical || lexical.pos <= parser.pos || same_token(tokens, lexical.pos, parser.pos) {
                return Err(parse_error(source, lexical.pos, lexical.message.clone()));
            }
        }
        return Err(parse_error(source, parser.pos, parser.message.clone()));
    }
    if let Some(error) = first_lexical {
        return Err(parse_error(source, error.pos, error.message.clone()));
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
    Ok(())
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
    }
}
