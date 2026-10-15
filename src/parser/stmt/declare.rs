//! Purpose:
//! Parses PHP `declare` directives and their statement, braced, or alternative-syntax bodies.
//! Validates PHP's literal-value and `strict_types` placement/form restrictions.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()` when the current token is `declare`.
//!
//! Key details:
//! - `strict_types` is recorded on the parser's per-file source profile
//!   (`crate::source::declare_strict_types`), which stamps every statement parsed afterwards.
//!   PHP requires the directive to be a file's first statement, so "afterwards" is exactly
//!   "the rest of this file"; the type checker reads the stamp back per statement to pick
//!   between PHP's strict and coercive parameter binding.
//! - Every other directive (`ticks`, `encoding`) is compile-time syntax only.
//! - Bodies lower through `Synthetic` so they execute in the enclosing scope.
//! - PHP scopes `strict_types` per file, but the recorded flag is per compilation, so a
//!   program mixing strict and coercive files is checked as if every file were strict.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{Stmt, StmtKind};
use crate::span::Span;

use super::{
    expect_semicolon, expect_token, parse_block, parse_stmt, recover_to_statement_boundary,
};

/// Parses `declare(directive=literal, ...)` and lowers its effective body to `Synthetic`.
pub(super) fn parse_declare(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let declare_pos = *pos;
    *pos += 1;

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'declare'")?;
    let strict_types = parse_directives(tokens, pos, span)?;
    expect_token(
        tokens,
        pos,
        &Token::RParen,
        "Expected ')' after declare directives",
    )?;

    if strict_types.is_some() && declare_pos != 1 {
        return Err(CompileError::new(
            span,
            "strict_types declaration must be the very first statement in the script",
        ));
    }

    if matches!(
        tokens.get(*pos).map(|(token, _)| token),
        Some(Token::Semicolon)
    ) {
        *pos += 1;
        // Applied only once the directive has passed every placement and form check, so a
        // rejected `declare` never leaves the rest of the file typed under it.
        if let Some(enabled) = strict_types {
            crate::source::declare_strict_types(enabled);
        }
        return Ok(Stmt::new(StmtKind::Synthetic(Vec::new()), span));
    }

    if strict_types.is_some() {
        return Err(CompileError::new(
            span,
            "strict_types declaration must not use block mode",
        ));
    }

    let body = match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::LBrace) => parse_block(tokens, pos)?,
        Some(Token::Colon) => parse_alternative_body(tokens, pos)?,
        Some(Token::Eof) | None => {
            return Err(CompileError::new(
                span,
                "Expected a statement after declare(...)",
            ));
        }
        _ => vec![parse_stmt(tokens, pos)?],
    };

    Ok(Stmt::new(StmtKind::Synthetic(body), span))
}

/// Parses one or more directive/literal pairs.
///
/// Returns `Some(true)` for `strict_types=1`, `Some(false)` for `strict_types=0`, and `None`
/// when the list holds no `strict_types` directive at all. The caller needs the three-way answer
/// because only a present directive is subject to PHP's placement and block-form restrictions.
fn parse_directives(
    tokens: &[SpannedToken],
    pos: &mut usize,
    declare_span: Span,
) -> Result<Option<bool>, CompileError> {
    let mut strict_types = None;

    loop {
        let (name, name_span) = match tokens.get(*pos) {
            Some((Token::Identifier(name), metadata)) => (name.clone(), metadata.span),
            _ => {
                return Err(CompileError::new(
                    declare_span,
                    "Expected a directive name in 'declare(...)'",
                ));
            }
        };
        *pos += 1;

        expect_token(
            tokens,
            pos,
            &Token::Assign,
            "Expected '=' after declare directive name",
        )?;
        let integer_value = parse_literal_value(tokens, pos, &name, name_span)?;

        if !matches!(
            tokens.get(*pos).map(|(token, _)| token),
            Some(Token::Comma | Token::RParen)
        ) {
            return Err(CompileError::new(
                name_span,
                &format!("declare({}) value must be a literal", name),
            ));
        }

        if name.eq_ignore_ascii_case("strict_types") {
            match integer_value {
                Some(0) => strict_types = Some(false),
                Some(1) => strict_types = Some(true),
                _ => {
                    return Err(CompileError::new(
                        name_span,
                        "strict_types declaration must have 0 or 1 as its value",
                    ));
                }
            }
        }

        if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Comma)) {
            break;
        }
        *pos += 1;
    }

    Ok(strict_types)
}

/// Consumes a PHP declare literal and returns its integer value when it is an integer.
fn parse_literal_value(
    tokens: &[SpannedToken],
    pos: &mut usize,
    directive: &str,
    directive_span: Span,
) -> Result<Option<i64>, CompileError> {
    match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::IntLiteral(value)) => {
            let value = *value;
            *pos += 1;
            Ok(Some(value))
        }
        Some(Token::FloatLiteral(_) | Token::StringLiteral(_)) => {
            *pos += 1;
            Ok(None)
        }
        _ => Err(CompileError::new(
            directive_span,
            &format!("declare({}) value must be a literal", directive),
        )),
    }
}

/// Parses `: ... enddeclare;`, collecting nested statement errors before closing the block.
fn parse_alternative_body(
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<Vec<Stmt>, CompileError> {
    *pos += 1;
    let mut body = Vec::new();
    let mut errors = Vec::new();

    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::EndDeclare | Token::Eof) {
        match parse_stmt(tokens, pos) {
            Ok(stmt) => body.push(stmt),
            Err(error) => {
                errors.extend(error.flatten());
                recover_to_statement_boundary(tokens, pos);
            }
        }
    }

    expect_token(
        tokens,
        pos,
        &Token::EndDeclare,
        "Expected 'enddeclare' after declare block",
    )?;
    expect_semicolon(tokens, pos)?;

    if errors.is_empty() {
        Ok(body)
    } else {
        Err(CompileError::from_many(errors))
    }
}
