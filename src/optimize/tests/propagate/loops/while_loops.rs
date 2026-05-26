//! Purpose:
//! Regression tests for optimizer propagate loops while_loops behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
// Tests that constant propagation preserves scalar values across a while loop with a
// false condition. The body writes to `base`, but since the loop never executes,
// the initial assignment `base = 2` should be the only value propagated.
fn test_propagate_constants_preserves_scalar_across_while_false_body_writes() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::new(
            StmtKind::While {
                condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                body: vec![Stmt::assign("base", Expr::int_lit(9))],
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
// Tests that constant propagation correctly tracks an assignment written in a
// do-while body when the condition is false. Since the body executes once before
// the condition is evaluated, `base = 2` should be propagated to the echo.
fn test_propagate_constants_tracks_assignment_through_do_while_false() {
    let program = vec![
        Stmt::new(
            StmtKind::DoWhile {
                body: vec![Stmt::assign("base", Expr::int_lit(2))],
                condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
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
// Tests that constant propagation tracks an assignment written in a while loop body
// that executes once before a break. The loop condition is true, `base = 2` is
// assigned, then `break 1` exits — so `base = 2` should be propagated.
fn test_propagate_constants_tracks_assignment_through_while_true_break() {
    let program = vec![
        Stmt::new(
            StmtKind::While {
                condition: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
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
// Tests that constant propagation merges branches when both arms of an if statement
// inside a while loop assign the same value to `base` before breaking. The value
// should be propagated since both paths through the if assign to `base`.
fn test_propagate_constants_merges_branch_breaks_through_while_true() {
    let program = vec![
        Stmt::new(
            StmtKind::While {
                condition: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                body: vec![Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![
                            Stmt::assign("base", Expr::int_lit(2)),
                            Stmt::new(StmtKind::Break(1), Span::dummy()),
                        ],
                        elseif_clauses: Vec::new(),
                        else_body: Some(vec![
                            Stmt::assign("base", Expr::int_lit(2)),
                            Stmt::new(StmtKind::Break(1), Span::dummy()),
                        ]),
                    },
                    Span::dummy(),
                )],
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
// Tests that constant propagation tracks an assignment written in a do-while body
// when a continue statement precedes the condition evaluation. The body executes
// once, assigns `base = 2`, then continues — so `base = 2` should be propagated.
fn test_propagate_constants_tracks_continue_through_do_while_false() {
    let program = vec![
        Stmt::new(
            StmtKind::DoWhile {
                body: vec![
                    Stmt::assign("base", Expr::int_lit(2)),
                    Stmt::new(StmtKind::Continue(1), Span::dummy()),
                ],
                condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
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
// Tests that constant propagation preserves unmodified scalar values inside a while
// loop body. The variable `base` is assigned before the loop and never modified
// inside the loop, so the echo statement should be folded to a literal `8.0`.
fn test_propagate_constants_preserves_unmodified_scalar_inside_while_loop_body() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::assign("i", Expr::int_lit(0)),
        Stmt::new(
            StmtKind::While {
                condition: Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(2)),
                body: vec![
                    Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
                    Stmt::new(
                        StmtKind::ExprStmt(Expr::new(
                            ExprKind::PostIncrement("i".to_string()),
                            Span::dummy(),
                        )),
                        Span::dummy(),
                    ),
                ],
            },
            Span::dummy(),
        ),
    ];

    let propagated = propagate_constants(program);

    let StmtKind::While { body, .. } = &propagated[2].kind else {
        panic!("expected while");
    };

    assert_eq!(
        body[0],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}
