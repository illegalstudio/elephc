use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{BinOp, CatchClause, Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::parse_expr;
use crate::parser::stmt::{expect_semicolon, expect_token, name_starts_at, parse_block, parse_body, parse_name};
use crate::span::Span;

/// Parse: if (expr) { stmts } (elseif (expr) { stmts })* (else { stmts })?
pub fn parse_if(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'if'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after if condition")?;
    let then_body = parse_body(tokens, pos)?;

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
            let body = parse_body(tokens, pos)?;
            elseif_clauses.push((cond, body));
        } else if tokens[*pos].0 == Token::Else {
            *pos += 1;
            else_body = Some(parse_body(tokens, pos)?);
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

/// Parse: ifdef SYMBOL { stmts } (else { stmts })?
pub fn parse_ifdef(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;

    let symbol = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(name)) => name.clone(),
        _ => return Err(CompileError::new(span, "Expected symbol name after 'ifdef'")),
    };
    *pos += 1;

    let then_body = parse_block(tokens, pos)?;
    let else_body = if *pos < tokens.len() && tokens[*pos].0 == Token::Else {
        *pos += 1;
        Some(parse_block(tokens, pos)?)
    } else {
        None
    };

    Ok(Stmt::new(
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        },
        span,
    ))
}

/// Parse: while (expr) { stmts }
pub fn parse_while(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'while'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after while condition")?;
    let body = parse_body(tokens, pos)?;
    Ok(Stmt::new(StmtKind::While { condition, body }, span))
}

/// Parse: foreach ($array as $value) { stmts }
pub fn parse_foreach(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'foreach'")?;
    let array = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::As, "Expected 'as' in foreach")?;

    let first_var = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable after 'as'")),
    };
    *pos += 1;

    // Check for => (foreach $arr as $key => $value)
    let (key_var, value_var) = if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
        *pos += 1;
        let val_var = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => n.clone(),
            _ => return Err(CompileError::new(span, "Expected variable after '=>'")),
        };
        *pos += 1;
        (Some(first_var), val_var)
    } else {
        (None, first_var)
    };

    expect_token(tokens, pos, &Token::RParen, "Expected ')' after foreach")?;
    let body = parse_body(tokens, pos)?;

    Ok(Stmt::new(
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        },
        span,
    ))
}

/// Parse: do { stmts } while (expr);
pub fn parse_do_while(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let body = parse_block(tokens, pos)?;
    expect_token(tokens, pos, &Token::While, "Expected 'while' after do block")?;
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'while'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after condition")?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::DoWhile { body, condition }, span))
}

/// Parse: for (init; condition; update) { stmts }
pub fn parse_for(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'for'")?;

    let init = if *pos < tokens.len() && tokens[*pos].0 != Token::Semicolon {
        let init_span = tokens[*pos].1;
        let s = parse_assign_inline(tokens, pos, init_span)?;
        Some(Box::new(s))
    } else {
        None
    };
    expect_semicolon(tokens, pos)?;

    let condition = if *pos < tokens.len() && tokens[*pos].0 != Token::Semicolon {
        Some(parse_expr(tokens, pos)?)
    } else {
        None
    };
    expect_semicolon(tokens, pos)?;

    let update = if *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        let update_span = tokens[*pos].1;
        let s = parse_assign_inline(tokens, pos, update_span)?;
        Some(Box::new(s))
    } else {
        None
    };
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after for clauses")?;

    let body = parse_body(tokens, pos)?;

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

/// Parse: try { stmts } (catch (TypeA|TypeB $e) { stmts })+ (finally { stmts })?
///     or: try { stmts } finally { stmts }
pub fn parse_try(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let try_body = parse_body(tokens, pos)?;

    let mut catches = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 == Token::Catch {
        *pos += 1;
        expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'catch'")?;
        let mut exception_types = Vec::new();
        loop {
            if *pos < tokens.len() && tokens[*pos].0 == Token::Self_ {
                exception_types.push(crate::names::Name::unqualified("self"));
                *pos += 1;
            } else if *pos < tokens.len() && tokens[*pos].0 == Token::Parent {
                exception_types.push(crate::names::Name::unqualified("parent"));
                *pos += 1;
            } else if name_starts_at(tokens, *pos) {
                exception_types.push(parse_name(
                    tokens,
                    pos,
                    span,
                    "Expected exception class name in catch clause",
                )?);
            } else {
                return Err(CompileError::new(
                    span,
                    "Expected exception class name in catch clause",
                ));
            }
            if *pos < tokens.len() && tokens[*pos].0 == Token::Pipe {
                *pos += 1;
                continue;
            }
            break;
        }
        let variable = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(name)) => {
                *pos += 1;
                Some(name.clone())
            }
            Some(Token::RParen) => None,
            _ => {
                return Err(CompileError::new(
                    span,
                    "Expected catch variable or ')' after exception type",
                ))
            }
        };
        expect_token(tokens, pos, &Token::RParen, "Expected ')' after catch clause")?;
        let body = parse_body(tokens, pos)?;
        catches.push(CatchClause {
            exception_types,
            variable,
            body,
        });
    }

    let finally_body = if *pos < tokens.len() && tokens[*pos].0 == Token::Finally {
        *pos += 1;
        Some(parse_body(tokens, pos)?)
    } else {
        None
    };

    if catches.is_empty() && finally_body.is_none() {
        return Err(CompileError::new(
            span,
            "Expected at least one catch or a finally block after try",
        ));
    }

    Ok(Stmt::new(
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        },
        span,
    ))
}

/// Parse a simple statement without trailing semicolon (for use inside for-loops).
pub fn parse_assign_inline(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    if *pos < tokens.len() {
        match &tokens[*pos].0 {
            Token::PlusPlus => {
                *pos += 1;
                let name = match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Variable(n)) => n.clone(),
                    _ => return Err(CompileError::new(span, "Expected variable after '++'")),
                };
                *pos += 1;
                let expr = Expr::new(ExprKind::PreIncrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            Token::MinusMinus => {
                *pos += 1;
                let name = match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Variable(n)) => n.clone(),
                    _ => return Err(CompileError::new(span, "Expected variable after '--'")),
                };
                *pos += 1;
                let expr = Expr::new(ExprKind::PreDecrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            _ => {}
        }
    }

    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable in for clause")),
    };
    *pos += 1;

    if *pos < tokens.len() {
        match &tokens[*pos].0 {
            Token::PlusPlus => {
                *pos += 1;
                let expr = Expr::new(ExprKind::PostIncrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            Token::MinusMinus => {
                *pos += 1;
                let expr = Expr::new(ExprKind::PostDecrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            _ => {}
        }
    }

    if *pos >= tokens.len() {
        return Err(CompileError::new(span, "Expected '=' after variable name"));
    }

    let compound_op = match &tokens[*pos].0 {
        Token::PlusAssign => Some(BinOp::Add),
        Token::MinusAssign => Some(BinOp::Sub),
        Token::StarAssign => Some(BinOp::Mul),
        Token::SlashAssign => Some(BinOp::Div),
        Token::PercentAssign => Some(BinOp::Mod),
        Token::DotAssign => Some(BinOp::Concat),
        Token::Assign => None,
        Token::QuestionQuestionAssign => {
            *pos += 1;
            let rhs = parse_expr(tokens, pos)?;
            let value = Expr::new(
                ExprKind::NullCoalesce {
                    value: Box::new(Expr::new(ExprKind::Variable(name.clone()), span)),
                    default: Box::new(rhs),
                },
                span,
            );
            return Ok(Stmt::new(StmtKind::Assign { name, value }, span));
        }
        _ => return Err(CompileError::new(span, "Expected '=' after variable name")),
    };
    *pos += 1;

    let rhs = parse_expr(tokens, pos)?;
    let value = if let Some(op) = compound_op {
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(ExprKind::Variable(name.clone()), span)),
                op,
                right: Box::new(rhs),
            },
            span,
        )
    } else {
        rhs
    };
    Ok(Stmt::new(StmtKind::Assign { name, value }, span))
}

/// Parse: switch (expr) { case expr: stmts... case expr: stmts... default: stmts... }
pub fn parse_switch(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'switch'
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'switch'")?;
    let subject = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after switch expression")?;
    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after switch")?;

    let mut cases: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
    let mut default: Option<Vec<Stmt>> = None;

    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        if tokens[*pos].0 == Token::Case {
            // Parse one or more case values
            let mut values = Vec::new();
            while *pos < tokens.len() && tokens[*pos].0 == Token::Case {
                *pos += 1;
                values.push(parse_expr(tokens, pos)?);
                expect_token(tokens, pos, &Token::Colon, "Expected ':' after case value")?;
            }
            // Parse case body (statements until next case/default/})
            let mut body = Vec::new();
            while *pos < tokens.len()
                && tokens[*pos].0 != Token::Case
                && tokens[*pos].0 != Token::Default
                && tokens[*pos].0 != Token::RBrace
            {
                body.push(crate::parser::stmt::parse_stmt(tokens, pos)?);
            }
            cases.push((values, body));
        } else if tokens[*pos].0 == Token::Default {
            *pos += 1;
            expect_token(tokens, pos, &Token::Colon, "Expected ':' after 'default'")?;
            let mut body = Vec::new();
            while *pos < tokens.len()
                && tokens[*pos].0 != Token::Case
                && tokens[*pos].0 != Token::RBrace
            {
                body.push(crate::parser::stmt::parse_stmt(tokens, pos)?);
            }
            default = Some(body);
        } else {
            return Err(CompileError::new(
                tokens[*pos].1,
                "Expected 'case' or 'default' inside switch",
            ));
        }
    }

    expect_token(tokens, pos, &Token::RBrace, "Expected '}' to close switch")?;

    Ok(Stmt::new(
        StmtKind::Switch {
            subject,
            cases,
            default,
        },
        span,
    ))
}
