//! Purpose:
//! Regression tests for optimizer propagate control_paths behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Tests that constant propagation merges identical assignments across all switch cases.
///
/// When every switch case assigns the same constant to a variable, and the variable
/// is subsequently used in a foldable expression, the optimizer must compute the
/// final constant result directly from any case (since all are equivalent).
/// This test verifies `2 ^ 3 = 8` is folded correctly from `base = 2` in switch.
#[test]
fn test_propagate_constants_merges_identical_switch_assignments() {
    let program = vec![
        Stmt::new(
            StmtKind::Switch {
                subject: Expr::var("flag"),
                cases: vec![(
                    vec![Expr::int_lit(1)],
                    vec![
                        Stmt::assign("base", Expr::int_lit(2)),
                        Stmt::new(StmtKind::Break(1), Span::dummy()),
                    ],
                )],
                default: Some(vec![Stmt::assign("base", Expr::int_lit(2))]),
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::int_lit(8))
    );
}

/// Tests that constant propagation merges identical assignments across try and catch blocks.
///
/// When both the try body and every catch clause assign the same constant to a variable,
/// the optimizer must treat the variable as constant regardless of which path executes.
/// This test verifies `2 ^ 3 = 8` is folded correctly when `base = 2` appears in both
/// the try and catch branches.
#[test]
fn test_propagate_constants_merges_identical_try_catch_assignments() {
    let program = vec![
        Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::assign("base", Expr::int_lit(2))],
                catches: vec![crate::parser::ast::CatchClause {
                    exception_types: vec![Name::from("Exception")],
                    variable: Some("e".to_string()),
                    body: vec![Stmt::assign("base", Expr::int_lit(2))],
                }],
                finally_body: None,
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::int_lit(8))
    );
}

/// Tests that constant propagation ignores catch assignments when the try body cannot throw.
///
/// A non-throwing try body means the catch is unreachable. The optimizer must not
/// propagate the catch's assignment when computing the variable's value, even if
/// the catch assigns a different constant. This test verifies `base` remains `2`
/// from the try body, not `9` from the catch, yielding `2 ^ 3 = 8`.
#[test]
fn test_propagate_constants_ignores_unreachable_catch_after_non_throwing_try() {
    let program = vec![
        Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::assign("base", Expr::int_lit(2))],
                catches: vec![crate::parser::ast::CatchClause {
                    exception_types: vec![Name::from("Exception")],
                    variable: Some("e".to_string()),
                    body: vec![Stmt::assign("base", Expr::int_lit(9))],
                }],
                finally_body: None,
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(Expr::int_lit(8))
    );
}

/// Tests that a switch body entered by falling through keeps the writes of the body before it.
///
/// `case 1: $k = 10;` falls into `case 2: echo $k + 1;`, so `$k` is not the `0` it held before
/// the switch there, and the echo must not fold to `1`.
#[test]
fn test_propagate_constants_keeps_fallthrough_writes_in_the_next_switch_body() {
    let echo_k_plus_one = Stmt::echo(Expr::binop(Expr::var("k"), BinOp::Add, Expr::int_lit(1)));
    let program = vec![
        Stmt::assign("k", Expr::int_lit(0)),
        Stmt::new(
            StmtKind::Switch {
                subject: Expr::var("flag"),
                cases: vec![
                    (vec![Expr::int_lit(1)], vec![Stmt::assign("k", Expr::int_lit(10))]),
                    (
                        vec![Expr::int_lit(2)],
                        vec![echo_k_plus_one.clone(), Stmt::new(StmtKind::Break(1), Span::dummy())],
                    ),
                ],
                default: None,
            },
            Span::dummy(),
        ),
    ];

    let propagated = propagate_constants(program);

    let StmtKind::Switch { cases, .. } = &propagated[1].kind else {
        panic!("expected the switch to survive propagation");
    };
    assert_eq!(cases[1].1[0], echo_k_plus_one);
}
