//! Purpose:
//! Regression tests for guard invalidation inside `foreach` bodies.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness.
//!
//! Key details:
//! - A by-reference value aliases the iterable root, while a by-value value
//!   does not; body-entry guards must distinguish those two cases.

use super::*;

/// Builds the loose array equality condition used before and inside the loop.
fn array_equality_condition() -> Expr {
    Expr::binop(Expr::var("a"), BinOp::Eq, Expr::var("b"))
}

/// Builds a nested equality branch whose optimized shape exposes stale guards.
fn guarded_array_equality() -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition: array_equality_condition(),
            then_body: vec![Stmt::echo(Expr::int_lit(7))],
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
        },
        Span::dummy(),
    )
}

/// Runs DCE and returns the optimized body of a guarded `foreach` fixture.
fn optimized_foreach_body(value_by_ref: bool) -> Vec<Stmt> {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "run".into(),
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: array_equality_condition(),
                    then_body: vec![Stmt::new(
                        StmtKind::Foreach {
                            array: Expr::var("a"),
                            key_var: None,
                            value_var: "value".into(),
                            value_by_ref,
                            body: vec![
                                Stmt::new(
                                    StmtKind::Assign {
                                        name: "value".into(),
                                        value: Expr::int_lit(2),
                                    },
                                    Span::dummy(),
                                ),
                                guarded_array_equality(),
                            ],
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

    let mut eliminated = eliminate_dead_code(program);
    let StmtKind::FunctionDecl { mut body, .. } = eliminated.remove(0).kind else {
        panic!("expected function");
    };
    let StmtKind::If { mut then_body, .. } = body.remove(0).kind else {
        panic!("expected outer if");
    };
    let StmtKind::Foreach { body, .. } = then_body.remove(0).kind else {
        panic!("expected foreach");
    };
    body
}

/// Runs DCE on a guard established after a by-ref `foreach` leaves its value alias alive.
fn optimized_post_foreach_guard_body() -> Vec<Stmt> {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "run".into(),
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::Foreach {
                        array: Expr::var("a"),
                        key_var: None,
                        value_var: "value".into(),
                        value_by_ref: true,
                        body: Vec::new(),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::If {
                        condition: array_equality_condition(),
                        then_body: vec![
                            Stmt::new(
                                StmtKind::Assign {
                                    name: "value".into(),
                                    value: Expr::int_lit(2),
                                },
                                Span::dummy(),
                            ),
                            guarded_array_equality(),
                        ],
                        elseif_clauses: Vec::new(),
                        else_body: None,
                    },
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let mut eliminated = eliminate_dead_code(program);
    let StmtKind::FunctionDecl { mut body, .. } = eliminated.remove(0).kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = body.remove(1).kind else {
        panic!("expected outer if after foreach");
    };
    then_body
}

/// Verifies writes through a by-ref value invalidate guards on the iterable root.
#[test]
fn test_eliminate_dead_code_invalidates_iterable_guard_for_by_ref_foreach_value() {
    let body = optimized_foreach_body(true);

    assert!(matches!(body[1].kind, StmtKind::If { .. }));
}

/// Verifies a by-value iteration keeps valid guards on the unchanged iterable root.
#[test]
fn test_eliminate_dead_code_keeps_iterable_guard_for_by_value_foreach_value() {
    let body = optimized_foreach_body(false);

    assert_eq!(body[1], Stmt::echo(Expr::int_lit(7)));
}

/// Verifies the iterable root remains untrackable while the post-loop value alias survives.
#[test]
fn test_eliminate_dead_code_invalidates_iterable_guard_for_lingering_foreach_alias() {
    let body = optimized_post_foreach_guard_body();

    assert!(matches!(body[1].kind, StmtKind::If { .. }));
}
