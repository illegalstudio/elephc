//! Purpose:
//! Regression tests for optimizer dce switches case_shadowing behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
// Verifies DCE eliminates the second case when it has the same pattern as the first, preserving only the first case body.
fn test_eliminate_dead_code_drops_switch_case_shadowed_by_terminating_duplicate_pattern() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::var("x"),
                    cases: vec![
                        (
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::int_lit(7)),
                                Stmt::new(StmtKind::Break(1), Span::dummy()),
                            ],
                        ),
                        (
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::int_lit(8)),
                                Stmt::new(StmtKind::Break(1), Span::dummy()),
                            ],
                        ),
                    ],
                    default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, vec![Expr::int_lit(1)]);
    assert_eq!(
        cases[0].1,
        vec![
            Stmt::echo(Expr::int_lit(7)),
            Stmt::new(StmtKind::Break(1), Span::dummy()),
        ]
    );
    assert_eq!(default, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
// Verifies DCE merges the first case body with the second when the second is unreachable due to pattern shadowing.
fn test_eliminate_dead_code_merges_fallthrough_body_from_fully_shadowed_switch_case() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::var("x"),
                    cases: vec![
                        (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
                        (
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::int_lit(8)),
                                Stmt::new(StmtKind::Break(1), Span::dummy()),
                            ],
                        ),
                    ],
                    default: None,
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
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, vec![Expr::int_lit(1)]);
    assert_eq!(
        cases[0].1,
        vec![
            Stmt::echo(Expr::int_lit(7)),
            Stmt::echo(Expr::int_lit(8)),
            Stmt::new(StmtKind::Break(1), Span::dummy()),
        ]
    );
    assert_eq!(default, &None);
}

#[test]
// Verifies DCE removes dead case labels (0 and 1) from a switch with mixed live patterns (2 and true), keeping only live patterns.
fn test_eliminate_dead_code_prunes_dead_label_inside_live_mixed_switch_case() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("value"),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::new(
                                ExprKind::BinaryOp {
                                    left: Box::new(Expr::var("value")),
                                    op: BinOp::StrictNotEq,
                                    right: Box::new(Expr::int_lit(1)),
                                },
                                Span::dummy(),
                            ),
                            then_body: vec![Stmt::new(
                                StmtKind::Switch {
                                    subject: Expr::var("value"),
                                    cases: vec![
                                        (
                                            vec![Expr::int_lit(0)],
                                            vec![
                                                Stmt::echo(Expr::int_lit(7)),
                                                Stmt::new(StmtKind::Break(1), Span::dummy()),
                                            ],
                                        ),
                                        (
                                            vec![
                                                Expr::int_lit(1),
                                                Expr::int_lit(2),
                                                Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                                            ],
                                            vec![Stmt::echo(Expr::int_lit(8))],
                                        ),
                                    ],
                                    default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                                },
                                Span::dummy(),
                            )],
                            elseif_clauses: Vec::new(),
                            else_body: None,
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
    let (cases, default) = match &then_body[0].kind {
        StmtKind::If { then_body, .. } => match &then_body[0].kind {
            StmtKind::Switch { cases, default, .. } => (cases, default),
            _ => panic!("expected switch in inner if"),
        },
        StmtKind::Switch { cases, default, .. } => (cases, default),
        _ => panic!("expected inner if or switch"),
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(
        cases[0].0,
        vec![Expr::int_lit(2), Expr::new(ExprKind::BoolLiteral(true), Span::dummy())]
    );
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
    assert!(default.is_none());
}
