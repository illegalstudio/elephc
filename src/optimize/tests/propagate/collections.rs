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

/// Builds an `ArrayAccess` expression.
fn access(array: Expr, index: Expr) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(array),
            index: Box::new(index),
        },
        Span::dummy(),
    )
}

/// Builds an indexed array literal of integer elements.
fn int_array(values: &[i64]) -> Expr {
    Expr::new(
        ExprKind::ArrayLiteral(values.iter().map(|value| Expr::int_lit(*value)).collect()),
        Span::dummy(),
    )
}

/// `$a = [1, 2, 3]; echo $a[1];` folds the element read to `2`.
#[test]
fn test_array_fact_folds_constant_index_access() {
    let program = vec![
        Stmt::assign("a", int_array(&[1, 2, 3])),
        Stmt::echo(access(Expr::var("a"), Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(propagated[1], Stmt::echo(Expr::int_lit(2)));
}

/// An associative literal fact folds string-key reads.
#[test]
fn test_assoc_array_fact_folds_key_access() {
    let program = vec![
        Stmt::assign(
            "a",
            Expr::new(
                ExprKind::ArrayLiteralAssoc(vec![(
                    Expr::string_lit("k"),
                    Expr::int_lit(7),
                )]),
                Span::dummy(),
            ),
        ),
        Stmt::echo(access(Expr::var("a"), Expr::string_lit("k"))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(propagated[1], Stmt::echo(Expr::int_lit(7)));
}

/// `$b = $a` snapshots the fact (COW value semantics): a later element write
/// through `$b` kills only `$b`'s fact, and `$a` keeps folding.
#[test]
fn test_array_fact_copy_respects_cow() {
    let program = vec![
        Stmt::assign("a", int_array(&[1, 2, 3])),
        Stmt::assign("b", Expr::var("a")),
        Stmt::new(
            StmtKind::ArrayAssign {
                array: "b".to_string(),
                index: Expr::int_lit(0),
                value: Expr::int_lit(9),
            },
            Span::dummy(),
        ),
        Stmt::echo(access(Expr::var("a"), Expr::int_lit(1))),
        Stmt::echo(access(Expr::var("b"), Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[3],
        Stmt::echo(Expr::int_lit(2)),
        "writes through $b must not kill $a's fact (COW)"
    );
    assert_eq!(
        propagated[4],
        Stmt::echo(access(Expr::var("b"), Expr::int_lit(1))),
        "$b's own fact dies at the element write"
    );
}

/// A by-ref exposure (`sort($a)`) kills the array fact.
#[test]
fn test_array_fact_dies_at_by_ref_builtin() {
    let program = vec![
        Stmt::assign("a", int_array(&[3, 1, 2])),
        Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::from("sort"),
                    args: vec![Expr::var("a")],
                },
                Span::dummy(),
            )),
            Span::dummy(),
        ),
        Stmt::echo(access(Expr::var("a"), Expr::int_lit(0))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(access(Expr::var("a"), Expr::int_lit(0))),
        "sort($a) rewrites the array"
    );
}

/// An out-of-range read keeps the runtime access (and its warning).
#[test]
fn test_array_fact_out_of_range_access_stays() {
    let program = vec![
        Stmt::assign("a", int_array(&[1, 2, 3])),
        Stmt::echo(access(Expr::var("a"), Expr::int_lit(9))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(access(Expr::var("a"), Expr::int_lit(9))),
        "out-of-range reads keep their runtime warning"
    );
}

/// Oversized literals carry no fact (environment size cap).
#[test]
fn test_array_fact_size_cap() {
    let values: Vec<i64> = (0..65).collect();
    let program = vec![
        Stmt::assign("a", int_array(&values)),
        Stmt::echo(access(Expr::var("a"), Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[1],
        Stmt::echo(access(Expr::var("a"), Expr::int_lit(1))),
        "a 65-element literal is over the fact cap"
    );
}

/// A by-value pass to a user function without by-ref params keeps the fact.
#[test]
fn test_array_fact_survives_by_value_user_call() {
    let program = vec![
        Stmt::new(
            StmtKind::FunctionDecl {
                name: "reader".to_string(),
                params: vec![("arr".to_string(), None, None, false)],
                param_attributes: Vec::new(),
                variadic: None,
                variadic_by_ref: false,
                variadic_type: None,
                return_type: None,
                by_ref_return: false,
                body: vec![Stmt::echo(Expr::string_lit("r"))],
            },
            Span::dummy(),
        ),
        Stmt::assign("a", int_array(&[1, 2, 3])),
        Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::from("reader"),
                    args: vec![Expr::var("a")],
                },
                Span::dummy(),
            )),
            Span::dummy(),
        ),
        Stmt::echo(access(Expr::var("a"), Expr::int_lit(2))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[3],
        Stmt::echo(Expr::int_lit(3)),
        "a by-value pass copies the array (COW)"
    );
}

/// `list($x, $y) = $a` with an array fact extracts element facts.
#[test]
fn test_list_unpack_from_array_fact() {
    let program = vec![
        Stmt::assign("a", int_array(&[4, 5])),
        Stmt::new(
            StmtKind::ListUnpack {
                vars: vec!["x".to_string(), "y".to_string()],
                value: Expr::var("a"),
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::var("y"))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(propagated[2], Stmt::echo(Expr::int_lit(9)));
}
