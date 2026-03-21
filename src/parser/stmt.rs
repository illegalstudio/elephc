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
        Token::If => parse_if(tokens, pos, span),
        Token::While => parse_while(tokens, pos, span),
        Token::For => parse_for(tokens, pos, span),
        Token::Break => {
            *pos += 1;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::Break, span))
        }
        Token::Continue => {
            *pos += 1;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::Continue, span))
        }
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
    *pos += 1;
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

/// Parse: if (expr) { stmts } (elseif (expr) { stmts })* (else { stmts })?
fn parse_if(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'if'

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'if'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after if condition")?;
    let then_body = parse_block(tokens, pos)?;

    let mut elseif_clauses = Vec::new();
    let mut else_body = None;

    loop {
        if *pos >= tokens.len() {
            break;
        }
        if tokens[*pos].0 == Token::ElseIf {
            *pos += 1;
            expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'elseif'")?;
            let cond = parse_expr(tokens, pos)?;
            expect_token(tokens, pos, &Token::RParen, "Expected ')' after elseif condition")?;
            let body = parse_block(tokens, pos)?;
            elseif_clauses.push((cond, body));
        } else if tokens[*pos].0 == Token::Else {
            *pos += 1;
            else_body = Some(parse_block(tokens, pos)?);
            break;
        } else {
            break;
        }
    }

    Ok(Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        },
        span,
    ))
}

/// Parse: while (expr) { stmts }
fn parse_while(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'while'

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'while'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after while condition")?;
    let body = parse_block(tokens, pos)?;

    Ok(Stmt::new(StmtKind::While { condition, body }, span))
}

/// Parse: for (init; condition; update) { stmts }
fn parse_for(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'for'

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'for'")?;

    // Init (optional assignment)
    let init = if *pos < tokens.len() && tokens[*pos].0 != Token::Semicolon {
        let init_span = tokens[*pos].1;
        let s = parse_assign_inline(tokens, pos, init_span)?;
        Some(Box::new(s))
    } else {
        None
    };
    expect_semicolon(tokens, pos)?;

    // Condition (optional expression)
    let condition = if *pos < tokens.len() && tokens[*pos].0 != Token::Semicolon {
        Some(parse_expr(tokens, pos)?)
    } else {
        None
    };
    expect_semicolon(tokens, pos)?;

    // Update (optional assignment)
    let update = if *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        let update_span = tokens[*pos].1;
        let s = parse_assign_inline(tokens, pos, update_span)?;
        Some(Box::new(s))
    } else {
        None
    };
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after for clauses")?;

    let body = parse_block(tokens, pos)?;

    Ok(Stmt::new(
        StmtKind::For {
            init,
            condition,
            update,
            body,
        },
        span,
    ))
}

/// Parse an assignment without trailing semicolon (for use inside for-loops).
fn parse_assign_inline(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable in for clause")),
    };
    *pos += 1;

    if *pos >= tokens.len() || tokens[*pos].0 != Token::Assign {
        return Err(CompileError::new(span, "Expected '=' after variable name"));
    }
    *pos += 1;

    let value = parse_expr(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Assign { name, value }, span))
}

/// Parse a brace-delimited block: { stmt* }
pub fn parse_block(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Vec<Stmt>, CompileError> {
    let span = if *pos < tokens.len() {
        tokens[*pos].1
    } else {
        Span::dummy()
    };
    expect_token(tokens, pos, &Token::LBrace, "Expected '{'")?;

    let mut stmts = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        stmts.push(parse_stmt(tokens, pos)?);
    }

    if *pos >= tokens.len() || tokens[*pos].0 != Token::RBrace {
        return Err(CompileError::new(span, "Expected '}'"));
    }
    *pos += 1;

    Ok(stmts)
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

fn expect_token(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    expected: &Token,
    msg: &str,
) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == *expected {
        *pos += 1;
        Ok(())
    } else {
        let span = if *pos < tokens.len() {
            tokens[*pos].1
        } else {
            Span::dummy()
        };
        Err(CompileError::new(span, msg))
    }
}
