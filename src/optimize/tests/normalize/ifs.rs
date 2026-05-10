//! Purpose:
//! Regression tests for optimizer normalize ifs behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
fn test_normalize_control_flow_inverts_single_live_else_branch() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("flag"),
            then_body: Vec::new(),
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(
                    ExprKind::Not(Box::new(Expr::var("flag"))),
                    Span::dummy(),
                ),
                then_body: vec![Stmt::echo(Expr::int_lit(7))],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_normalize_control_flow_canonicalizes_elseif_chain_into_nested_else_if() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: vec![Stmt::echo(Expr::int_lit(1))],
            elseif_clauses: vec![(
                Expr::var("b"),
                vec![Stmt::echo(Expr::int_lit(2))],
            )],
            else_body: Some(vec![Stmt::echo(Expr::int_lit(3))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::If {
        condition,
        then_body,
        elseif_clauses,
        else_body,
    } = &pruned[0].kind
    else {
        panic!("expected if");
    };
    assert_eq!(*condition, Expr::var("a"));
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(1))]);
    assert!(elseif_clauses.is_empty());

    let else_body = else_body.as_ref().expect("expected nested else body");
    assert_eq!(else_body.len(), 1);
    let StmtKind::If {
        condition,
        then_body,
        elseif_clauses,
        else_body,
    } = &else_body[0].kind
    else {
        panic!("expected nested if");
    };
    assert_eq!(*condition, Expr::var("b"));
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(2))]);
    assert!(elseif_clauses.is_empty());
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(3))]));
}

#[test]
fn test_normalize_control_flow_merges_identical_if_chain_bodies_into_or_condition() {
    let shared_body = vec![Stmt::echo(Expr::int_lit(7))];
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: shared_body.clone(),
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("b"),
                    then_body: shared_body.clone(),
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )]),
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
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::var("a")),
                        op: BinOp::Or,
                        right: Box::new(Expr::var("b")),
                    },
                    Span::dummy(),
                )
            );
            assert_eq!(then_body, &shared_body);
            assert!(elseif_clauses.is_empty());
            assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_merges_identical_if_chain_tail_into_inverted_and() {
    let shared_tail = vec![Stmt::echo(Expr::int_lit(9))];
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: shared_tail.clone(),
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("b"),
                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(shared_tail.clone()),
                },
                Span::dummy(),
            )]),
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
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::new(
                            ExprKind::Not(Box::new(Expr::var("a"))),
                            Span::dummy(),
                        )),
                        op: BinOp::And,
                        right: Box::new(Expr::var("b")),
                    },
                    Span::dummy(),
                )
            );
            assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
            assert!(elseif_clauses.is_empty());
            assert_eq!(else_body, &Some(shared_tail));
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_recursively_merges_longer_if_chain_heads() {
    let shared_body = vec![Stmt::echo(Expr::int_lit(7))];
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: shared_body.clone(),
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("b"),
                    then_body: shared_body.clone(),
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("c"),
                            then_body: shared_body.clone(),
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                        },
                        Span::dummy(),
                    )]),
                },
                Span::dummy(),
            )]),
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
                    Expr::var("a"),
                    combine_if_chain_conditions(Expr::var("b"), Expr::var("c")),
                )
            );
            assert_eq!(then_body, &shared_body);
            assert!(elseif_clauses.is_empty());
            assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_flattens_nested_single_path_ifs() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("b"),
                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
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
            assert!(else_body.is_none());
            assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
            assert_eq!(
                *condition,
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::var("a")),
                        op: BinOp::And,
                        right: Box::new(Expr::var("b")),
                    },
                    Span::dummy(),
                )
            );
        }
        other => panic!("expected flattened if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_collapses_identical_if_branches_to_condition_effects_plus_body() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("tick"),
                    args: Vec::new(),
                },
                Span::dummy(),
            ),
            then_body: vec![Stmt::echo(Expr::int_lit(7))],
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![
            Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::unqualified("tick"),
                        args: Vec::new(),
                    },
                    Span::dummy(),
                )),
                Span::dummy(),
            ),
            Stmt::echo(Expr::int_lit(7))
        ]
    );
}
