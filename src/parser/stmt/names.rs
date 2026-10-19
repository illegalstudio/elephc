//! Purpose:
//! Parses qualified and unqualified PHP names used by statement-level syntax.
//!
//! Called from:
//! - Declaration, namespace, FFI, and object-oriented statement parsers.
//!
//! Key details:
//! - `enum` remains a soft keyword in class-like name positions.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token, TokenMetadata};
use crate::names::{Name, NameKind};
use crate::span::Span;

/// Converts a token accepted as an ordinary PHP name segment to its source spelling.
pub(crate) fn name_part_from_token(
    token: &Token,
    metadata: &TokenMetadata,
) -> Option<String> {
    match token {
        Token::Identifier(name) => Some(name.clone()),
        Token::Enum => crate::parser::keyword_name::bareword_name_from_token(token, metadata),
        _ => super::token_as_import_name(token, metadata),
    }
}

/// Returns whether the token at `pos` starts a PHP class-like name.
pub(crate) fn name_starts_at(tokens: &[SpannedToken], pos: usize) -> bool {
    match tokens.get(pos) {
        Some((Token::Backslash, _)) => true,
        Some((token, metadata)) => name_part_from_token(token, metadata).is_some(),
        None => false,
    }
}

/// Parses one unqualified class-like declaration name.
pub(crate) fn parse_unqualified_name(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    error: &str,
) -> Result<String, CompileError> {
    let Some((token, metadata)) = tokens.get(*pos) else {
        return Err(CompileError::new(span, error));
    };
    let name = name_part_from_token(token, metadata)
        .ok_or_else(|| CompileError::new(span, error))?;
    *pos += 1;
    Ok(name)
}

/// Parses a PHP qualified or unqualified name from the token stream.
pub(crate) fn parse_name(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    first_error: &str,
) -> Result<Name, CompileError> {
    let mut kind = NameKind::Unqualified;
    if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
        kind = NameKind::FullyQualified;
        *pos += 1;
        if let Some((token @ (Token::Self_ | Token::Parent | Token::Static), metadata)) =
            tokens.get(*pos)
        {
            let word = token.word_spelling(metadata).unwrap_or_default();
            return Err(CompileError::new(
                span,
                &format!("'\\{word}' is an invalid class name"),
            ));
        }
    }

    let mut parts = Vec::new();
    loop {
        match tokens.get(*pos) {
            Some((token, metadata)) if name_part_from_token(token, metadata).is_some() => {
                parts.push(
                    name_part_from_token(token, metadata)
                        .expect("name part was checked immediately above"),
                );
                *pos += 1;
            }
            _ if parts.is_empty() => return Err(CompileError::new(span, first_error)),
            _ => {
                return Err(CompileError::new(
                    span,
                    "Expected identifier after '\\' in qualified name",
                ))
            }
        }

        if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
            if kind != NameKind::FullyQualified {
                kind = NameKind::Qualified;
            }
            *pos += 1;
            continue;
        }
        break;
    }

    Ok(Name::from_parts(kind, parts))
}
