//! Purpose:
//! Regression tests for optimizer control behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Verifies that `analyze_switch_tail_paths` classifies both case tails as FallsThrough
/// when neither case body terminates with a break or return.
#[test]
fn test_switch_tail_reachability_tracks_suffix_paths() {
    let cases = vec![
        (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
        (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
    ];
    let default = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let reachability = analyze_switch_tail_paths(&cases, &default);

    assert_eq!(
        reachability.case_tail_paths,
        vec![TailPathKind::FallsThrough, TailPathKind::FallsThrough]
    );
    assert_eq!(reachability.default_tail_path, Some(TailPathKind::FallsThrough));
}

/// Verifies that `build_switch_cfg` produces correct case/default entry indices and
/// that successor edges reflect break vs fallthrough semantics.
#[test]
fn test_build_switch_cfg_tracks_case_successors() {
    let cases = vec![
        (vec![Expr::int_lit(1)], Vec::new()),
        (
            vec![Expr::int_lit(2)],
            vec![Stmt::new(StmtKind::Break(1), Span::dummy())],
        ),
    ];
    let default = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let cfg = build_switch_cfg(&cases, &default);

    assert_eq!(cfg.case_entries, vec![0, 1]);
    assert_eq!(cfg.default_entry, Some(2));
    assert_eq!(
        cfg.blocks,
        vec![
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Block(1)],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Breaks],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::FallsThrough],
            },
        ]
    );
}

/// Verifies that `classify_switch_cfg_paths` propagates FallsThrough along the chain
/// and that the default block's final successor is also FallsThrough.
#[test]
fn test_classify_switch_cfg_paths_follows_fallthrough_chain() {
    let cases = vec![
        (vec![Expr::int_lit(1)], Vec::new()),
        (vec![Expr::int_lit(2)], Vec::new()),
    ];
    let default = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let cfg = build_switch_cfg(&cases, &default);

    assert_eq!(
        classify_switch_cfg_paths(&cfg),
        vec![
            BasicBlockSuccessor::FallsThrough,
            BasicBlockSuccessor::FallsThrough,
        ]
    );
    assert_eq!(
        classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(cfg.default_entry.unwrap())),
        BasicBlockSuccessor::FallsThrough
    );
}

/// Verifies that `collect_reachable_cfg_blocks` marks only the first case block
/// reachable when the second case terminates with a break and the default body is unreachable.
#[test]
fn test_collect_reachable_switch_cfg_blocks_follows_only_reachable_suffix() {
    let cases = vec![
        (
            vec![Expr::int_lit(1)],
            vec![Stmt::new(StmtKind::Break(1), Span::dummy())],
        ),
        (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
    ];
    let default = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let cfg = build_switch_cfg(&cases, &default);
    let reachable = collect_reachable_cfg_blocks(&cfg.blocks, &[0]);

    assert_eq!(reachable, vec![true, false, false]);
}

/// Verifies that the first case is classified Breaks (has echo then break) and the
/// second case FallsThrough (no break) when analyzing tail reachability.
#[test]
fn test_switch_tail_reachability_tracks_break_and_fallthrough_paths() {
    let cases = vec![
        (
            vec![Expr::int_lit(1)],
            vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
        ),
        (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
    ];
    let default = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let reachability = analyze_switch_tail_paths(&cases, &default);

    assert_eq!(
        reachability.case_tail_paths,
        vec![TailPathKind::Breaks, TailPathKind::FallsThrough]
    );
    assert_eq!(reachability.default_tail_path, Some(TailPathKind::FallsThrough));
}

/// Verifies that when a case body contains an if/else with a break in one branch
/// and a return in the other, `analyze_switch_tail_paths` classifies the tail as Unknown.
#[test]
fn test_switch_tail_reachability_marks_mixed_break_paths_unknown() {
    let cases = vec![(
        vec![Expr::int_lit(1)],
        vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::new(StmtKind::Break(1), Span::dummy())],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(7))),
                    Span::dummy(),
                )]),
            },
            Span::dummy(),
        )],
    )];

    let reachability = analyze_switch_tail_paths(&cases, &None);

    assert_eq!(reachability.case_tail_paths, vec![TailPathKind::Unknown]);
    assert_eq!(reachability.default_tail_path, None);
}

/// Verifies that the then body exits, the first elseif sinks (fallthrough), the second
/// elseif does not sink (has return), no explicit else body, and implicit else sinks.
#[test]
fn test_if_tail_reachability_tracks_fallthrough_and_implicit_else() {
    let elseif_clauses = vec![
        (
            Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
            vec![Stmt::new(StmtKind::Return(Some(Expr::int_lit(7))), Span::dummy())],
        ),
        (
            Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
            vec![Stmt::echo(Expr::int_lit(8))],
        ),
    ];

    let reachability = analyze_if_tail_paths(
        &[Stmt::new(StmtKind::Return(Some(Expr::int_lit(1))), Span::dummy())],
        &elseif_clauses,
        &None,
    );

    assert!(!reachability.then_sinks_tail);
    assert_eq!(reachability.elseif_sinks_tail, vec![false, true]);
    assert!(!reachability.else_sinks_tail);
    assert!(reachability.implicit_else_sinks_tail);
}

/// Verifies that `build_if_cfg` produces correct entry indices for then/elseif/else
/// and that each block's successors reflect exits, falls-through, or branch edges.
#[test]
fn test_build_if_cfg_tracks_condition_and_body_successors() {
    let elseif_clauses = vec![(
        Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
        vec![Stmt::new(StmtKind::Return(Some(Expr::int_lit(7))), Span::dummy())],
    )];
    let else_body = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let cfg = build_if_cfg(
        &[Stmt::echo(Expr::int_lit(1))],
        &elseif_clauses,
        &else_body,
    );

    assert_eq!(cfg.body_entries, vec![2, 3]);
    assert_eq!(cfg.else_entry, Some(4));
    assert_eq!(cfg.implicit_else_successor, BasicBlockSuccessor::Unknown);
    assert_eq!(
        cfg.blocks,
        vec![
            BasicBlock {
                successors: vec![
                    BasicBlockSuccessor::Block(2),
                    BasicBlockSuccessor::Block(1),
                ],
            },
            BasicBlock {
                successors: vec![
                    BasicBlockSuccessor::Block(3),
                    BasicBlockSuccessor::Block(4),
                ],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::FallsThrough],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Exits],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::FallsThrough],
            },
        ]
    );
}

/// Verifies that `classify_if_cfg_paths` marks the then body as Exits (has return)
/// and the elseif body as FallsThrough.
#[test]
fn test_classify_if_cfg_paths_tracks_branch_bodies() {
    let elseif_clauses = vec![(
        Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
        vec![Stmt::echo(Expr::int_lit(8))],
    )];

    let cfg = build_if_cfg(
        &[Stmt::new(StmtKind::Return(Some(Expr::int_lit(1))), Span::dummy())],
        &elseif_clauses,
        &None,
    );

    assert_eq!(
        classify_if_cfg_paths(&cfg),
        vec![BasicBlockSuccessor::Exits, BasicBlockSuccessor::FallsThrough]
    );
}

/// Verifies that an ifdef with an else body that exits means the then body does not
/// sink and neither the else nor implicit else sinks.
#[test]
fn test_ifdef_tail_reachability_tracks_implicit_else() {
    let reachability = analyze_ifdef_tail_paths(
        &[Stmt::echo(Expr::int_lit(7))],
        &Some(vec![Stmt::new(
            StmtKind::Return(Some(Expr::int_lit(8))),
            Span::dummy(),
        )]),
    );

    assert!(reachability.then_sinks_tail);
    assert!(!reachability.else_sinks_tail);
    assert!(!reachability.implicit_else_sinks_tail);
}

/// Verifies that a try/catch/finally with all bodies falling through sets
/// `can_sink_into_finally` to true, but adding a catch with a return sets it to false.
#[test]
fn test_try_tail_reachability_prefers_finally_only_when_safe() {
    let safe_try = vec![Stmt::echo(Expr::int_lit(7))];
    let safe_finally = Some(vec![Stmt::echo(Expr::int_lit(8))]);

    let safe = analyze_try_tail_paths(&safe_try, &Vec::new(), &safe_finally);
    assert_eq!(safe.try_tail_path, TailPathKind::FallsThrough);
    assert_eq!(safe.finally_tail_path, Some(TailPathKind::FallsThrough));
    assert!(safe.can_sink_into_finally);

    let catch_body = vec![crate::parser::ast::CatchClause {
        exception_types: vec!["Exception".into()],
        variable: Some("e".into()),
        body: vec![Stmt::new(StmtKind::Return(Some(Expr::int_lit(9))), Span::dummy())],
    }];
    let with_catch = analyze_try_tail_paths(&safe_try, &catch_body, &safe_finally);
    assert_eq!(with_catch.try_tail_path, TailPathKind::FallsThrough);
    assert_eq!(with_catch.catch_tail_paths, vec![TailPathKind::NoTail]);
    assert_eq!(with_catch.finally_tail_path, Some(TailPathKind::FallsThrough));
    assert!(!with_catch.can_sink_into_finally);
}

/// Verifies that `build_try_cfg` assigns correct entry indices to try/catch/finally
/// and that successor edges reflect break, exit, or fallthrough semantics.
#[test]
fn test_build_try_cfg_tracks_try_catch_and_finally_successors() {
    let catches = vec![
        crate::parser::ast::CatchClause {
            exception_types: vec!["Exception".into()],
            variable: Some("e".into()),
            body: vec![Stmt::new(StmtKind::Break(1), Span::dummy())],
        },
        crate::parser::ast::CatchClause {
            exception_types: vec!["RuntimeException".into()],
            variable: Some("e".into()),
            body: vec![Stmt::new(
                StmtKind::Return(Some(Expr::int_lit(9))),
                Span::dummy(),
            )],
        },
    ];
    let finally_body = Some(vec![Stmt::echo(Expr::int_lit(10))]);

    let cfg = build_try_cfg(&[Stmt::echo(Expr::int_lit(7))], &catches, &finally_body);

    assert_eq!(cfg.try_entry, 0);
    assert_eq!(cfg.catch_entries, vec![1, 2]);
    assert_eq!(cfg.finally_entry, Some(3));
    assert_eq!(
        cfg.blocks,
        vec![
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Block(3)],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Breaks],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Exits],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::FallsThrough],
            },
        ]
    );
}

/// Verifies that `classify_try_cfg_paths` classifies try/catch bodies as FallsThrough
/// and catch-with-return as Exits, and that the finally block also FallsThrough.
#[test]
fn test_classify_try_cfg_paths_tracks_try_and_catch_bodies() {
    let catches = vec![
        crate::parser::ast::CatchClause {
            exception_types: vec!["Exception".into()],
            variable: Some("e".into()),
            body: vec![Stmt::echo(Expr::int_lit(8))],
        },
        crate::parser::ast::CatchClause {
            exception_types: vec!["RuntimeException".into()],
            variable: Some("e".into()),
            body: vec![Stmt::new(
                StmtKind::Return(Some(Expr::int_lit(9))),
                Span::dummy(),
            )],
        },
    ];
    let finally_body = Some(vec![Stmt::echo(Expr::int_lit(10))]);

    let cfg = build_try_cfg(&[Stmt::echo(Expr::int_lit(7))], &catches, &finally_body);

    assert_eq!(
        classify_try_cfg_paths(&cfg),
        vec![
            BasicBlockSuccessor::FallsThrough,
            BasicBlockSuccessor::FallsThrough,
            BasicBlockSuccessor::Exits,
        ]
    );
    assert_eq!(
        classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(cfg.finally_entry.unwrap())),
        BasicBlockSuccessor::FallsThrough
    );
}

/// Verifies that when no finally is present, the first catch FallsThrough and the
/// second catch Exits; `can_sink_into_finally` is false.
#[test]
fn test_try_tail_reachability_tracks_catch_fallthrough_without_finally() {
    let catches = vec![
        crate::parser::ast::CatchClause {
            exception_types: vec!["Exception".into()],
            variable: Some("e".into()),
            body: vec![Stmt::echo(Expr::int_lit(8))],
        },
        crate::parser::ast::CatchClause {
            exception_types: vec!["RuntimeException".into()],
            variable: Some("e".into()),
            body: vec![Stmt::new(StmtKind::Return(Some(Expr::int_lit(9))), Span::dummy())],
        },
    ];

    let reachability = analyze_try_tail_paths(
        &[Stmt::echo(Expr::int_lit(7))],
        &catches,
        &None,
    );

    assert_eq!(reachability.try_tail_path, TailPathKind::FallsThrough);
    assert_eq!(
        reachability.catch_tail_paths,
        vec![TailPathKind::FallsThrough, TailPathKind::NoTail]
    );
    assert_eq!(reachability.finally_tail_path, None);
    assert!(!reachability.can_sink_into_finally);
}
