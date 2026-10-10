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
//!   PHP places the directive at the head of the file — after only empty statements and earlier
//!   `declare` calls — so "afterwards" is exactly "the rest of this file"; the type checker reads
//!   the stamp back per statement to pick between PHP's strict and coercive parameter binding.
//! - Every other directive (`ticks`, `encoding`) is compile-time syntax only.
//! - Bodies lower through `Synthetic` so they execute in the enclosing scope.

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

    // PHP accepts `strict_types` as the file's very first statement, or after only empty
    // statements and earlier `declare` calls (either form): `<?php ; declare(strict_types=1);`
    // and `<?php declare(ticks=1); declare(strict_types=1);` both print `1`. A real statement
    // before it — inline HTML included, and anything inside a body — is not.
    if strict_types.is_some() && !only_declares_and_empty_statements_before(tokens, declare_pos) {
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

/// Returns whether only empty statements and complete earlier `declare` statements sit between
/// the open tag and `declare_pos`, PHP's condition for placing `strict_types`.
///
/// Scanning tokens (rather than the top-level statement list) also rejects a `declare` nested
/// inside a function, class, or `declare` block: the enclosing body's `{`/keyword is not an empty
/// statement or a preceding `declare`.
fn only_declares_and_empty_statements_before(tokens: &[SpannedToken], declare_pos: usize) -> bool {
    let mut pos = 1;
    while pos < declare_pos {
        match &tokens[pos].0 {
            Token::Semicolon => pos += 1,
            Token::Declare => match skip_declare_statement(tokens, pos) {
                Some(next) if next <= declare_pos => pos = next,
                _ => return false,
            },
            _ => return false,
        }
    }
    pos == declare_pos
}

/// Returns the token index just past a complete `declare (...)` statement, or `None` when the
/// tokens at `start` do not spell one.
///
/// Handles the `;`, `{ … }`, `: … enddeclare ;` and bare single-statement (`declare(ticks=1)
/// echo 1;`) forms, so a `strict_types` that follows any of them is accepted as PHP accepts it.
fn skip_declare_statement(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let after_parens = skip_declare_parens(tokens, start)?;
    match &tokens.get(after_parens)?.0 {
        Token::Semicolon => Some(after_parens + 1),
        Token::LBrace => skip_braced_block(tokens, after_parens),
        Token::Colon => skip_alternative_block(tokens, after_parens),
        // The bare single-statement form (`declare(ticks=1) echo 1;`).
        _ => skip_single_statement(tokens, after_parens),
    }
}

/// Returns the token index just past a `declare (...)`'s matching `)`, or `None`.
fn skip_declare_parens(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let mut pos = start + 1;
    if !matches!(tokens.get(pos)?.0, Token::LParen) {
        return None;
    }
    let mut depth = 0usize;
    while let Some((token, _)) = tokens.get(pos) {
        match token {
            Token::LParen => depth += 1,
            Token::RParen => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos + 1);
                }
            }
            _ => {}
        }
        pos += 1;
    }
    None
}

/// Returns the token index just past a `{ … }` declare body.
fn skip_braced_block(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let mut pos = start;
    let mut braces = 0usize;
    while let Some((token, _)) = tokens.get(pos) {
        match token {
            Token::LBrace => braces += 1,
            Token::RBrace => {
                braces -= 1;
                if braces == 0 {
                    return Some(pos + 1);
                }
            }
            _ => {}
        }
        pos += 1;
    }
    None
}

/// Returns the token index just past a `: … enddeclare;` declare body, matching NESTED colon-form
/// declares: `declare(ticks=1): declare(ticks=2): enddeclare; enddeclare;` closes at the SECOND
/// `enddeclare`, not the first.
fn skip_alternative_block(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let mut pos = start + 1;
    let mut depth = 1usize;
    while let Some((token, _)) = tokens.get(pos) {
        match token {
            Token::Declare => {
                if let Some(after_parens) = skip_declare_parens(tokens, pos) {
                    if matches!(tokens.get(after_parens).map(|(token, _)| token), Some(Token::Colon))
                    {
                        depth += 1;
                        pos = after_parens + 1;
                        continue;
                    }
                    pos = after_parens;
                    continue;
                }
                pos += 1;
            }
            Token::EndDeclare => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos + 2);
                }
                pos += 1;
            }
            _ => pos += 1,
        }
    }
    None
}

/// Returns the token index just past one bare single-statement declare body.
///
/// Scans to the statement's terminating `;` at delimiter depth 0, or past a trailing `}` for a
/// block statement (an optional `;` after it is consumed too). This is the `declare(ticks=1)
/// echo 1;` form PHP accepts before a later `strict_types`.
fn skip_single_statement(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let mut pos = start;
    let mut depth = 0usize;
    while let Some((token, _)) = tokens.get(pos) {
        match token {
            Token::LParen | Token::LBracket | Token::LBrace => depth += 1,
            Token::RParen | Token::RBracket => depth = depth.checked_sub(1)?,
            Token::RBrace => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let next = pos + 1;
                    return Some(
                        if matches!(tokens.get(next).map(|(token, _)| token), Some(Token::Semicolon))
                        {
                            next + 1
                        } else {
                            next
                        },
                    );
                }
            }
            Token::Semicolon if depth == 0 => return Some(pos + 1),
            Token::Eof => return None,
            _ => {}
        }
        pos += 1;
    }
    None
}

/// Seeds the file's `strict_types` state before any statement is parsed.
///
/// PHP applies `declare(strict_types=1)` to the WHOLE file, including statements that PRECEDE the
/// directive (a leading `declare(ticks=...)` body: `declare(ticks=1) { echo f(true); }
/// declare(strict_types=1);` still throws on `f(true)`). The parser stamps each statement as it is
/// created, so a directive parsed late would leave those earlier statements coercive. Scanning the
/// leading tokens for the directive and setting the flag up front stamps every statement of the
/// file; the later `parse_declare` re-applies the same value idempotently. The scan stops at the
/// first real statement, so a directive that is not legally placed seeds nothing the placement
/// check would not reject anyway.
pub(crate) fn preseed_strict_types(tokens: &[SpannedToken]) {
    let mut pos = 1; // past the open tag
    while pos < tokens.len() {
        match &tokens[pos].0 {
            Token::Semicolon => pos += 1,
            Token::Declare => {
                if let Some(enabled) = peek_strict_types(tokens, pos) {
                    crate::source::declare_strict_types(enabled);
                    return;
                }
                match skip_declare_statement(tokens, pos) {
                    Some(next) => pos = next,
                    None => return,
                }
            }
            _ => return,
        }
    }
}

/// Returns the `strict_types` value of a `declare(...)` at `declare_pos`, if it declares one.
fn peek_strict_types(tokens: &[SpannedToken], declare_pos: usize) -> Option<bool> {
    let mut pos = declare_pos + 1;
    if !matches!(tokens.get(pos)?.0, Token::LParen) {
        return None;
    }
    pos += 1;
    parse_directives(tokens, &mut pos, Span::dummy()).ok().flatten()
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
