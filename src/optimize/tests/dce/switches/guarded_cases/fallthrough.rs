//! Purpose:
//! Regression tests for switch case guards at direct-entry and fall-through joins.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness.
//!
//! Key details:
//! - A case pattern is valid inside its body only when every reachable entry
//!   evaluates and matches that pattern; fall-through entries must remain conservative.

use super::*;

/// Builds an integer comparison against `$x` for case and nested guard fixtures.
fn x_comparison(op: BinOp, value: i64) -> Expr {
    Expr::binop(Expr::var("x"), op, Expr::int_lit(value))
}

/// Builds a two-arm nested condition whose retained or pruned shape is observable in the AST.
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

/// Runs DCE on a `switch (true)` inside a function with an exact `int` parameter.
fn optimized_switch_cases(
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
) -> Vec<(Vec<Expr>, Vec<Stmt>)> {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "run".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(TypeExpr::Int), None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                    cases,
                    default: None,
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
    let StmtKind::Switch { cases, .. } = body.remove(0).kind else {
        panic!("expected switch");
    };
    cases
}

/// Verifies fall-through does not assume the current case's stronger integer range.
#[test]
fn test_eliminate_dead_code_does_not_extend_range_guard_on_switch_fallthrough() {
    let cases = optimized_switch_cases(vec![
        (
            vec![x_comparison(BinOp::Gt, 0)],
            vec![Stmt::echo(Expr::int_lit(6))],
        ),
        (
            vec![x_comparison(BinOp::Gt, 100)],
            vec![
                guarded_echo(x_comparison(BinOp::Gt, 50)),
                Stmt::new(StmtKind::Break(1), Span::dummy()),
            ],
        ),
    ]);

    assert!(cases[0]
        .1
        .iter()
        .any(|stmt| matches!(stmt.kind, StmtKind::If { .. })));
}

/// Verifies fall-through does not assume even a structurally identical case condition.
#[test]
fn test_eliminate_dead_code_does_not_extend_structural_guard_on_switch_fallthrough() {
    let current_case = x_comparison(BinOp::Gt, 100);
    let cases = optimized_switch_cases(vec![
        (
            vec![x_comparison(BinOp::Gt, 0)],
            vec![Stmt::echo(Expr::int_lit(6))],
        ),
        (
            vec![current_case.clone()],
            vec![
                guarded_echo(current_case),
                Stmt::new(StmtKind::Break(1), Span::dummy()),
            ],
        ),
    ]);

    assert!(cases[0]
        .1
        .iter()
        .any(|stmt| matches!(stmt.kind, StmtKind::If { .. })));
}

/// Verifies a case reached only by direct matching still receives its pattern range.
#[test]
fn test_eliminate_dead_code_keeps_case_guard_for_direct_only_switch_entry() {
    let cases = optimized_switch_cases(vec![
        (
            vec![x_comparison(BinOp::Lt, 0)],
            vec![
                Stmt::echo(Expr::int_lit(6)),
                Stmt::new(StmtKind::Break(1), Span::dummy()),
            ],
        ),
        (
            vec![x_comparison(BinOp::Gt, 100)],
            vec![
                guarded_echo(x_comparison(BinOp::Gt, 50)),
                Stmt::new(StmtKind::Break(1), Span::dummy()),
            ],
        ),
    ]);

    assert_eq!(cases[1].1[0], Stmt::echo(Expr::int_lit(7)));
}

/// Verifies a break-only case remains a control-flow barrier before later case bodies.
#[test]
fn test_eliminate_dead_code_preserves_break_only_switch_case_barrier() {
    let break_stmt = Stmt::new(StmtKind::Break(1), Span::dummy());
    let cases = optimized_switch_cases(vec![
        (
            vec![x_comparison(BinOp::Lt, 0)],
            vec![break_stmt.clone()],
        ),
        (
            vec![x_comparison(BinOp::Gt, 100)],
            vec![Stmt::echo(Expr::int_lit(7)), break_stmt.clone()],
        ),
    ]);

    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].1, vec![break_stmt]);
    assert_eq!(
        cases[1].1,
        vec![
            Stmt::echo(Expr::int_lit(7)),
            Stmt::new(StmtKind::Break(1), Span::dummy()),
        ]
    );
}
