//! Purpose:
//! Parses PHP control-flow statements and inline loop/header expressions.
//! Covers if/ifdef, loops, foreach, try/catch/finally, switch, and control headers.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()`.
//!
//! Key details:
//! - Control parsers must preserve PHP statement nesting and spans for later flow and diagnostic passes.
//! - Brace and alternative (`:` … `endX;`) bodies produce identical `StmtKind` shapes, so the
//!   distinction never escapes this module.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::alt_syntax::{
    close_alternative_block, parse_alternative_stmts, parse_control_body,
    reject_mixed_branch_body, starts_alternative_body, IF_SEGMENT_STOPS,
};
use crate::parser::ast::{CatchClause, Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::{parse_expr, starts_void_statement_cast};
use crate::parser::stmt::{
    expect_semicolon, expect_token, name_starts_at, parse_block, parse_body,
    parse_destructuring_pattern_unpack, parse_name, parse_void_expr_inline,
    starts_destructuring_pattern,
};
use crate::span::Span;

/// Parse: if (expr) { stmts } (elseif (expr) { stmts })* (else { stmts })?
///
/// Also accepts PHP's alternative form `if (expr): … elseif (expr): … else: … endif;`,
/// which is delegated to `parse_alternative_if` and yields the same `StmtKind::If`.
pub fn parse_if(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'if'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after if condition")?;

    if starts_alternative_body(tokens, *pos) {
        return parse_alternative_if(tokens, pos, span, condition);
    }

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
            reject_mixed_branch_body(tokens, *pos, "elseif")?;
            let body = parse_body(tokens, pos)?;
            elseif_clauses.push((cond, body));
        } else if tokens[*pos].0 == Token::Else {
            *pos += 1;
            reject_mixed_branch_body(tokens, *pos, "else")?;
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

/// Parse the alternative `if` form: `: stmts (elseif (expr): stmts)* (else: stmts)? endif;`.
///
/// `pos` points at the `:` that opened the `then` segment and `condition` is the already-parsed
/// `if` condition. PHP requires every branch of an alternative `if` to use the colon form and the
/// whole chain to be closed by `endif;`, so a brace body or a bare `else if` is rejected here.
fn parse_alternative_if(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    condition: Expr,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let then_body = parse_alternative_stmts(tokens, pos, IF_SEGMENT_STOPS)?;

    let mut elseif_clauses = Vec::new();
    let mut else_body = None;

    loop {
        match tokens.get(*pos).map(|(token, _)| token) {
            Some(Token::ElseIf) => {
                *pos += 1;
                expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'elseif'")?;
                let cond = parse_expr(tokens, pos)?;
                expect_token(tokens, pos, &Token::RParen, "Expected ')' after elseif condition")?;
                expect_token(
                    tokens,
                    pos,
                    &Token::Colon,
                    "Expected ':' after elseif condition in an alternative-syntax if block",
                )?;
                let body = parse_alternative_stmts(tokens, pos, IF_SEGMENT_STOPS)?;
                elseif_clauses.push((cond, body));
            }
            Some(Token::Else) => {
                *pos += 1;
                expect_token(
                    tokens,
                    pos,
                    &Token::Colon,
                    "Expected ':' after 'else' in an alternative-syntax if block",
                )?;
                else_body = Some(parse_alternative_stmts(tokens, pos, &[Token::EndIf])?);
                break;
            }
            _ => break,
        }
    }

    close_alternative_block(tokens, pos, &Token::EndIf, "endif")?;

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
    tokens: &[SpannedToken],
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

/// Parse: while (expr) { stmts }, or the alternative form `while (expr): stmts endwhile;`.
pub fn parse_while(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'while'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after while condition")?;
    let body = parse_control_body(tokens, pos, &Token::EndWhile, "endwhile")?;
    Ok(Stmt::new(StmtKind::While { condition, body }, span))
}

/// Parses a foreach loop: `foreach ($array as $value)` or `foreach ($array as $key => $value)`.
/// Supports by-reference values via `&` prefix and by-reference loop variables.
pub fn parse_foreach(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'foreach'")?;
    let array = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::As, "Expected 'as' in foreach")?;

    let first_by_ref = if matches!(
        tokens.get(*pos).map(|(token, _)| token),
        Some(Token::Ampersand)
    ) {
        *pos += 1;
        true
    } else {
        false
    };

    // `foreach ($pairs as [$a, $b])`: the value target is a destructuring pattern, so the
    // loop binds a hidden temporary and the body starts by unpacking it.
    if starts_destructuring_pattern(tokens, *pos) {
        if first_by_ref {
            return Err(CompileError::new(
                span,
                "Cannot take a reference to a destructuring pattern in foreach",
            ));
        }
        let (value_var, unpack) = parse_foreach_pattern_target(tokens, pos, span)?;
        expect_token(tokens, pos, &Token::RParen, "Expected ')' after foreach")?;
        let loop_body = parse_control_body(tokens, pos, &Token::EndForeach, "endforeach")?;
        let body = prepend_stmt(unpack, loop_body);
        return Ok(Stmt::new(
            StmtKind::Foreach {
                array,
                key_var: None,
                value_var,
                value_by_ref: false,
                body,
            },
            span,
        ));
    }

    let first_var = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable after 'as'")),
    };
    *pos += 1;

    // Check for => (foreach $arr as $key => $value)
    let (key_var, value_var, value_by_ref, unpack) =
        if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
        if first_by_ref {
            return Err(CompileError::new(
                span,
                "Key element cannot be a reference in foreach",
            ));
        }
        *pos += 1;
        let value_by_ref = if matches!(
            tokens.get(*pos).map(|(token, _)| token),
            Some(Token::Ampersand)
        ) {
            *pos += 1;
            true
        } else {
            false
        };
        // `foreach ($m as $k => [$a, $b])` destructures the value the same way.
        if starts_destructuring_pattern(tokens, *pos) {
            if value_by_ref {
                return Err(CompileError::new(
                    span,
                    "Cannot take a reference to a destructuring pattern in foreach",
                ));
            }
            let (val_var, unpack) = parse_foreach_pattern_target(tokens, pos, span)?;
            (Some(first_var), val_var, false, Some(unpack))
        } else {
            let val_var = match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Variable(n)) => n.clone(),
                _ => return Err(CompileError::new(span, "Expected variable after '=>'")),
            };
            *pos += 1;
            (Some(first_var), val_var, value_by_ref, None)
        }
    } else {
        (None, first_var, first_by_ref, None)
    };

    expect_token(tokens, pos, &Token::RParen, "Expected ')' after foreach")?;
    let body = parse_control_body(tokens, pos, &Token::EndForeach, "endforeach")?;
    let body = match unpack {
        Some(unpack) => prepend_stmt(unpack, body),
        None => body,
    };

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

/// Parses a `foreach` value destructuring pattern into a hidden loop variable plus the
/// statement that unpacks it.
///
/// The loop still binds one value per iteration, so the pattern becomes
/// `foreach (… as $tmp) { [pattern] = $tmp; … }`. The temporary is named from the pattern's
/// source position so nested loops in one function never collide.
fn parse_foreach_pattern_target(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<(String, Stmt), CompileError> {
    let pattern_span = tokens
        .get(*pos)
        .map(|(_, metadata)| metadata.span)
        .unwrap_or(span);
    let value_var = format!(
        "__elephc_foreach_{}_{}",
        pattern_span.line, pattern_span.col
    );
    let source = Expr::new(ExprKind::Variable(value_var.clone()), pattern_span);
    let unpack = parse_destructuring_pattern_unpack(tokens, pos, pattern_span, source)?;
    Ok((value_var, unpack))
}

/// Returns `body` with `first` inserted as its first statement.
fn prepend_stmt(first: Stmt, body: Vec<Stmt>) -> Vec<Stmt> {
    let mut stmts = Vec::with_capacity(body.len() + 1);
    stmts.push(first);
    stmts.extend(body);
    stmts
}

/// Parse: do { stmts } while (expr);
pub fn parse_do_while(
    tokens: &[SpannedToken],
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

/// Parse: for (init; condition; update) { stmts }, or `for (…): stmts endfor;`.
pub fn parse_for(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'for'")?;

    let init = if *pos < tokens.len() && tokens[*pos].0 != Token::Semicolon {
        let init_span = tokens[*pos].1.span;
        let s = parse_for_clause_stmt(tokens, pos, init_span)?;
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
        let update_span = tokens[*pos].1.span;
        let s = parse_for_clause_stmt(tokens, pos, update_span)?;
        Some(Box::new(s))
    } else {
        None
    };
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after for clauses")?;

    let body = parse_control_body(tokens, pos, &Token::EndFor, "endfor")?;

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
    tokens: &[SpannedToken],
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

/// Parses any PHP expression used in a `for` initializer or update clause without consuming
/// the clause delimiter. A plain local assignment is canonicalized to `StmtKind::Assign` so
/// statement-level analyses retain their existing assignment semantics; other expressions stay
/// expression statements.
pub fn parse_assign_inline(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let expr = parse_expr(tokens, pos)?;
    if let ExprKind::Assignment {
        target,
        value,
        result_target,
        prelude,
        conditional_value_temp,
    } = &expr.kind
    {
        if prelude.is_empty() && result_target.is_none() && conditional_value_temp.is_none() {
            if let ExprKind::Variable(name) = &target.kind {
                return Ok(Stmt::new(
                    StmtKind::Assign {
                        name: name.clone(),
                        value: value.as_ref().clone(),
                    },
                    span,
                ));
            }
        }
    }
    Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
}

/// Parses one side-effecting `for` initializer or update clause without its delimiter.
fn parse_for_clause_stmt(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    if starts_void_statement_cast(tokens, *pos) {
        parse_void_expr_inline(tokens, pos, span)
    } else {
        parse_assign_inline(tokens, pos, span)
    }
}

/// Parse: switch (expr) { case expr: stmts... case expr: stmts... default: stmts... }
///
/// Also accepts PHP's alternative form `switch (expr): case …: … endswitch;`. Both forms
/// produce the same `StmtKind::Switch`; only the case-list terminator differs.
pub fn parse_switch(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'switch'
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'switch'")?;
    let subject = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after switch expression")?;

    let alternative = starts_alternative_body(tokens, *pos);
    if alternative {
        *pos += 1;
    } else {
        expect_token(tokens, pos, &Token::LBrace, "Expected '{' after switch")?;
    }
    // The case list ends at `}` in the brace form and at `endswitch` in the alternative form.
    let close = if alternative {
        Token::EndSwitch
    } else {
        Token::RBrace
    };

    let mut cases: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
    let mut default: Option<Vec<Stmt>> = None;

    while *pos < tokens.len() && tokens[*pos].0 != close && tokens[*pos].0 != Token::Eof {
        if tokens[*pos].0 == Token::Case {
            // Parse one or more case values
            let mut values = Vec::new();
            while *pos < tokens.len() && tokens[*pos].0 == Token::Case {
                *pos += 1;
                values.push(parse_expr(tokens, pos)?);
                expect_case_separator(tokens, pos, "Expected ':' after case value")?;
            }
            // Parse case body (statements until the next case/default or the case-list end)
            let mut body = Vec::new();
            while *pos < tokens.len()
                && tokens[*pos].0 != Token::Case
                && tokens[*pos].0 != Token::Default
                && tokens[*pos].0 != close
                && tokens[*pos].0 != Token::Eof
            {
                body.push(crate::parser::stmt::parse_stmt(tokens, pos)?);
            }
            cases.push((values, body));
        } else if tokens[*pos].0 == Token::Default {
            *pos += 1;
            expect_case_separator(tokens, pos, "Expected ':' after 'default'")?;
            let mut body = Vec::new();
            while *pos < tokens.len()
                && tokens[*pos].0 != Token::Case
                && tokens[*pos].0 != close
                && tokens[*pos].0 != Token::Eof
            {
                body.push(crate::parser::stmt::parse_stmt(tokens, pos)?);
            }
            default = Some(body);
        } else {
            return Err(CompileError::new(
                tokens[*pos].1.span,
                "Expected 'case' or 'default' inside switch",
            ));
        }
    }

    if alternative {
        close_alternative_block(tokens, pos, &Token::EndSwitch, "endswitch")?;
    } else {
        expect_token(tokens, pos, &Token::RBrace, "Expected '}' to close switch")?;
    }

    Ok(Stmt::new(
        StmtKind::Switch {
            subject,
            cases,
            default,
        },
        span,
    ))
}

/// Consumes the separator that terminates a `case`/`default` label.
///
/// PHP accepts either `:` or `;` there, so both are allowed with the same meaning.
fn expect_case_separator(
    tokens: &[SpannedToken],
    pos: &mut usize,
    message: &str,
) -> Result<(), CompileError> {
    if matches!(
        tokens.get(*pos).map(|(token, _)| token),
        Some(Token::Semicolon)
    ) {
        *pos += 1;
        return Ok(());
    }
    expect_token(tokens, pos, &Token::Colon, message)
}
