//! Purpose:
//! Parses PHP `declare` directives and their statement, braced, or alternative-syntax bodies.
//! Validates PHP's literal-value and `strict_types` placement/form restrictions.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()` when the current token is `declare`.
//!
//! Key details:
//! - Directives are compile-time syntax only because elephc always uses strict typing.
//! - Bodies lower through `Synthetic` so they execute in the enclosing scope.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Stmt, StmtKind};
use crate::span::Span;

use super::{
    expect_semicolon, expect_token, parse_block, parse_stmt, recover_to_statement_boundary,
};

/// Parses `declare(directive=literal, ...)` and lowers its effective body to `Synthetic`.
pub(super) fn parse_declare(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let declare_pos = *pos;
    *pos += 1;

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'declare'")?;
    let has_strict_types = parse_directives(tokens, pos, span)?;
    expect_token(
        tokens,
        pos,
        &Token::RParen,
        "Expected ')' after declare directives",
    )?;

    if has_strict_types && declare_pos != 1 {
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
        return Ok(Stmt::new(StmtKind::Synthetic(Vec::new()), span));
    }

    if has_strict_types {
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

/// Parses one or more directive/literal pairs and reports whether `strict_types` occurred.
fn parse_directives(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    declare_span: Span,
) -> Result<bool, CompileError> {
    let mut has_strict_types = false;

    loop {
        let (name, name_span) = match tokens.get(*pos) {
            Some((Token::Identifier(name), span)) => (name.clone(), *span),
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
            has_strict_types = true;
            if !matches!(integer_value, Some(0 | 1)) {
                return Err(CompileError::new(
                    name_span,
                    "strict_types declaration must have 0 or 1 as its value",
                ));
            }
        }

        if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Comma)) {
            break;
        }
        *pos += 1;
    }

    Ok(has_strict_types)
}

/// Consumes a PHP declare literal and returns its integer value when it is an integer.
fn parse_literal_value(
    tokens: &[(Token, Span)],
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
    tokens: &[(Token, Span)],
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
