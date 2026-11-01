//! Purpose:
//! Parses qualified and unqualified PHP names used by statement-level syntax.
//!
//! Called from:
//! - Declaration, namespace, FFI, and object-oriented statement parsers.
//!
//! Key details:
//! - `enum` remains a soft keyword in class-like name positions.
//! - A leading `namespace\` is a relative name and resolves to a fully qualified name against
//!   the namespace being parsed (see `crate::parser::relative_names`).

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

/// Returns one segment of a name that follows a namespace separator.
///
/// PHP 8 lexes a namespaced name as ONE token, so a reserved word is an ordinary segment
/// anywhere after the first separator: `use Laravel\Prompts\Themes\Default\Renderer;` and
/// `new App\List\Item()` are valid. A leading segment still follows the identifier rules.
pub(crate) fn qualified_name_part_from_token(
    token: &Token,
    metadata: &TokenMetadata,
) -> Option<String> {
    name_part_from_token(token, metadata)
        .or_else(|| crate::parser::keyword_name::bareword_name_from_token(token, metadata))
}

/// Returns whether the token at `pos` starts a PHP class-like name.
pub(crate) fn name_starts_at(tokens: &[SpannedToken], pos: usize) -> bool {
    match tokens.get(pos) {
        Some((Token::Backslash, _)) => true,
        Some((Token::Namespace, _)) => crate::parser::relative_name_starts_at(tokens, pos),
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
    let mut parts = Vec::new();
    // `namespace\foo` is the relative name "foo in the current namespace" (#825), so it resolves
    // here to the fully qualified `\Current\Ns\foo`: no import alias or global fallback applies.
    let relative = crate::parser::relative_name_starts_at(tokens, *pos);
    if relative {
        kind = NameKind::FullyQualified;
        parts = crate::parser::current_namespace_parts();
        *pos += 2;
    } else if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
        kind = NameKind::FullyQualified;
        *pos += 1;
    }

    let namespace_len = parts.len();
    loop {
        // After a separator any reserved word is a segment (see `qualified_name_part_from_token`).
        let after_separator = !parts.is_empty() || kind == NameKind::FullyQualified;
        let part = tokens.get(*pos).and_then(|(token, metadata)| {
            if after_separator {
                qualified_name_part_from_token(token, metadata)
            } else {
                name_part_from_token(token, metadata)
            }
        });
        match part {
            Some(part) => {
                parts.push(part);
                *pos += 1;
            }
            _ if !relative && parts.len() == namespace_len => {
                return Err(CompileError::new(span, first_error))
            }
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
