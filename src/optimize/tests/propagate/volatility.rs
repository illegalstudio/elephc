//! Purpose:
//! Regression tests for the reference-volatility ledger: names exposed through
//! references (`global`, `static`, by-ref captures, by-ref foreach, ref-assign
//! lvalue roots, superglobals) must never carry propagated constants.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Behavioral fixtures assert that echoes stay unfolded; ledger fixtures
//!   inspect `is_reference_volatile` directly because the mark only becomes
//!   behaviorally observable once array facts land.

use super::*;
use crate::optimize::propagate::is_reference_volatile;

/// Builds a minimal closure expression capturing `captures` (by value) and
/// `capture_refs` (by reference) with an empty body.
fn closure_with_captures(captures: Vec<String>, capture_refs: Vec<String>) -> Expr {
    Expr::new(
        ExprKind::Closure {
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            body: Vec::new(),
            is_arrow: false,
            is_static: false,
            by_ref_return: false,
            captures,
            capture_refs,
        },
        Span::dummy(),
    )
}

/// `static $x = 0; $x = 5; echo $x + 1;` must not fold the echo: a recursive
/// call can rewrite the shared static cell without a visible local write.
#[test]
fn test_static_var_blocks_propagation() {
    let program = vec![
        Stmt::new(
            StmtKind::StaticVar {
                name: "x".to_string(),
                init: Expr::int_lit(0),
            },
            Span::dummy(),
        ),
        Stmt::assign("x", Expr::int_lit(5)),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
        "a static-bound local must never carry a propagated constant"
    );
}

/// `global $x; $x = 5; echo $x + 1;` must not fold the echo: any callee can
/// rewrite the aliased global storage without a visible local write.
#[test]
fn test_global_decl_blocks_propagation() {
    let program = vec![
        Stmt::new(
            StmtKind::Global {
                vars: vec!["x".to_string()],
            },
            Span::dummy(),
        ),
        Stmt::assign("x", Expr::int_lit(5)),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
        "a global-bound local must never carry a propagated constant"
    );
}

/// A closure capturing `$x` by reference can rewrite it whenever it is later
/// invoked, so `$x = 5; echo $x + 1;` after the capture must stay unfolded.
#[test]
fn test_closure_by_ref_capture_blocks_propagation() {
    let program = vec![
        Stmt::assign(
            "c",
            closure_with_captures(vec!["x".to_string()], vec!["x".to_string()]),
        ),
        Stmt::assign("x", Expr::int_lit(5)),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
        "a by-ref-captured local must never carry a propagated constant"
    );
}

/// A by-value capture leaves the outer variable untouched, so propagation
/// after the closure creation keeps folding.
#[test]
fn test_closure_by_value_capture_keeps_propagation() {
    let program = vec![
        Stmt::assign(
            "c",
            closure_with_captures(vec!["x".to_string()], Vec::new()),
        ),
        Stmt::assign("x", Expr::int_lit(5)),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::int_lit(6)),
        "a by-value capture must not block propagation of the outer variable"
    );
}

/// `foreach ($a as &$v) {}` leaves `$v` aliasing the last element, but a later
/// `$v = 9` still gives `$v` itself the value 9, so `$v` facts remain sound
/// and keep folding; only the array root is exposed.
#[test]
fn test_foreach_by_ref_keeps_value_var_facts() {
    let program = vec![
        Stmt::new(
            StmtKind::Foreach {
                array: Expr::var("a"),
                key_var: None,
                value_var: "v".to_string(),
                value_by_ref: true,
                body: Vec::new(),
            },
            Span::dummy(),
        ),
        Stmt::assign("v", Expr::int_lit(9)),
        Stmt::echo(Expr::binop(Expr::var("v"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::int_lit(10)),
        "the by-ref foreach value var's own value facts stay sound"
    );
}

/// `foreach ($a as &$v)` must mark the array root volatile: writes through
/// `$v` mutate `$a` invisibly, including after the loop ends.
#[test]
fn test_foreach_by_ref_marks_array_root_volatile() {
    let program = vec![Stmt::new(
        StmtKind::Foreach {
            array: Expr::var("a"),
            key_var: None,
            value_var: "v".to_string(),
            value_by_ref: true,
            body: Vec::new(),
        },
        Span::dummy(),
    )];

    propagate_constants(program);

    assert!(
        is_reference_volatile("a"),
        "the by-ref foreach array root must be marked volatile"
    );
    assert!(
        !is_reference_volatile("v"),
        "the by-ref foreach value var needs no volatility mark"
    );
}

/// `$t = &$a[0];` exposes `$a`'s storage through `$t`, so the array root of
/// the ref-assign source lvalue must be marked volatile.
#[test]
fn test_ref_assign_marks_source_lvalue_root_volatile() {
    let program = vec![Stmt::new(
        StmtKind::RefAssign {
            target: "t".to_string(),
            source: Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::var("a")),
                    index: Box::new(Expr::int_lit(0)),
                },
                Span::dummy(),
            ),
        },
        Span::dummy(),
    )];

    propagate_constants(program);

    assert!(
        is_reference_volatile("a"),
        "the ref-assign source lvalue root must be marked volatile"
    );
    assert!(is_reference_volatile("t"), "the ref-assign target stays volatile");
}

/// Request superglobals are writable from any scope under `--web`, so their
/// names are volatile from the start of every propagation run.
#[test]
fn test_superglobals_are_always_volatile() {
    propagate_constants(Vec::new());

    for name in crate::superglobals::SUPERGLOBALS {
        assert!(
            is_reference_volatile(name),
            "superglobal {name} must be volatile"
        );
    }
}
