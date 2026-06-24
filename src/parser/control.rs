//! Purpose:
//! Parses PHP control-flow statements and inline loop/header expressions.
//! Covers if/ifdef, loops, foreach, try/catch/finally, switch, and control headers.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()`.
//!
//! Key details:
//! - Control parsers must preserve PHP statement nesting and spans for later flow and diagnostic passes.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{BinOp, CatchClause, Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
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

/// Parses a foreach loop: `foreach ($array as $value)` or `foreach ($array as $key => $value)`.
/// Supports by-reference values via `&` prefix and by-reference loop variables.
///
/// Also supports PHP 7.1+ array-destructuring value patterns: `foreach ($arr as [$a, $b])`
/// and `foreach ($arr as $k => ['key' => $v])`. The bracket pattern is parsed and lowered
/// (via the standalone list-destructuring lowering) against a synthetic per-iteration
/// element variable, and the resulting destructure statement is prepended to the body so
/// the rest of the `Foreach` node — and every pass that reads its `value_var` — is unchanged.
pub fn parse_foreach(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'foreach'")?;
    let array = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::As, "Expected 'as' in foreach")?;

    // Destructure value pattern: `foreach ($arr as [pattern])`.
    if matches!(
        tokens.get(*pos).map(|(token, _)| token),
        Some(Token::LBracket)
    ) {
        return finish_foreach_destructure(tokens, pos, span, array, None);
    }

    let first_by_ref = if matches!(
        tokens.get(*pos).map(|(token, _)| token),
        Some(Token::Ampersand)
    ) {
        *pos += 1;
        true
    } else {
        false
    };

    let first_var = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable after 'as'")),
    };
    *pos += 1;

    // Check for => (foreach $arr as $key => $value)
    let (key_var, value_var, value_by_ref) =
        if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
        if first_by_ref {
            return Err(CompileError::new(
                span,
                "Key element cannot be a reference in foreach",
            ));
        }
        *pos += 1;
        // Destructure value pattern: `foreach ($arr as $k => [pattern])`.
        if matches!(
            tokens.get(*pos).map(|(token, _)| token),
            Some(Token::LBracket)
        ) {
            return finish_foreach_destructure(tokens, pos, span, array, Some(first_var));
        }
        let value_by_ref = if matches!(
            tokens.get(*pos).map(|(token, _)| token),
            Some(Token::Ampersand)
        ) {
            *pos += 1;
            true
        } else {
            false
        };
        let val_var = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => n.clone(),
            _ => return Err(CompileError::new(span, "Expected variable after '=>'")),
        };
        *pos += 1;
        (Some(first_var), val_var, value_by_ref)
    } else {
        (None, first_var, first_by_ref)
    };

    expect_token(tokens, pos, &Token::RParen, "Expected ')' after foreach")?;
    let body = parse_body(tokens, pos)?;

    Ok(Stmt::new(
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        },
        span,
    ))
}

/// Builds a `Foreach` whose value is destructured by a bracket pattern.
///
/// `key_var` is `Some(name)` for the `$k => [pattern]` form, `None` for the `as [pattern]`
/// form. The bracket pattern at `*pos` is parsed and lowered against a fresh synthetic
/// element variable (`__elephc_foreach_destructure_{line}_{col}`, unique per foreach by
/// its starting span) and the resulting destructure statement is prepended to the parsed
/// body. The `Foreach` node itself uses the synthetic variable as `value_var`, so every
/// downstream pass that reads `value_var` continues to work unchanged.
fn finish_foreach_destructure(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    array: Expr,
    key_var: Option<String>,
) -> Result<Stmt, CompileError> {
    let temp = format!("__elephc_foreach_destructure_{}_{}", span.line, span.col);
    let destructure_stmt = crate::parser::stmt::parse_and_lower_foreach_destructure(
        tokens,
        pos,
        span,
        Expr::new(ExprKind::Variable(temp.clone()), span),
    )?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after foreach")?;
    let mut body = parse_body(tokens, pos)?;
    body.insert(0, destructure_stmt);
    Ok(Stmt::new(
        StmtKind::Foreach {
            array,
            key_var,
            value_var: temp,
            value_by_ref: false,
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

    let init = parse_for_clause_list(tokens, pos, &Token::Semicolon)?;
    expect_semicolon(tokens, pos)?;

    let condition = if *pos < tokens.len() && tokens[*pos].0 != Token::Semicolon {
        Some(parse_expr(tokens, pos)?)
    } else {
        None
    };
    expect_semicolon(tokens, pos)?;

    let update = parse_for_clause_list(tokens, pos, &Token::RParen)?;
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
/// Parses a `for` init or update clause, which may be a comma-separated list of inline statements.
///
/// Stops at `terminator` (a `;` for the init clause, a `)` for the update clause). An empty clause
/// yields `None`; a single statement is returned directly; several comma-separated statements are
/// wrapped in a `Synthetic` block so the `for` lowering runs them in order (the init list once, the
/// update list after each iteration), matching PHP's `for ($i = 0, $j = 10; ...; $i++, $j--)`.
fn parse_for_clause_list(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    terminator: &Token,
) -> Result<Option<Box<Stmt>>, CompileError> {
    if *pos >= tokens.len() || tokens[*pos].0 == *terminator {
        return Ok(None);
    }
    let list_span = tokens[*pos].1;
    let mut stmts = Vec::new();
    loop {
        let stmt_span = tokens[*pos].1;
        stmts.push(parse_assign_inline(tokens, pos, stmt_span)?);
        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1; // consume ','
            continue;
        }
        break;
    }
    if stmts.len() == 1 {
        Ok(Some(Box::new(stmts.pop().expect("one statement present"))))
    } else {
        Ok(Some(Box::new(Stmt::new(StmtKind::Synthetic(stmts), list_span))))
    }
}

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
        Token::StarStarAssign => Some(BinOp::Pow),
        Token::SlashAssign => Some(BinOp::Div),
        Token::PercentAssign => Some(BinOp::Mod),
        Token::DotAssign => Some(BinOp::Concat),
        Token::AmpAssign => Some(BinOp::BitAnd),
        Token::PipeAssign => Some(BinOp::BitOr),
        Token::CaretAssign => Some(BinOp::BitXor),
        Token::LessLessAssign => Some(BinOp::ShiftLeft),
        Token::GreaterGreaterAssign => Some(BinOp::ShiftRight),
        Token::Assign => None,
        Token::QuestionQuestionAssign => {
            *pos += 1;
            let rhs = parse_assignment_value_expr(tokens, pos)?;
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

    let rhs = parse_assignment_value_expr(tokens, pos)?;
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
