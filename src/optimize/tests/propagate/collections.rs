//! Purpose:
//! Regression tests for optimizer propagate collections behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
fn test_propagate_constants_tracks_scalar_list_unpack() {
    let program = vec![
        Stmt::new(
            StmtKind::ListUnpack {
                vars: vec!["base".to_string(), "exp".to_string()],
                value: Expr::new(
                    ExprKind::ArrayLiteral(vec![Expr::int_lit(2), Expr::int_lit(3)]),
                    Span::dummy(),
                ),
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::var("exp"))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_tracks_scalar_array_literal_access() {
    let program = vec![
        Stmt::assign(
            "base",
            Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::new(
                        ExprKind::ArrayLiteral(vec![Expr::int_lit(2), Expr::int_lit(9)]),
                        Span::dummy(),
                    )),
                    index: Box::new(Expr::int_lit(0)),
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(propagated[0], Stmt::assign("base", Expr::int_lit(2)));
    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_tracks_scalar_assoc_array_literal_access() {
    let program = vec![
        Stmt::assign(
            "base",
            Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::new(
                        ExprKind::ArrayLiteralAssoc(vec![
                            (Expr::string_lit("left"), Expr::int_lit(2)),
                            (Expr::string_lit("right"), Expr::int_lit(9)),
                        ]),
                        Span::dummy(),
                    )),
                    index: Box::new(Expr::string_lit("left")),
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(propagated[0], Stmt::assign("base", Expr::int_lit(2)));
    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_unset() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::assign("tmp", Expr::int_lit(9)),
        Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::FunctionCall {
                    name: "unset".into(),
                    args: vec![Expr::var("tmp")],
                },
                Span::dummy(),
            )),
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
