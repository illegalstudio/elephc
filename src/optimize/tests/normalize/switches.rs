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

/// Switch with constant subject matching a case emits only that case body.
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

/// Switch with constant subject that falls through to a later case emits that case body.
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

/// Switch with constant subject not matching any case emits the default body.
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

/// Rewrites a single-case switch with a default to an equivalent if statement.
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

/// Merges adjacent switch cases with identical bodies by combining their match expressions.
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
            // The last body leaves the switch by falling off it, so its `break` is dropped.
            assert_eq!(cases[1].1, vec![Stmt::echo(Expr::int_lit(9))]);
            assert!(default.is_none());
        }
        other => panic!("expected normalized switch, got {:?}", other),
    }
}

/// Rewrites a switch with empty cases that fall through into a single if chain with Or conditions.
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

/// Builds the single-case switch whose DEFAULT body the rewrite materializes twice.
///
/// One case with a body that FALLS THROUGH (no `break`) is what makes the default body reachable
/// from the matching path as well, so `materialize_switch_execution` appends it to the `then`
/// branch and emits it again as the `else` branch. `decision_span` is the span of the default
/// body's assignment — the node a checker local-binding decision would be filed against.
fn single_case_fallthrough_switch(decision_span: Span) -> Program {
    vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![(vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))])],
            default: Some(vec![Stmt::new(
                StmtKind::Assign {
                    name: "a".to_string(),
                    value: Expr::int_lit(9),
                },
                decision_span,
            )]),
        },
        Span::dummy(),
    )]
}

/// Control for the guard below: with no local-binding decision in play, the single-case rewrite
/// still fires — and it really does write the default body's statement into BOTH branches.
///
/// The two `decision_span` assertions are the anti-vacuity half of the guard test: they pin that
/// this shape duplicates a span, so a guarded run that leaves the switch alone is measuring
/// something real rather than a rewrite that never applied.
#[test]
fn test_normalize_control_flow_materializes_a_single_case_default_into_both_branches() {
    let decision_span = Span::new(7, 1);

    let pruned = normalize_control_flow(single_case_fallthrough_switch(decision_span));

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
            assert_eq!(then_body.len(), 2);
            assert_eq!(then_body[0], Stmt::echo(Expr::int_lit(7)));
            assert_eq!(then_body[1].span, decision_span);
            let else_body = else_body.as_ref().expect("the default body must become the else");
            assert_eq!(else_body.len(), 1);
            assert_eq!(else_body[0].span, decision_span);
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

/// The same rewrite vetoes itself when the default body carries a checker local-binding decision.
///
/// The decisions are keyed BY SPAN and a clone carries the original's span, so materializing this
/// default into both branches would leave ONE decision naming TWO statements — the singularity
/// `checker::binding_decision_ambiguity` certified on the original program. The switch is left
/// exactly as it arrived, which costs this one optimization and nothing else.
///
/// Calls `crate::optimize::normalize_control_flow` directly rather than the test tree's shadow,
/// because the shadow installs the empty set every hand-built fixture wants.
#[test]
fn test_normalize_control_flow_keeps_a_single_case_switch_carrying_a_binding_decision() {
    let decision_span = Span::new(7, 1);

    let guarded = crate::optimize::normalize_control_flow(
        single_case_fallthrough_switch(decision_span),
        HashSet::from_iter([decision_span]),
    );

    assert_eq!(guarded.len(), 1);
    match &guarded[0].kind {
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            assert_eq!(*subject, Expr::var("x"));
            assert_eq!(cases.len(), 1);
            assert_eq!(cases[0].0, vec![Expr::int_lit(1)]);
            assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(7))]);
            let default = default.as_ref().expect("the default body must survive the veto");
            assert_eq!(default.len(), 1);
            assert_eq!(default[0].span, decision_span);
        }
        other => panic!("expected the switch to be left unpruned, got {:?}", other),
    }
}

/// The veto is about the DECISION, not about single-case switches: a decision span that names no
/// node in this switch leaves the rewrite firing exactly as the control above.
#[test]
fn test_normalize_control_flow_still_rewrites_when_the_decision_names_another_node() {
    let decision_span = Span::new(7, 1);

    let pruned = crate::optimize::normalize_control_flow(
        single_case_fallthrough_switch(decision_span),
        HashSet::from_iter([Span::new(99, 1)]),
    );

    assert_eq!(pruned.len(), 1);
    assert!(
        matches!(pruned[0].kind, StmtKind::If { .. }),
        "expected the rewrite to fire, got {:?}",
        pruned[0].kind
    );
}

/// The `default` body runs last, so its trailing `break` is dropped while the case bodies
/// keep theirs: `case 1: echo 1; break; default: echo 2; break;` keeps the first `break`
/// and loses the second.
#[test]
fn test_normalize_control_flow_drops_trailing_break_of_default_body() {
    let first_body = vec![
        Stmt::echo(Expr::int_lit(1)),
        Stmt::new(StmtKind::Break(1), Span::dummy()),
    ];
    let second_body = vec![
        Stmt::echo(Expr::int_lit(2)),
        Stmt::new(StmtKind::Break(1), Span::dummy()),
    ];
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![
                (vec![Expr::int_lit(1)], first_body.clone()),
                (vec![Expr::int_lit(2)], second_body.clone()),
            ],
            default: Some(vec![
                Stmt::echo(Expr::int_lit(3)),
                Stmt::new(StmtKind::Break(1), Span::dummy()),
            ]),
        },
        Span::dummy(),
    )];

    let normalized = normalize_control_flow(program);

    assert_eq!(
        normalized,
        vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::var("x"),
                cases: vec![
                    (vec![Expr::int_lit(1)], first_body),
                    (vec![Expr::int_lit(2)], second_body),
                ],
                default: Some(vec![Stmt::echo(Expr::int_lit(3))]),
            },
            Span::dummy(),
        )]
    );
}

/// With a `default` present, a `break`-only last case still exits the switch instead of
/// falling into `default`, so that `break` is kept.
#[test]
fn test_normalize_control_flow_keeps_break_only_last_case_before_default() {
    let cases = vec![
        (
            vec![Expr::int_lit(1)],
            vec![
                Stmt::echo(Expr::int_lit(1)),
                Stmt::new(StmtKind::Break(1), Span::dummy()),
            ],
        ),
        (
            vec![Expr::int_lit(2)],
            vec![Stmt::new(StmtKind::Break(1), Span::dummy())],
        ),
    ];
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: cases.clone(),
            default: Some(vec![Stmt::echo(Expr::int_lit(3))]),
        },
        Span::dummy(),
    )];

    let normalized = normalize_control_flow(program);

    assert_eq!(
        normalized,
        vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::var("x"),
                cases,
                default: Some(vec![Stmt::echo(Expr::int_lit(3))]),
            },
            Span::dummy(),
        )]
    );
}

/// Without a `default`, a `break`-only last case becomes an empty trailing case, which the
/// existing empty-case folding already represents as "match and leave".
#[test]
fn test_normalize_control_flow_empties_break_only_last_case_without_default() {
    let first_body = vec![
        Stmt::echo(Expr::int_lit(1)),
        Stmt::new(StmtKind::Break(1), Span::dummy()),
    ];
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![
                (vec![Expr::int_lit(1)], first_body.clone()),
                (
                    vec![Expr::int_lit(2)],
                    vec![Stmt::new(StmtKind::Break(1), Span::dummy())],
                ),
            ],
            default: None,
        },
        Span::dummy(),
    )];

    let normalized = normalize_control_flow(program);

    assert_eq!(
        normalized,
        vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::var("x"),
                cases: vec![
                    (vec![Expr::int_lit(1)], first_body),
                    (vec![Expr::int_lit(2)], Vec::new()),
                ],
                default: None,
            },
            Span::dummy(),
        )]
    );
}

/// Builds a span starting at `line:1` so fixtures can encode source order.
fn span_at_line(line: u32) -> Span {
    Span {
        line,
        col: 1,
        end_line: line,
        end_col: 1,
    }
}

/// A `default` written between two cases is not the last body of the switch: falling off it
/// would enter the following case, so its trailing `break` is kept (and nothing else in that
/// switch is stripped). The same shape with the `default` written last drops its `break`.
#[test]
fn test_normalize_control_flow_keeps_break_of_default_written_between_cases() {
    let case_body = |value: i64, line: u32| {
        vec![
            Stmt::new(StmtKind::Echo(Expr::int_lit(value)), span_at_line(line)),
            Stmt::new(StmtKind::Break(1), span_at_line(line)),
        ]
    };
    let default_body = |line: u32| {
        vec![
            Stmt::new(StmtKind::Echo(Expr::int_lit(9)), span_at_line(line)),
            Stmt::new(StmtKind::Break(1), span_at_line(line)),
        ]
    };
    let pattern = |value: i64, line: u32| {
        vec![Expr::new(ExprKind::IntLiteral(value), span_at_line(line))]
    };

    // case 1 (line 1), default (line 2), case 2 (line 3)
    let middle = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![
                (pattern(1, 1), case_body(1, 1)),
                (pattern(2, 3), case_body(2, 3)),
            ],
            default: Some(default_body(2)),
        },
        Span::dummy(),
    )];
    assert_eq!(normalize_control_flow(middle.clone()), middle);

    // case 1 (line 1), case 2 (line 2), default (line 3)
    let last = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![
                (pattern(1, 1), case_body(1, 1)),
                (pattern(2, 2), case_body(2, 2)),
            ],
            default: Some(default_body(3)),
        },
        Span::dummy(),
    )];
    assert_eq!(
        normalize_control_flow(last),
        vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::var("x"),
                cases: vec![
                    (pattern(1, 1), case_body(1, 1)),
                    (pattern(2, 2), case_body(2, 2)),
                ],
                default: Some(vec![Stmt::new(
                    StmtKind::Echo(Expr::int_lit(9)),
                    span_at_line(3),
                )]),
            },
            Span::dummy(),
        )]
    );
}

/// A switch whose `default` sits between cases is left structurally untouched: the empty
/// `case 1` before it must keep falling into the `default`, not be merged into `case 2`, and
/// the fallthrough `default` keeps its position.
#[test]
fn test_normalize_control_flow_keeps_switch_with_default_between_cases_untouched() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![
                (
                    vec![Expr::new(ExprKind::IntLiteral(1), span_at_line(1))],
                    Vec::new(),
                ),
                (
                    vec![Expr::new(ExprKind::IntLiteral(2), span_at_line(3))],
                    vec![
                        Stmt::new(StmtKind::Echo(Expr::int_lit(2)), span_at_line(3)),
                        Stmt::new(StmtKind::Break(1), span_at_line(3)),
                    ],
                ),
            ],
            default: Some(vec![Stmt::new(
                StmtKind::Echo(Expr::int_lit(9)),
                span_at_line(2),
            )]),
        },
        Span::dummy(),
    )];

    assert_eq!(normalize_control_flow(program.clone()), program);
}
