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
use crate::parser::expr::parse_expr;
use crate::parser::stmt::{
    expect_semicolon, expect_token, name_starts_at, parse_block, parse_body,
    parse_destructuring_pattern_unpack, parse_name, parse_stmt, starts_destructuring_pattern,
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

    let init = parse_for_clause(tokens, pos, &Token::Semicolon, "Expected ';' after for clauses", span)?;
    expect_semicolon(tokens, pos)?;

    let condition = if *pos < tokens.len() && tokens[*pos].0 != Token::Semicolon {
        Some(parse_expr(tokens, pos)?)
    } else {
        None
    };
    // PHP allows a comma list in the CONDITION too, evaluating every expression and taking
    // the last one's value as the loop test. elephc's AST has no sequence expression, and
    // the condition re-runs every iteration so the leading expressions cannot be hoisted
    // into the init clause — supporting it needs a new expression node rather than a
    // re-spelling. Say that, instead of letting the comma fall through to a bare
    // "Expected ';'" that names neither the construct nor the limitation.
    if matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Comma)) {
        return Err(CompileError::new(
            tokens[*pos].1.span,
            "A comma-separated list is not supported in a for CONDITION (the init and \
             update clauses do support one); rewrite the leading expressions into the loop \
             body or the update clause",
        ));
    }
    expect_semicolon(tokens, pos)?;

    let update = parse_for_clause(tokens, pos, &Token::RParen, "Expected ')' after for clauses", span)?;
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

/// Parses one `for` clause — the init or the update — up to but not including `terminator`.
///
/// PHP's grammar for both is a COMMA-SEPARATED LIST OF EXPRESSIONS, and assignment is an
/// expression there, so `for ($i = 0, $j = 10; …; $i++, $a[] = $i)` is ordinary PHP. This
/// used to be a bespoke mini-grammar (`parse_assign_inline`) that accepted only `++$v`,
/// `--$v`, `$v++`, `$v--` and `$v = / op= / ??= expr` — so an array push, an indexed
/// assignment, a property assignment and any comma at all were all rejected, which is
/// issue #476 (the issue reports the push and the comma; the other two are the same gap).
///
/// Rather than growing that grammar a form at a time, each comma-separated piece is handed
/// to the REAL statement parser. The clause is sliced to its terminator first — the
/// statement parsers expect a trailing `;`, so one is appended to the slice — which is also
/// what keeps `find_top_level_assignment` inside the clause instead of scanning on into the
/// loop body looking for an `=`.
///
/// Several pieces become a `StmtKind::Synthetic` list, the existing wrapper for "these
/// statements run as one". It cannot be confused with the nested-append fusion group that
/// also matches on `Synthetic`: that one's second lock is the reserved
/// `NESTED_APPEND_TEMP_PREFIX` temporary, which is not a legal PHP identifier and so can
/// never appear in a clause written by hand.
fn parse_for_clause(
    tokens: &[SpannedToken],
    pos: &mut usize,
    terminator: &Token,
    unterminated: &str,
    fallback_span: Span,
) -> Result<Option<Box<Stmt>>, CompileError> {
    let start = *pos;
    // An unterminated clause reports at its first token; a clause that starts past the end
    // of the stream has none, so the caller's `for` span is the only honest anchor left.
    let anchor = tokens
        .get(start)
        .map(|(_, metadata)| metadata.span)
        .unwrap_or(fallback_span);
    let end = for_clause_end(tokens, start, terminator)
        .ok_or_else(|| CompileError::new(anchor, unterminated))?;
    *pos = end;
    if start == end {
        return Ok(None);
    }

    let clause_span = tokens[start].1.span;
    let mut stmts = Vec::new();
    for piece in split_top_level_commas(&tokens[start..end]) {
        let Some((_, first)) = piece.first() else {
            return Err(CompileError::new(
                clause_span,
                "Expected an expression between ',' in the for clause",
            ));
        };
        let piece_span = first.span;
        // The statement parsers consume a trailing `;`; the clause slice has none, so one
        // is appended rather than threading a terminator mode through every one of them.
        let mut buffer: Vec<SpannedToken> = piece.to_vec();
        let last_span = piece[piece.len() - 1].1.span;
        buffer.push(crate::lexer::token::spanned(Token::Semicolon, last_span));

        let mut piece_pos = 0usize;
        let stmt = parse_stmt(&buffer, &mut piece_pos)?;
        // Delegating to the statement parser buys every assignment form for free, but PHP's
        // clause grammar is an EXPRESSION list: `for (echo "x"; …)` and
        // `for (function f() {}; …)` are both parse errors there, and a deny-list of
        // declarations alone let the `echo` through.
        //
        // So this is an allow-list of the statement kinds an expression can produce — every
        // assignment form, `ExprStmt`, the post-increment shape, `Throw`, and the
        // `Synthetic` a list destructuring expands to. `echo`, `return`, `global`, `static`
        // and `const` are statements in PHP's grammar, not expressions, and are rejected
        // here even though the statement parser accepts them.
        //
        // `throw` is on the list because it IS an expression in PHP 8, and PHP accepts it in
        // a clause: `for (throw new LogicException("x"); false; )` throws. elephc's parser
        // produces `StmtKind::Throw` for it rather than an `ExprStmt`, so it has to be named
        // here.
        //
        // Checked BEFORE the trailing-token test below: a declaration takes no trailing `;`
        // and would otherwise leave the synthetic one unconsumed, reporting the generic
        // "trailing tokens" instead of naming what is actually wrong.
        if clause_runs_an_include(&stmt) {
            return Err(CompileError::new(
                piece_span,
                "include/require is not supported in a for clause; move it above the loop",
            ));
        }
        if !matches!(
            stmt.kind,
            StmtKind::ExprStmt(_)
                | StmtKind::Throw(_)
                | StmtKind::Assign { .. }
                | StmtKind::TypedAssign { .. }
                | StmtKind::RefAssign { .. }
                | StmtKind::ArrayAssign { .. }
                | StmtKind::NestedArrayAssign { .. }
                | StmtKind::ArrayPush { .. }
                | StmtKind::PropertyAssign { .. }
                | StmtKind::PropertyArrayAssign { .. }
                | StmtKind::PropertyArrayPush { .. }
                | StmtKind::StaticPropertyAssign { .. }
                | StmtKind::StaticPropertyArrayAssign { .. }
                | StmtKind::StaticPropertyArrayPush { .. }
                | StmtKind::ListUnpack { .. }
                | StmtKind::Synthetic(_)
        ) {
            return Err(CompileError::new(
                piece_span,
                "Only expressions are allowed in a for clause",
            ));
        }
        if piece_pos < buffer.len() {
            return Err(CompileError::new(
                piece_span,
                "Unexpected trailing tokens in the for clause",
            ));
        }
        stmts.push(stmt);
    }

    Ok(match stmts.len() {
        0 => None,
        1 => Some(Box::new(stmts.pop().expect("one statement"))),
        _ => Some(Box::new(Stmt::new(StmtKind::Synthetic(stmts), clause_span))),
    })
}

/// Returns whether a `for` clause piece runs an `include`/`require` AS THE CLAUSE.
///
/// `include`/`require` ARE expressions in PHP, and PHP runs them in a clause. elephc cannot
/// yet: `resolver::engine` expands a value-include by rewriting the surrounding STATEMENT
/// LIST, which a clause is not, and its `StmtKind::For` arm resolves only the loop body. An
/// include left in a clause therefore survives into the checker as a transient node every
/// consumer treats as `unreachable!()` — `for ($v = include "f.php"; …)` reached that panic
/// once the clause started going through the real statement parser.
///
/// Only two shapes can reach it, which is why this is a match and not a tree walk.
/// `ExprKind::IncludeValue` has exactly TWO construction sites, both calling
/// `parser::stmt::simple::try_parse_value_include`: `parse_return`, whose `Return` is not on
/// the clause allow-list, and `parse_simple_assign`, which builds a plain `StmtKind::Assign`
/// and only for `AssignmentOperator::Assign`. That branch runs BEFORE
/// `parse_assignment_value_expr`, so no typed, indexed, property or static-property
/// assignment can carry one — `include` reaches those through the ordinary expression
/// parser, which rejects it. `test_error_include_is_only_an_expression_in_an_assignment_rhs`
/// pins that invariant, and fails here if it ever changes.
///
/// A closure body is deliberately NOT inspected. Its include is DEFERRED: it runs when the
/// closure is called, not while the clause is evaluated, so
/// `for ($f = function () { include "f.php"; }; …)` is left alone. An earlier version reused
/// the resolver's `has_includes`, which descends into closure bodies, and rejected it.
fn clause_runs_an_include(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Include { .. } => true,
        StmtKind::Assign { value, .. } => matches!(value.kind, ExprKind::IncludeValue { .. }),
        StmtKind::Synthetic(pieces) => pieces.iter().any(clause_runs_an_include),
        _ => false,
    }
}

/// Returns the index of `terminator` that closes this clause, ignoring ones nested inside
/// parentheses, brackets or braces.
///
/// Depth tracking is what lets `for ($i = f($a, $b); …)` keep its argument comma and
/// `for (…; …; $m[")"] = 1)` keep its bracketed paren.
fn for_clause_end(tokens: &[SpannedToken], start: usize, terminator: &Token) -> Option<usize> {
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    for (offset, (token, _)) in tokens[start..].iter().enumerate() {
        if paren == 0 && bracket == 0 && brace == 0 && token == terminator {
            return Some(start + offset);
        }
        match token {
            Token::LParen => paren += 1,
            Token::RParen => {
                if paren == 0 {
                    return None;
                }
                paren -= 1;
            }
            Token::LBracket => bracket += 1,
            Token::RBracket => bracket = bracket.saturating_sub(1),
            Token::LBrace => brace += 1,
            Token::RBrace => brace = brace.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// Splits a clause slice on its top-level commas, skipping the ones nested inside a call's
/// argument list, an array literal or a block.
fn split_top_level_commas(clause: &[SpannedToken]) -> Vec<&[SpannedToken]> {
    let mut pieces = Vec::new();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut piece_start = 0usize;
    for (index, (token, _)) in clause.iter().enumerate() {
        match token {
            Token::LParen => paren += 1,
            Token::RParen => paren = paren.saturating_sub(1),
            Token::LBracket => bracket += 1,
            Token::RBracket => bracket = bracket.saturating_sub(1),
            Token::LBrace => brace += 1,
            Token::RBrace => brace = brace.saturating_sub(1),
            Token::Comma if paren == 0 && bracket == 0 && brace == 0 => {
                pieces.push(&clause[piece_start..index]);
                piece_start = index + 1;
            }
            _ => {}
        }
    }
    pieces.push(&clause[piece_start..]);
    pieces
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
            let default_span = tokens[*pos].1.span;
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
            // The AST keeps `default` apart from the cases, and its source position is later
            // recovered from the span of its first statement. A `default:` with no statements
            // written BEFORE another case (`default: case 2: ...`) falls through into that case,
            // so it needs a position too: an empty synthetic no-op carrying the label's span
            // keeps the body non-empty and orderable without adding any behavior.
            if body.is_empty() && *pos < tokens.len() && tokens[*pos].0 == Token::Case {
                body.push(Stmt::new(StmtKind::Synthetic(Vec::new()), default_span));
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
