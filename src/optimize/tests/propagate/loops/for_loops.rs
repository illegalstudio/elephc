//! Purpose:
//! Regression tests for optimizer propagate loops for_loops behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
fn test_propagate_constants_uses_known_switch_subject_for_merge() {
    let program = vec![
        Stmt::assign("mode", Expr::int_lit(1)),
        Stmt::new(
            StmtKind::Switch {
                subject: Expr::var("mode"),
                cases: vec![(
                    vec![Expr::int_lit(1)],
                    vec![
                        Stmt::assign("base", Expr::int_lit(2)),
                        Stmt::new(StmtKind::Break(1), Span::dummy()),
                    ],
                )],
                default: Some(vec![Stmt::assign("base", Expr::int_lit(9))]),
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_for_loop() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("i", Expr::int_lit(0)))),
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(3))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![Stmt::echo(Expr::var("i"))],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_tracks_assignment_through_for_infinite_break() {
    let program = vec![
        Stmt::new(
            StmtKind::For {
                init: None,
                condition: None,
                update: None,
                body: vec![
                    Stmt::assign("base", Expr::int_lit(2)),
                    Stmt::new(StmtKind::Break(1), Span::dummy()),
                ],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_preserves_for_init_when_condition_is_false() {
    let program = vec![
        Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("base", Expr::int_lit(2)))),
                condition: Some(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                update: Some(Box::new(Stmt::assign("base", Expr::int_lit(9)))),
                body: vec![Stmt::assign("base", Expr::int_lit(9))],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_tracks_stable_for_init_assignments() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::assign("i", Expr::int_lit(0)),
        Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("exp", Expr::int_lit(3)))),
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(2))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![Stmt::echo(Expr::binop(
                    Expr::var("base"),
                    BinOp::Pow,
                    Expr::var("exp"),
                ))],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::var("exp")),
    ];

    let propagated = propagate_constants(program);

    let StmtKind::For { body, .. } = &propagated[2].kind else {
        panic!("expected for");
    };

    assert_eq!(
        body[0],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
    assert_eq!(
        propagated[3],
        Stmt::echo(Expr::new(ExprKind::IntLiteral(3), Span::dummy()))
    );
}
