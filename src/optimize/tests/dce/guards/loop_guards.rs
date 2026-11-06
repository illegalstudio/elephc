//! Purpose:
//! Regression tests for pre-tested loop-condition facts in AST DCE.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness.
//!
//! Key details:
//! - Covers pure `while`/`for` entry facts, sequential write invalidation,
//!   impure-condition refusal, float/NaN conservatism, and the `do...while` exclusion.

use super::*;

/// Builds one function with a single typed parameter and the supplied body.
fn function_with_typed_param(type_expr: TypeExpr, body: Vec<Stmt>) -> Vec<Stmt> {
    vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(type_expr), None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body,
        },
        Span::dummy(),
    )]
}

/// Builds a two-arm nested guard whose branches remain distinguishable after DCE.
fn guarded_echo(condition: Expr) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition,
            then_body: vec![Stmt::echo(Expr::int_lit(7))],
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
        },
        Span::dummy(),
    )
}

/// Runs DCE and extracts the sole test function's optimized body.
fn optimized_function_body(program: Vec<Stmt>) -> Vec<Stmt> {
    let mut eliminated = eliminate_dead_code(program);
    match eliminated.remove(0).kind {
        StmtKind::FunctionDecl { body, .. } => body,
        _ => panic!("expected function"),
    }
}

/// Verifies a pure true `while` condition strengthens nested integer range guards.
#[test]
fn test_eliminate_dead_code_strengthens_while_body_from_loop_condition() {
    let program = function_with_typed_param(
        TypeExpr::Int,
        vec![Stmt::new(
            StmtKind::While {
                condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(10)),
                body: vec![guarded_echo(Expr::binop(
                    Expr::var("x"),
                    BinOp::Gt,
                    Expr::int_lit(5),
                ))],
            },
            Span::dummy(),
        )],
    );

    let body = optimized_function_body(program);
    let StmtKind::While { body, .. } = &body[0].kind else {
        panic!("expected while");
    };
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

/// Verifies a pure true `for` condition strengthens nested integer range guards.
#[test]
fn test_eliminate_dead_code_strengthens_for_body_from_loop_condition() {
    let program = function_with_typed_param(
        TypeExpr::Int,
        vec![Stmt::new(
            StmtKind::For {
                init: None,
                condition: Some(Expr::binop(
                    Expr::var("x"),
                    BinOp::Gt,
                    Expr::int_lit(10),
                )),
                update: None,
                body: vec![guarded_echo(Expr::binop(
                    Expr::var("x"),
                    BinOp::Gt,
                    Expr::int_lit(5),
                ))],
            },
            Span::dummy(),
        )],
    );

    let body = optimized_function_body(program);
    let StmtKind::For { body, .. } = &body[0].kind else {
        panic!("expected for");
    };
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

/// Verifies a body write clears the loop-condition fact before a later nested guard.
#[test]
fn test_eliminate_dead_code_invalidates_loop_condition_after_body_write() {
    let program = function_with_typed_param(
        TypeExpr::Int,
        vec![Stmt::new(
            StmtKind::While {
                condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(10)),
                body: vec![
                    Stmt::new(
                        StmtKind::Assign {
                            name: "x".into(),
                            value: Expr::int_lit(0),
                        },
                        Span::dummy(),
                    ),
                    guarded_echo(Expr::binop(
                        Expr::var("x"),
                        BinOp::Gt,
                        Expr::int_lit(10),
                    )),
                ],
            },
            Span::dummy(),
        )],
    );

    let body = optimized_function_body(program);
    let StmtKind::While { body, .. } = &body[0].kind else {
        panic!("expected while");
    };
    assert!(matches!(body[1].kind, StmtKind::If { .. }));
}

/// Verifies a call-bearing loop condition does not seed even a structural nested fact.
#[test]
fn test_eliminate_dead_code_refuses_impure_loop_condition_guards() {
    let impure_condition = Expr::binop(
        Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(10)),
        BinOp::And,
        Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("touch"),
                args: Vec::new(),
            },
            Span::dummy(),
        ),
    );
    let program = function_with_typed_param(
        TypeExpr::Int,
        vec![Stmt::new(
            StmtKind::While {
                condition: impure_condition,
                body: vec![guarded_echo(Expr::binop(
                    Expr::var("x"),
                    BinOp::Gt,
                    Expr::int_lit(10),
                ))],
            },
            Span::dummy(),
        )],
    );

    let body = optimized_function_body(program);
    let StmtKind::While { body, .. } = &body[0].kind else {
        panic!("expected while");
    };
    assert!(matches!(body[0].kind, StmtKind::If { .. }));
}

/// Verifies a float loop subject keeps the fractional/NaN-sensitive nested branch.
#[test]
fn test_eliminate_dead_code_keeps_float_gap_under_loop_condition() {
    let program = function_with_typed_param(
        TypeExpr::Float,
        vec![Stmt::new(
            StmtKind::While {
                condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(10)),
                body: vec![guarded_echo(Expr::binop(
                    Expr::var("x"),
                    BinOp::GtEq,
                    Expr::int_lit(11),
                ))],
            },
            Span::dummy(),
        )],
    );

    let body = optimized_function_body(program);
    let StmtKind::While { body, .. } = &body[0].kind else {
        panic!("expected while");
    };
    assert!(matches!(body[0].kind, StmtKind::If { .. }));
}

/// Verifies `do...while` does not reuse its condition before the first body execution.
#[test]
fn test_eliminate_dead_code_does_not_seed_do_while_body_from_loop_condition() {
    let program = function_with_typed_param(
        TypeExpr::Int,
        vec![Stmt::new(
            StmtKind::DoWhile {
                body: vec![guarded_echo(Expr::binop(
                    Expr::var("x"),
                    BinOp::Gt,
                    Expr::int_lit(5),
                ))],
                condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(10)),
            },
            Span::dummy(),
        )],
    );

    let body = optimized_function_body(program);
    let StmtKind::DoWhile { body, .. } = &body[0].kind else {
        panic!("expected do-while");
    };
    assert!(matches!(body[0].kind, StmtKind::If { .. }));
}
