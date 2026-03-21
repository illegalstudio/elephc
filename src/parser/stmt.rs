use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Stmt, StmtKind};
use crate::parser::expr::parse_expr;
use crate::span::Span;

pub fn parse_stmt(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Stmt, CompileError> {
    let span = tokens[*pos].1;

    match &tokens[*pos].0 {
        Token::Echo => parse_echo(tokens, pos, span),
        Token::Variable(_) => parse_assign(tokens, pos, span),
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token at statement position: {:?}", other),
        )),
    }
}

fn parse_echo(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'echo'
    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Echo(expr), span))
}

fn parse_assign(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };
    *pos += 1;

    if *pos >= tokens.len() || tokens[*pos].0 != Token::Assign {
        return Err(CompileError::new(span, "Expected '=' after variable name"));
    }
    *pos += 1;

    let value = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::new(StmtKind::Assign { name, value }, span))
}

fn expect_semicolon(tokens: &[(Token, Span)], pos: &mut usize) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        Ok(())
    } else {
        let span = if *pos < tokens.len() {
            tokens[*pos].1
        } else {
            Span::dummy()
        };
        Err(CompileError::new(span, "Expected ';'"))
    }
}
