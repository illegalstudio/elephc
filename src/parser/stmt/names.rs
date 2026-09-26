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
        _ => None,
    }
}

/// Returns the segment spelled by the token at `pos` inside a qualified name.
///
/// PHP 8 lexes a qualified name as one token, so a reserved word is an ordinary segment
/// there (`Demo\Namespace`, `Vendor\Default\Theme`). A keyword counts only when the name
/// already has a separator before it (`after_separator`) or has one right after it; alone, it
/// is still the keyword.
pub(crate) fn qualified_segment_at(
    tokens: &[SpannedToken],
    pos: usize,
    after_separator: bool,
) -> Option<String> {
    let (token, metadata) = tokens.get(pos)?;
    if let Some(part) = name_part_from_token(token, metadata) {
        return Some(part);
    }
    let separator_follows = matches!(tokens.get(pos + 1), Some((Token::Backslash, _)));
    if after_separator || separator_follows {
        return crate::parser::keyword_name::bareword_name_from_token(token, metadata);
    }
    None
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
    }

    let mut parts = Vec::new();
    loop {
        let after_separator = kind == NameKind::FullyQualified || !parts.is_empty();
        match qualified_segment_at(tokens, *pos, after_separator) {
            Some(part) => {
                parts.push(part);
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
