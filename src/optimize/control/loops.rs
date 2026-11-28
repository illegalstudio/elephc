//! Purpose:
//! Canonicalizes loop shells during control-flow normalization: `for` loops without an update
//! clause become `while` loops, `do ... while (true)` becomes `while (true)`, leading
//! `if (...) { break; }` guards fold into the loop test, trailing `continue` statements are
//! dropped, and an unconditional loop that ends in a break guard rotates into `do { ... } while`.
//!
//! Called from:
//! - `crate::optimize::control::prune::statements::prune_stmt()` for `While`, `DoWhile`, and `For`.
//!
//! Key details:
//! - Every rewrite moves nodes instead of cloning them, so no span-keyed checker decision is duplicated.
//! - Guard folding keeps PHP evaluation order: the guard runs exactly where the body used to test it,
//!   after the loop test and only when that test passed, on the first entry and after every `continue`.
//! - Rotation into `do ... while` refuses bodies with a `continue` targeting the loop, because that
//!   `continue` skips the trailing guard today but would reach the rotated test.

use super::*;

/// Canonicalizes a pruned `while` loop shell and returns the replacement statements.
pub(crate) fn normalize_while_stmt(
    condition: Expr,
    body: Vec<Stmt>,
    span: crate::span::Span,
    source_mode: crate::source::SourceMode,
    strict_types: bool,
) -> Vec<Stmt> {
    let body = strip_trailing_terminator(body, TailTerminator::LoopContinue);
    let (condition, body) = fold_leading_break_guards(Some(condition), body);
    let condition = condition.expect("a while loop keeps its condition");

    let body = if is_truthy_literal(&condition) {
        match split_trailing_break_guard(body) {
            Ok((body, guard)) => {
                return vec![Stmt {
                    kind: StmtKind::DoWhile {
                        body,
                        condition: invert_condition(guard),
                    },
                    span,
                    source_mode,
                    strict_types,
                    attributes: Vec::new(),
                }];
            }
            Err(body) => body,
        }
    } else {
        body
    };

    vec![Stmt {
        kind: StmtKind::While { condition, body },
        span,
        source_mode,
        strict_types,
        attributes: Vec::new(),
    }]
}

/// Canonicalizes a pruned `do ... while` loop shell and returns the replacement statements.
///
/// A literal-true condition makes the shell a `while (true)` loop: `continue` reaches the
/// always-true test in both forms, so the two are interchangeable and the `while` form is the
/// one every other loop rewrite understands.
pub(crate) fn normalize_do_while_stmt(
    body: Vec<Stmt>,
    condition: Expr,
    span: crate::span::Span,
    source_mode: crate::source::SourceMode,
    strict_types: bool,
) -> Vec<Stmt> {
    let body = strip_trailing_terminator(body, TailTerminator::LoopContinue);
    if is_truthy_literal(&condition) {
        return normalize_while_stmt(condition, body, span, source_mode, strict_types);
    }
    vec![Stmt {
        kind: StmtKind::DoWhile { body, condition },
        span,
        source_mode,
        strict_types,
        attributes: Vec::new(),
    }]
}

/// Canonicalizes a pruned `for` loop shell and returns the replacement statements.
///
/// Without an update clause `continue` goes straight to the test, exactly as in a `while`
/// loop, so the init clause is hoisted in front and the loop becomes `while (condition)`
/// (`while (true)` when there is no test). With an update clause the loop keeps its `for`
/// shape and only the shared body/test canonicalizations apply.
pub(crate) fn normalize_for_stmt(
    init: Option<Box<Stmt>>,
    condition: Option<Expr>,
    update: Option<Box<Stmt>>,
    body: Vec<Stmt>,
    span: crate::span::Span,
    source_mode: crate::source::SourceMode,
    strict_types: bool,
) -> Vec<Stmt> {
    if update.is_none() {
        let condition =
            condition.unwrap_or_else(|| Expr::new(ExprKind::BoolLiteral(true), span));
        let mut stmts: Vec<Stmt> = init.map(|stmt| vec![*stmt]).unwrap_or_default();
        stmts.extend(normalize_while_stmt(condition, body, span, source_mode, strict_types));
        return stmts;
    }

    let body = strip_trailing_terminator(body, TailTerminator::LoopContinue);
    let (condition, body) = fold_leading_break_guards(condition, body);
    vec![Stmt {
        kind: StmtKind::For {
            init,
            condition,
            update,
            body,
        },
        span,
        source_mode,
        strict_types,
        attributes: Vec::new(),
    }]
}

/// Returns whether `expr` is a compile-time scalar that PHP treats as true.
fn is_truthy_literal(expr: &Expr) -> bool {
    condition_scalar_value(expr).is_some_and(|value| value.truthy())
}

/// Returns whether `body` is exactly one level-1 `break`.
fn is_single_break(body: &[Stmt]) -> bool {
    matches!(body, [Stmt { kind: StmtKind::Break(1), .. }])
}

/// Folds every leading `if (guard) { break; } [else { ... }]` of `body` into the loop test.
///
/// `while (c) { if (g) { break; } rest }` becomes `while (c && !g) { rest }`; a missing or
/// literal-true test becomes plain `!g`. An `else` block of the guard is the code that ran when
/// the loop did not exit, so it is spliced in front of the remaining body. The fold repeats
/// until the body no longer starts with such a guard.
fn fold_leading_break_guards(
    mut condition: Option<Expr>,
    mut body: Vec<Stmt>,
) -> (Option<Expr>, Vec<Stmt>) {
    loop {
        let is_guard = matches!(
            body.first().map(|stmt| &stmt.kind),
            Some(StmtKind::If {
                then_body,
                elseif_clauses,
                ..
            }) if elseif_clauses.is_empty() && is_single_break(then_body)
        );
        if !is_guard {
            return (condition, body);
        }

        let guard = body.remove(0);
        let StmtKind::If {
            condition: guard_condition,
            else_body,
            ..
        } = guard.kind
        else {
            unreachable!("guard shape was checked above");
        };

        let stay_test = invert_condition(guard_condition);
        condition = Some(match condition {
            Some(test) if !is_truthy_literal(&test) => combine_if_conditions(test, stay_test),
            _ => stay_test,
        });
        if let Some(else_body) = else_body {
            body.splice(0..0, else_body);
        }
    }
}

/// Splits a trailing `if (guard) { break; }` off `body` so the loop can rotate into
/// `do { body } while (!guard)`. Fails, handing the body back, when the loop does not end in
/// such a guard or when the body has a `continue` targeting the loop: that `continue` skips
/// the guard today but would run the rotated test.
fn split_trailing_break_guard(mut body: Vec<Stmt>) -> Result<(Vec<Stmt>, Expr), Vec<Stmt>> {
    let is_guard = matches!(
        body.last().map(|stmt| &stmt.kind),
        Some(StmtKind::If {
            then_body,
            elseif_clauses,
            else_body: None,
            ..
        }) if elseif_clauses.is_empty() && is_single_break(then_body)
    );
    if !is_guard || block_continues_enclosing_loop(&body[..body.len() - 1], 0) {
        return Err(body);
    }

    let guard = body.pop().expect("guard presence was checked above");
    let StmtKind::If { condition, .. } = guard.kind else {
        unreachable!("guard shape was checked above");
    };
    Ok((body, condition))
}
