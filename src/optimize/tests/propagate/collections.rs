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

/// Tests that constant propagation tracks scalar values unpacked from a `list()` assignment.
/// The `base` and `exp` variables are initialized from a fixed array literal `[2, 3]`.
/// After propagation, the subsequent `echo $base ** $exp` expression is folded to `8.0`.
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

/// Tests that constant propagation tracks scalar values accessed from a numeric-indexed array literal.
/// `$base` is assigned `&$arr[0]` where `$arr = [2, 9]`; after propagation `$base = 2`.
/// The subsequent `echo $base ** 3` is folded to `8.0`.
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

/// Tests that constant propagation tracks scalar values accessed from an associative array literal.
/// `$base` is assigned `&$arr["left"]` where `$arr = ["left" => 2, "right" => 9]`; after propagation `$base = 2`.
/// The subsequent `echo $base ** 3` is folded to `8.0`.
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

/// Tests that constant propagation preserves scalar values that are not targeted by `unset()`.
/// `$base = 2` and `$tmp = 9`; `unset($tmp)` invalidates `$tmp` but `$base` remains a constant.
/// After propagation, `echo $base ** 3` is folded to `8.0` while `echo $tmp` is unaffected.
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

/// Tests that `unset()` with multiple targets correctly invalidates all named variables.
/// `$base = 2`, `$tmp = 9`, `$other = 10`; `unset($tmp, $other)` invalidates `$tmp` and `$other`.
/// After propagation, `echo $tmp` remains a variable (not folded) and `echo $base ** 3` is `8.0`.
#[test]
fn test_propagate_constants_invalidates_multiple_unset_targets() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::assign("tmp", Expr::int_lit(9)),
        Stmt::assign("other", Expr::int_lit(10)),
        Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::FunctionCall {
                    name: "unset".into(),
                    args: vec![Expr::var("tmp"), Expr::var("other")],
                },
                Span::dummy(),
            )),
            Span::dummy(),
        ),
        Stmt::echo(Expr::var("tmp")),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(propagated[4], Stmt::echo(Expr::var("tmp")));
    assert_eq!(
        propagated[5],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}
