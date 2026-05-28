//! Purpose:
//! Regression tests for optimizer propagate straight_line behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Tests that integer literals assigned to sequential local variables are propagated
/// through straight-line code (no control flow). The expression `x ** y` is folded to
/// `8.0` because both `x = 2` and `y = 3` are known constant values at the echo site.
#[test]
fn test_propagate_constants_through_straight_line_locals() {
    let program = vec![
        Stmt::assign("x", Expr::int_lit(2)),
        Stmt::assign("y", Expr::int_lit(3)),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Pow, Expr::var("y"))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated,
        vec![
            Stmt::assign("x", Expr::int_lit(2)),
            Stmt::assign("y", Expr::int_lit(3)),
            Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy())),
        ]
    );
}

/// Tests that when both branches of an If statement assign the same constant value
/// to a variable, the variable is treated as a uniform constant after the If.
/// The second statement (`echo base ** 3`) should fold to `8.0` because `base` is
/// known to be `2` regardless of which branch executes.
#[test]
fn test_propagate_constants_merges_identical_if_assignments() {
    let program = vec![
        Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::assign("base", Expr::int_lit(2))],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::assign("base", Expr::int_lit(2))]),
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

/// Tests that reassignment of a variable to a non-scalar expression (a function call
/// with side effects) invalidates constant propagation for that variable.
/// The second assignment `x = strlen("abc")` is not a scalar literal, so `x` cannot
/// be propagated out of the echo; the expression remains as `x + 1` rather than folding.
#[test]
fn test_propagate_constants_invalidates_non_scalar_reassignment() {
    let program = vec![
        Stmt::assign("x", Expr::int_lit(2)),
        Stmt::assign(
            "x",
            Expr::new(
                ExprKind::FunctionCall {
                    name: Name::from("strlen"),
                    args: vec![Expr::string_lit("abc")],
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1)))
    );
}

/// Tests that when both branches of a ternary expression are the same constant,
/// the resulting assignment is treated as a uniform constant. `base = flag ? 2 : 2`
/// always yields `2`, so `base ** 3` folds to `8.0`.
#[test]
fn test_propagate_constants_tracks_uniform_ternary_assignment() {
    let program = vec![
        Stmt::assign(
            "base",
            Expr::new(
                ExprKind::Ternary {
                    condition: Box::new(Expr::var("flag")),
                    then_expr: Box::new(Expr::int_lit(2)),
                    else_expr: Box::new(Expr::int_lit(2)),
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

/// Tests that when all arms of a match expression and its default clause yield the
/// same constant value, the resulting assignment is treated as a uniform constant.
/// `base = match(flag) { 1 => 2, default => 2 }` always yields `2`, so `base ** 3`
/// folds to `8.0`.
#[test]
fn test_propagate_constants_tracks_uniform_match_assignment() {
    let program = vec![
        Stmt::assign(
            "base",
            Expr::new(
                ExprKind::Match {
                    subject: Box::new(Expr::var("flag")),
                    arms: vec![(vec![Expr::int_lit(1)], Expr::int_lit(2))],
                    default: Some(Box::new(Expr::int_lit(2))),
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

/// Tests that when a match expression's subject is a known constant, the optimizer
/// can determine which arm fires and propagate the resulting constant. Here the
/// subject `mode = 1` means the first arm matches, so `base = 2` and `base ** 3`
/// folds to `8.0`.
#[test]
fn test_propagate_constants_tracks_known_match_assignment() {
    let program = vec![
        Stmt::assign("mode", Expr::int_lit(1)),
        Stmt::assign(
            "base",
            Expr::new(
                ExprKind::Match {
                    subject: Box::new(Expr::var("mode")),
                    arms: vec![(vec![Expr::int_lit(1)], Expr::int_lit(2))],
                    default: Some(Box::new(Expr::int_lit(9))),
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(propagated[1], Stmt::assign("base", Expr::int_lit(2)));
    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}
