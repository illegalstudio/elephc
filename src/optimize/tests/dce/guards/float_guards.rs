//! Purpose:
//! Regression tests pinning DCE guard literals to PHP's `===` semantics for floats.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness.
//!
//! Key details:
//! - `0.0 === -0.0` is `true` in PHP but the two have different bit patterns, so a guard state
//!   keyed on bits prunes a branch the program can actually reach.
//! - The fixtures put the guard literal on the *left* of the inner comparison; with the literal
//!   on the right the structural `condition_guards` lookup answers first and hides the bug.

use super::*;

/// Builds `function probe($x) { if ($x === outer) { if (inner === $x) { echo 1; } else { echo 2; } } }`
/// and returns the inner if-statement's surviving branches after DCE.
fn eliminate_nested_float_guard(outer: Expr, inner: Expr) -> Vec<Stmt> {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "probe".into(),
            type_params: Vec::new(),
            params: vec![("x".to_string(), None, None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::StrictEq, outer),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(inner, BinOp::StrictEq, Expr::var("x")),
                            then_body: vec![Stmt::echo(Expr::int_lit(1))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(2))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);
    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected outer if");
    };
    then_body.clone()
}

/// Verifies a `=== 0.0` guard does not prune a nested `-0.0 === $x` branch.
///
/// PHP prints `1` here (`-0.0 === 0.0` is `true`), but a bit-pattern guard comparison decided
/// the inner condition was false and eliminated the reachable branch.
#[test]
fn test_signed_zero_guard_keeps_reachable_branch() {
    let then_body = eliminate_nested_float_guard(Expr::float_lit(0.0), Expr::float_lit(-0.0));
    assert_eq!(then_body, vec![Stmt::echo(Expr::int_lit(1))]);
}

/// Verifies the mirrored fixture: a `=== -0.0` guard proves a nested `0.0 === $x` is true.
#[test]
fn test_negative_zero_guard_keeps_reachable_branch() {
    let then_body = eliminate_nested_float_guard(Expr::float_lit(-0.0), Expr::float_lit(0.0));
    assert_eq!(then_body, vec![Stmt::echo(Expr::int_lit(1))]);
}

/// Verifies a guard on a different float value still prunes the impossible branch.
///
/// Guards the fix above against over-correcting into "never conclude anything about floats".
#[test]
fn test_distinct_float_guard_still_prunes() {
    let then_body = eliminate_nested_float_guard(Expr::float_lit(1.0), Expr::float_lit(2.0));
    assert_eq!(then_body, vec![Stmt::echo(Expr::int_lit(2))]);
}

/// Verifies a `!== 0.0` exclusion guard rules out a nested `-0.0 === $x`.
///
/// `-0.0 === 0.0` is `true`, so a value that is provably not `0.0` cannot be `-0.0` either;
/// the fix makes the excluded-value lookup see that through PHP's `===` instead of bits.
#[test]
fn test_signed_zero_exclusion_guard_prunes_impossible_branch() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "probe".into(),
            type_params: Vec::new(),
            params: vec![("x".to_string(), None, None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("x"),
                        BinOp::StrictNotEq,
                        Expr::float_lit(0.0),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::float_lit(-0.0),
                                BinOp::StrictEq,
                                Expr::var("x"),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(1))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(2))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);
    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected outer if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(2))]);
}
