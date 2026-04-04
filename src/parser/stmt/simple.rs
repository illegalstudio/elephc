use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{ExprKind, Stmt, StmtKind};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::{expect_semicolon, expect_token};

pub(super) fn parse_include(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    once: bool,
    required: bool,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume include/require keyword

    // Support both: include 'file.php'; and include('file.php');
    let has_parens = *pos < tokens.len() && tokens[*pos].0 == Token::LParen;
    if has_parens {
        *pos += 1;
    }

    let path = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::StringLiteral(s)) => s.clone(),
        _ => {
            return Err(CompileError::new(
                span,
                "Expected string path after include/require",
            ))
        }
    };
    *pos += 1;

    if has_parens {
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(span, "Expected ')' after include path"));
        }
        *pos += 1;
    }

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(
        StmtKind::Include {
            path,
            once,
            required,
        },
        span,
    ))
}

pub(super) fn parse_echo(tokens: &[(Token, Span)], pos: &mut usize, span: Span) -> Result<Stmt, CompileError> {
    *pos += 1;
    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Echo(expr), span))
}

pub(super) fn parse_return(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;

    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        return Ok(Stmt::new(StmtKind::Return(None), span));
    }

    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Return(Some(expr)), span))
}

pub(super) fn parse_throw(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Throw(expr), span))
}

/// Handle statements starting with $this: $this->prop = value; or $this->method();
pub(super) fn parse_this_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    // Parse as expression first
    let expr = parse_expr(tokens, pos)?;
    // Check if followed by assignment
    if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
        *pos += 1;
        let value = parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        if let ExprKind::PropertyAccess { object, property } = expr.kind {
            return Ok(Stmt::new(
                StmtKind::PropertyAssign {
                    object,
                    property,
                    value,
                },
                span,
            ));
        }
        return Err(CompileError::new(
            span,
            "Invalid assignment target after $this",
        ));
    }
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
}

pub(super) fn parse_const_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'const'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => n.clone(),
        _ => {
            return Err(CompileError::new(
                span,
                "Expected constant name after 'const'",
            ))
        }
    };
    *pos += 1;

    expect_token(
        tokens,
        pos,
        &Token::Assign,
        "Expected '=' after constant name",
    )?;

    let value = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::new(StmtKind::ConstDecl { name, value }, span))
}
