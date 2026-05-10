//! Purpose:
//! Regression tests for optimizer normalize switches behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
fn test_normalize_control_flow_materializes_constant_switch_match() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(2),
            cases: vec![
                (
                    vec![Expr::int_lit(1)],
                    vec![Stmt::echo(Expr::int_lit(5)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                ),
                (
                    vec![Expr::int_lit(2)],
                    vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                ),
            ],
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_normalize_control_flow_materializes_constant_switch_fallthrough() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(1),
            cases: vec![
                (vec![Expr::int_lit(1)], Vec::new()),
                (
                    vec![Expr::int_lit(2)],
                    vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                ),
            ],
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_normalize_control_flow_materializes_constant_switch_default() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(3),
            cases: vec![(
                vec![Expr::int_lit(1)],
                vec![Stmt::echo(Expr::int_lit(5)), Stmt::new(StmtKind::Break(1), Span::dummy())],
            )],
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(9))]);
}

#[test]
fn test_normalize_control_flow_rewrites_single_case_switch_to_if() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![(
                vec![Expr::int_lit(1)],
                vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
            )],
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            assert!(elseif_clauses.is_empty());
            assert_eq!(
                *condition,
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::var("x")),
                        op: BinOp::Eq,
                        right: Box::new(Expr::int_lit(1)),
                    },
                    Span::dummy(),
                )
            );
            assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
            assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_merges_adjacent_identical_switch_cases() {
    let shared_body = vec![
        Stmt::echo(Expr::int_lit(7)),
        Stmt::new(StmtKind::Break(1), Span::dummy()),
    ];
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![
                (vec![Expr::int_lit(1)], shared_body.clone()),
                (vec![Expr::int_lit(2)], shared_body.clone()),
                (
                    vec![Expr::int_lit(3)],
                    vec![Stmt::echo(Expr::int_lit(9)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                ),
            ],
            default: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            assert_eq!(*subject, Expr::var("x"));
            assert_eq!(cases.len(), 2);
            assert_eq!(cases[0].0, vec![Expr::int_lit(1), Expr::int_lit(2)]);
            assert_eq!(cases[0].1, shared_body);
            assert_eq!(cases[1].0, vec![Expr::int_lit(3)]);
            assert_eq!(
                cases[1].1,
                vec![Stmt::echo(Expr::int_lit(9)), Stmt::new(StmtKind::Break(1), Span::dummy())]
            );
            assert!(default.is_none());
        }
        other => panic!("expected normalized switch, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_merges_fallthrough_switch_labels_into_next_case() {
    let shared_body = vec![
        Stmt::echo(Expr::int_lit(7)),
        Stmt::new(StmtKind::Break(1), Span::dummy()),
    ];
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![
                (vec![Expr::int_lit(1)], Vec::new()),
                (vec![Expr::int_lit(2)], Vec::new()),
                (vec![Expr::int_lit(3)], shared_body.clone()),
            ],
            default: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            assert_eq!(
                *condition,
                combine_if_chain_conditions(
                    combine_if_chain_conditions(
                        Expr::new(
                            ExprKind::BinaryOp {
                                left: Box::new(Expr::var("x")),
                                op: BinOp::Eq,
                                right: Box::new(Expr::int_lit(1)),
                            },
                            Span::dummy(),
                        ),
                        Expr::new(
                            ExprKind::BinaryOp {
                                left: Box::new(Expr::var("x")),
                                op: BinOp::Eq,
                                right: Box::new(Expr::int_lit(2)),
                            },
                            Span::dummy(),
                        ),
                    ),
                    Expr::new(
                        ExprKind::BinaryOp {
                            left: Box::new(Expr::var("x")),
                            op: BinOp::Eq,
                            right: Box::new(Expr::int_lit(3)),
                        },
                        Span::dummy(),
                    ),
                )
            );
            assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
            assert!(elseif_clauses.is_empty());
            assert!(else_body.is_none());
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}
