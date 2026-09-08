//! Purpose:
//! Parses statement bodies and shared braced blocks, including parse-error recovery.
//!
//! Called from:
//! - `crate::parser::stmt` and control-flow statement parsers.
//!
//! Key details:
//! - A block accumulates recoverable statement errors before reporting them together.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::Stmt;
use crate::span::Span;

use super::{parse_stmt, recover_to_statement_boundary};

/// Parses a braced block `{ stmts }`, returning statements or accumulated errors.
pub fn parse_block(tokens: &[SpannedToken], pos: &mut usize) -> Result<Vec<Stmt>, CompileError> {
    let span = if *pos < tokens.len() {
        tokens[*pos].1.span
    } else {
        Span::dummy()
    };
    expect_token(tokens, pos, &Token::LBrace, "Expected '{'")?;

    let mut stmts = Vec::new();
    let mut errors = Vec::new();
    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        if tokens[*pos].0 == Token::Semicolon {
            *pos += 1;
            continue;
        }
        if tokens[*pos].0 == Token::LBrace {
            match parse_block(tokens, pos) {
                Ok(mut block_stmts) => stmts.append(&mut block_stmts),
                Err(error) => errors.extend(error.flatten()),
            }
            continue;
        }
        match parse_stmt(tokens, pos) {
            Ok(stmt) => stmts.push(stmt),
            Err(error) => {
                errors.extend(error.flatten());
                recover_to_statement_boundary(tokens, pos);
            }
        }
    }

    if *pos >= tokens.len() || tokens[*pos].0 != Token::RBrace {
        errors.push(CompileError::new(span, "Expected '}'"));
        return Err(CompileError::from_many(errors));
    }
    *pos += 1;

    if errors.is_empty() {
        Ok(stmts)
    } else {
        Err(CompileError::from_many(errors))
    }
}

/// Parses either a braced block or one braceless statement body.
pub fn parse_body(tokens: &[SpannedToken], pos: &mut usize) -> Result<Vec<Stmt>, CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::LBrace {
        parse_block(tokens, pos)
    } else if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        Ok(Vec::new())
    } else {
        let stmt = parse_stmt(tokens, pos)?;
        Ok(vec![stmt])
    }
}

/// Consumes a semicolon token or reports its absence at the current source span.
pub(crate) fn expect_semicolon(
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        Ok(())
    } else {
        let span = if *pos < tokens.len() {
            tokens[*pos].1.span
        } else {
            Span::dummy()
        };
        Err(CompileError::new(span, "Expected ';'"))
    }
}

/// Consumes one exact token or reports the supplied syntax error.
pub(crate) fn expect_token(
    tokens: &[SpannedToken],
    pos: &mut usize,
    expected: &Token,
    msg: &str,
) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == *expected {
        *pos += 1;
        Ok(())
    } else {
        let span = if *pos < tokens.len() {
            tokens[*pos].1.span
        } else {
            Span::dummy()
        };
        Err(CompileError::new(span, msg))
    }
}
