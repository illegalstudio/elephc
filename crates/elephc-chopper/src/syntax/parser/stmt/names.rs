//! Purpose:
//! Parses qualified and unqualified PHP names used by statement-level syntax.
//!
//! Called from:
//! - Declaration, namespace, FFI, and object-oriented statement parsers.
//!
//! Key details:
//! - `enum` remains a soft keyword in class-like name positions.
//! - A leading `namespace\` is a relative name and resolves to a fully qualified name against
//!   the namespace being parsed (see `crate::syntax::parser::relative_names`).

use crate::syntax::parser::validation_only;
use crate::syntax::errors::CompileError;
use crate::syntax::lexer::{SpannedToken, Token, TokenMetadata};
use crate::syntax::names::{Name, NameKind};
use crate::syntax::span::Span;

/// Converts a token accepted as an ordinary PHP name segment to its source spelling.
pub fn name_part_from_token(
    token: &Token,
    metadata: &TokenMetadata,
) -> Option<String> {
    match token {
        Token::Identifier(name) => Some(name.clone()),
        Token::Enum => crate::syntax::parser::keyword_name::bareword_name_from_token(token, metadata),
        _ => super::token_as_import_name(token, metadata),
    }
}

/// Returns whether the token at `pos` starts a PHP class-like name.
pub fn name_starts_at(tokens: &[SpannedToken], pos: usize) -> bool {
    match tokens.get(pos) {
        Some((Token::Backslash, _)) => true,
        Some((Token::Namespace, _)) => crate::syntax::parser::relative_name_starts_at(tokens, pos),
        Some((token, metadata)) => name_part_from_token(token, metadata).is_some(),
        None => false,
    }
}

/// Parses one unqualified class-like declaration name.
pub fn parse_unqualified_name(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    error: &str,
) -> Result<String, CompileError> {
    let Some((token, metadata)) = tokens.get(*pos) else {
        return Err(CompileError::new(span, error));
    };
    let Some(name) = name_part_from_token(token, metadata) else {
        // PHP reports the offending token itself, with its fixed expectation.
        if validation_only() {
            return Err(CompileError::new(metadata.span, "Expected declaration name"));
        }
        if crate::syntax::parser::runtime_fragment() && token.word_spelling(metadata).is_some() {
            return Err(CompileError::runtime_unsupported(metadata.span, error));
        }
        return Err(CompileError::new(span, error));
    };
    if validation_only() && crate::syntax::parser::keyword_name::is_reserved_construct_name(&name) {
        return Err(CompileError::new(metadata.span, "Expected declaration name"));
    }
    *pos += 1;
    Ok(name)
}

/// Parses a PHP qualified or unqualified name from the token stream.
pub fn parse_name(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    first_error: &str,
) -> Result<Name, CompileError> {
    let mut kind = NameKind::Unqualified;
    let mut parts = Vec::new();
    // `namespace\foo` is the relative name "foo in the current namespace" (#825), so it resolves
    // here to the fully qualified `\Current\Ns\foo`: no import alias or global fallback applies.
    let relative = crate::syntax::parser::relative_name_starts_at(tokens, *pos);
    if relative {
        kind = NameKind::FullyQualified;
        parts = crate::syntax::parser::current_namespace_parts();
        *pos += 2;
    } else if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
        kind = NameKind::FullyQualified;
        *pos += 1;
    }

    let namespace_len = parts.len();
    loop {
        match tokens.get(*pos) {
            Some((token, metadata)) if name_part_from_token(token, metadata).is_some() => {
                parts.push(
                    name_part_from_token(token, metadata)
                        .expect("name part was checked immediately above"),
                );
                *pos += 1;
            }
            _ if !relative && parts.len() == namespace_len => {
                if crate::syntax::parser::runtime_fragment() && tokens.get(*pos)
                    .is_some_and(|(token, metadata)| token.word_spelling(metadata).is_some())
                {
                    return Err(CompileError::runtime_unsupported(tokens[*pos].1.span, first_error));
                }
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
