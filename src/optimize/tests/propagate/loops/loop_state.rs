//! Purpose:
//! Regression tests for optimizer propagate loops loop_state behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

// Verifies that a scalar variable assigned before a for loop is not propagated
// through the loop when the loop contains a `switch` that could theoretically
// skip iterations. The variable `base` is assigned 2 outside the loop and
// never modified inside the loop body (which contains a switch on the loop
// index). After constant propagation, `base ^ 3` must be folded to `8.0`.
#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_loop_with_switch() {
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
                body: vec![Stmt::new(
                    StmtKind::Switch {
                        subject: Expr::var("i"),
                        cases: vec![(
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::var("i")),
                                Stmt::new(StmtKind::Break(1), Span::dummy()),
                            ],
                        )],
                        default: Some(vec![Stmt::echo(Expr::var("i"))]),
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
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

// Verifies that a scalar variable assigned before a for loop is not propagated
// through the loop when the loop body contains a `try` statement. The variable
// `base` is assigned 2 outside the loop and never modified inside the loop body
// (which contains a try/catch on the loop index). After constant propagation,
// `base ^ 3` must be folded to `8.0`.
#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_loop_with_try() {
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
                body: vec![Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::echo(Expr::var("i"))],
                        catches: vec![crate::parser::ast::CatchClause {
                            exception_types: vec![Name::from("Exception")],
                            variable: Some("e".to_string()),
                            body: vec![Stmt::echo(Expr::int_lit(9))],
                        }],
                        finally_body: Some(vec![]),
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
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

// Verifies that a scalar variable assigned before nested loops is not propagated
// through the loops when the inner loop contains local statements. The variable
// `base` is assigned 2 before the outer for loop and never modified inside either
// loop (which modifies loop indices `i` and `j`). After constant propagation,
// `base ^ 3` must be folded to `8.0`.
#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_nested_loops() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::assign("i", Expr::int_lit(0)),
        Stmt::new(
            StmtKind::For {
                init: None,
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(2))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![
                    Stmt::assign("j", Expr::int_lit(0)),
                    Stmt::new(
                        StmtKind::While {
                            condition: Expr::binop(
                                Expr::var("j"),
                                BinOp::Lt,
                                Expr::int_lit(2),
                            ),
                            body: vec![
                                Stmt::echo(Expr::var("j")),
                                Stmt::new(
                                    StmtKind::ExprStmt(Expr::new(
                                        ExprKind::PostIncrement("j".to_string()),
                                        Span::dummy(),
                                    )),
                                    Span::dummy(),
                                ),
                            ],
                        },
                        Span::dummy(),
                    ),
                ],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[3],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

// Verifies that a scalar variable assigned before a for loop is not propagated
// through the loop when the loop body contains array writes on a local variable
// (`items`). The variable `base` is assigned 2 outside the loop and never modified.
// The loop body performs `ArrayPush` and `ArrayAssign` on `items`, which must not
// be treated as modifications of `base`. After constant propagation, `base ^ 3`
// must be folded to `8.0`.
#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_loop_local_array_writes() {
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
                body: vec![
                    Stmt::new(
                        StmtKind::ArrayPush {
                            array: "items".to_string(),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::ArrayAssign {
                            array: "items".to_string(),
                            index: Expr::int_lit(0),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                ],
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

// Verifies that a scalar variable assigned before a for loop is not propagated
// through the loop when the loop body contains property writes on an object
// (`box`). The variable `base` is assigned 2 outside the loop and never modified.
// The loop body performs `PropertyAssign`, `PropertyArrayPush`, and
// `PropertyArrayAssign` on `$box->...`, which must not be treated as modifications
// of `base`. After constant propagation, `base ^ 3` must be folded to `8.0`.
#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_loop_property_writes() {
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
                body: vec![
                    Stmt::new(
                        StmtKind::PropertyAssign {
                            object: Box::new(Expr::var("box")),
                            property: "last".to_string(),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::PropertyArrayPush {
                            object: Box::new(Expr::var("box")),
                            property: "items".to_string(),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::PropertyArrayAssign {
                            object: Box::new(Expr::var("box")),
                            property: "items".to_string(),
                            index: Expr::int_lit(0),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                ],
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
