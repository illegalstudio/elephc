//! Purpose:
//! Regression tests for optimizer prune behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Verifies that constant-false conditions in if/elseif chains are pruned,
/// keeping only the first truthy branch (or the else body if all conditions
/// are false). The else branch is selected when the second elseif condition
/// evaluates to true.
#[test]
fn test_prune_constant_if_chain() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
            then_body: vec![Stmt::echo(Expr::int_lit(1))],
            elseif_clauses: vec![
                (
                    Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                    vec![Stmt::echo(Expr::int_lit(2))],
                ),
                (
                    Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                    vec![Stmt::echo(Expr::int_lit(3))],
                ),
            ],
            else_body: Some(vec![Stmt::echo(Expr::int_lit(4))]),
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(3))]);
}

/// Verifies that a while loop with a false condition is removed entirely,
/// and a do-while loop with a false condition is kept because its body
/// executes at least once before the condition is checked.
#[test]
fn test_prune_while_false_and_do_while_false() {
    let program = vec![
        Stmt::new(
            StmtKind::While {
                condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                body: vec![Stmt::echo(Expr::int_lit(1))],
            },
            Span::dummy(),
        ),
        Stmt::new(
            StmtKind::DoWhile {
                body: vec![Stmt::echo(Expr::int_lit(2))],
                condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
            },
            Span::dummy(),
        ),
    ];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(2))]);
}

/// Verifies that for loop with false condition keeps only the init statement.
/// The condition is false so body and update are pruned; init still runs.
#[test]
fn test_prune_for_false_keeps_init_only() {
    let program = vec![Stmt::new(
        StmtKind::For {
            init: Some(Box::new(Stmt::assign("i", Expr::int_lit(1)))),
            condition: Some(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
            update: Some(Box::new(Stmt::assign("i", Expr::int_lit(2)))),
            body: vec![Stmt::echo(Expr::int_lit(3))],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::assign("i", Expr::int_lit(1))]);
}

/// Verifies that a do-while with a false condition is NOT pruned when the
/// loop body contains a continue that prevents reaching the condition check.
/// The loop exit via continue counts as non-exhaustive control flow.
#[test]
fn test_prune_keeps_do_while_false_with_loop_exit() {
    let program = vec![Stmt::new(
        StmtKind::DoWhile {
            body: vec![
                Stmt::echo(Expr::int_lit(2)),
                Stmt::new(StmtKind::Continue(1), Span::dummy()),
            ],
            condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::new(
            StmtKind::DoWhile {
                body: vec![
                    Stmt::echo(Expr::int_lit(2)),
                    Stmt::new(StmtKind::Continue(1), Span::dummy()),
                ],
                condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
            },
            Span::dummy(),
        )]
    );
}

/// Verifies that statements following an unconditional return inside a
/// function body are pruned. Return is exhaustive so code after it is dead.
#[test]
fn test_prune_block_drops_statements_after_return() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(StmtKind::Return(Some(Expr::int_lit(7))), Span::dummy()),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(body[0].kind, StmtKind::Return(_)));
}

/// Verifies that pure expression statements (no side effects) inside a
/// function body are pruned. Only the echo statement remains.
#[test]
fn test_prune_drops_pure_expr_stmt() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::ExprStmt(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(8))),
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(7)),
            ],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert_eq!(body[0], Stmt::echo(Expr::int_lit(7)));
}

/// Verifies that the unused branch of a ternary with a true condition is
/// pruned. Since condition is true, only the then_expr is kept.
#[test]
fn test_prune_ternary_drops_unused_pure_branch() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Ternary {
                condition: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                then_expr: Box::new(Expr::var("answer")),
                else_expr: Box::new(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(8))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::assign("x", Expr::var("answer"))]);
}

/// Verifies that short-circuit evaluation drops the RHS when the left
/// operand of a logical Or is already truthy. Only the left BoolLiteral(true) is kept.
#[test]
fn test_prune_short_circuit_drops_unused_pure_rhs() {
    let program = vec![Stmt::echo(Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
            op: BinOp::Or,
            right: Box::new(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(8))),
        },
        Span::dummy(),
    ))];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy()))]
    );
}

/// Verifies that if an if statement covers all cases (both branches return),
/// statements after it are pruned as unreachable.
#[test]
fn test_prune_block_drops_statements_after_exhaustive_if() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        elseif_clauses: Vec::new(),
                        else_body: Some(vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(8))),
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    let StmtKind::If { .. } = &body[0].kind else {
        panic!("expected if");
    };
}

/// Verifies that if a switch with a default case covers all possible values,
/// statements after it are pruned. The switch normalizes to an if.
#[test]
fn test_prune_block_drops_statements_after_exhaustive_switch() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::Switch {
                        subject: Expr::var("flag"),
                        cases: vec![(
                            vec![Expr::int_lit(1)],
                            vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(7))),
                                Span::dummy(),
                            )],
                        )],
                        default: Some(vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(8))),
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    let StmtKind::If { .. } = &body[0].kind else {
        panic!("expected normalized if");
    };
}

/// Verifies that statements following a Break inside a switch case are
/// pruned. Break exits the switch, so subsequent statements are unreachable.
#[test]
fn test_prune_switch_case_body_drops_statements_after_break() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(1),
            cases: vec![(
                vec![Expr::int_lit(1)],
                vec![
                    Stmt::new(StmtKind::Break(1), Span::dummy()),
                    Stmt::echo(Expr::int_lit(9)),
                ],
            )],
            default: None,
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    assert!(pruned.is_empty());
}

/// Verifies that a match expression with a known subject value reduces to
/// the matching arm's expression. Subject 3 matches arm with value 3, yielding 20.
#[test]
fn test_prune_match_expr_to_selected_arm() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Match {
                subject: Box::new(Expr::int_lit(3)),
                arms: vec![
                    (vec![Expr::int_lit(1)], Expr::int_lit(10)),
                    (vec![Expr::int_lit(3)], Expr::int_lit(20)),
                ],
                default: Some(Box::new(Expr::int_lit(30))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::assign("x", Expr::int_lit(20))]);
}

/// Verifies that match uses strict (===) case comparison. BoolLiteral(true)
/// does not strictly equal int 1, so default arm (20) is selected.
#[test]
fn test_prune_match_uses_strict_case_comparison() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Match {
                subject: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                arms: vec![(vec![Expr::int_lit(1)], Expr::int_lit(10))],
                default: Some(Box::new(Expr::int_lit(20))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::assign("x", Expr::int_lit(20))]);
}

/// Verifies that when a later match arm's patterns fully shadow an earlier
/// arm (same patterns), the earlier arm is dropped. Arm with [1] -> 10 is dropped.
#[test]
fn test_prune_match_drops_fully_shadowed_duplicate_arm() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Match {
                subject: Box::new(Expr::var("value")),
                arms: vec![
                    (vec![Expr::int_lit(1)], Expr::int_lit(10)),
                    (vec![Expr::int_lit(1)], Expr::int_lit(20)),
                ],
                default: Some(Box::new(Expr::int_lit(30))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::Assign { value, .. } = &pruned[0].kind else {
        panic!("expected assign");
    };
    let ExprKind::Match { arms, default, .. } = &value.kind else {
        panic!("expected match");
    };
    assert_eq!(arms.len(), 1);
    assert_eq!(arms[0].0, vec![Expr::int_lit(1)]);
    assert_eq!(arms[0].1, Expr::int_lit(10));
    assert_eq!(default.as_deref(), Some(&Expr::int_lit(30)));
}

/// Verifies that patterns from an earlier arm that are also present in a
/// later arm are dropped from the earlier arm. Arm [1,2] keeps only pattern 2.
#[test]
fn test_prune_match_drops_shadowed_patterns_from_later_arm() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Match {
                subject: Box::new(Expr::var("value")),
                arms: vec![
                    (vec![Expr::int_lit(1)], Expr::int_lit(10)),
                    (
                        vec![Expr::int_lit(1), Expr::int_lit(2)],
                        Expr::int_lit(20),
                    ),
                ],
                default: Some(Box::new(Expr::int_lit(30))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::Assign { value, .. } = &pruned[0].kind else {
        panic!("expected assign");
    };
    let ExprKind::Match { arms, default, .. } = &value.kind else {
        panic!("expected match");
    };
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].0, vec![Expr::int_lit(1)]);
    assert_eq!(arms[1].0, vec![Expr::int_lit(2)]);
    assert_eq!(arms[1].1, Expr::int_lit(20));
    assert_eq!(default.as_deref(), Some(&Expr::int_lit(30)));
}

/// Verifies that non-matching switch cases before the matching one are
/// pruned. Subject 3 matches case [3], yielding echo(20) before the Break.
#[test]
fn test_prune_switch_drops_leading_non_matching_cases() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(3),
            cases: vec![
                (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(10))]),
                (
                    vec![Expr::int_lit(3)],
                    vec![Stmt::echo(Expr::int_lit(20)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                ),
            ],
            default: Some(vec![Stmt::echo(Expr::int_lit(30))]),
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(20))]);
}
