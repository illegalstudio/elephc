//! Purpose:
//! Regression tests for the targeted local-write invalidation analysis:
//! by-ref call arguments, unset beyond plain variables, unknown callees, and
//! the top-level globals guard.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Tests drive `expr_invalidation` directly under signature/effect installs;
//!   `Invalidation::Names` results are compared structurally.

use super::*;
use crate::optimize::propagate::{
    collect_by_ref_signatures, expr_invalidation, is_reference_volatile,
    reset_reference_volatile, with_by_ref_signatures, with_function_scope, Invalidation,
};

/// Builds a `FunctionCall` expression.
fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::from(name),
            args,
        },
        Span::dummy(),
    )
}

/// Builds an `ArrayAccess` expression.
fn array_access(array: Expr, index: Expr) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(array),
            index: Box::new(index),
        },
        Span::dummy(),
    )
}

/// Shorthand for `Invalidation::Names` over the given names.
fn names(list: &[&str]) -> Invalidation {
    Invalidation::Names(list.iter().map(|name| name.to_string()).collect())
}

/// `unset($a[0])` writes only the root variable; the index contributes its own
/// invalidation.
#[test]
fn test_unset_array_element_invalidates_root_only() {
    let expr = call("unset", vec![array_access(Expr::var("a"), Expr::int_lit(0))]);
    assert_eq!(expr_invalidation(&expr), names(&["a"]));
}

/// `unset($o->p)` writes heap state, not a caller local.
#[test]
fn test_unset_property_invalidates_nothing() {
    let expr = call(
        "unset",
        vec![Expr::new(
            ExprKind::PropertyAccess {
                object: Box::new(Expr::var("o")),
                property: "p".to_string(),
            },
            Span::dummy(),
        )],
    );
    assert_eq!(expr_invalidation(&expr), names(&[]));
}

/// A by-ref builtin (`sort`) invalidates exactly its by-ref argument, without
/// volatilizing it: builtins never retain references.
#[test]
fn test_by_ref_builtin_invalidates_argument_without_retention() {
    reset_reference_volatile();
    let expr = call("sort", vec![Expr::var("a")]);
    assert_eq!(expr_invalidation(&expr), names(&["a"]));
    assert!(
        !is_reference_volatile("a"),
        "builtin by-ref arguments are not retained"
    );
}

/// A pure by-value builtin invalidates nothing.
#[test]
fn test_by_value_builtin_invalidates_nothing() {
    let expr = call("strlen", vec![Expr::var("s")]);
    assert_eq!(expr_invalidation(&expr), names(&[]));
}

/// A user function's by-ref parameter invalidates the argument root and marks
/// it volatile: the callee may retain the reference.
#[test]
fn test_user_by_ref_param_invalidates_and_retains() {
    reset_reference_volatile();
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "f".to_string(),
            params: vec![
                ("p".to_string(), None, None, true),
                ("q".to_string(), None, None, false),
            ],
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: Vec::new(),
        },
        Span::dummy(),
    )];
    let sigs = collect_by_ref_signatures(&program);

    with_by_ref_signatures(sigs, || {
        with_function_scope(|| {
            let expr = call("f", vec![Expr::var("x"), Expr::var("y")]);
            assert_eq!(expr_invalidation(&expr), names(&["x"]));
            assert!(is_reference_volatile("x"), "user callees may retain the ref");
            assert!(!is_reference_volatile("y"), "by-value args are not exposed");
        });
    });
}

/// An unknown callee (`$f(...)`) can write any lvalue-rooted argument, but
/// nothing else, inside a function body.
#[test]
fn test_unknown_callee_exposes_variable_arguments_only() {
    reset_reference_volatile();
    with_function_scope(|| {
        let expr = Expr::new(
            ExprKind::ClosureCall {
                var: "cb".to_string(),
                args: vec![Expr::var("x"), Expr::int_lit(3)],
            },
            Span::dummy(),
        );
        assert_eq!(expr_invalidation(&expr), names(&["x"]));
        assert!(is_reference_volatile("x"));
    });
}

/// At top level, a callee that writes globals (or an unknown callee) can write
/// any local; inside a function body the same call touches nothing (its
/// `global`-bound names are volatile instead).
#[test]
fn test_top_level_globals_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "gw".to_string(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::Global {
                        vars: vec!["g".to_string()],
                    },
                    Span::dummy(),
                ),
                Stmt::assign("g", Expr::int_lit(1)),
            ],
        },
        Span::dummy(),
    )];
    let (function_effects, static_method_effects, private_instance_method_effects) =
        compute_program_callable_effects(&program);
    let sigs = collect_by_ref_signatures(&program);

    with_callable_effects(
        function_effects,
        static_method_effects,
        private_instance_method_effects,
        || {
            with_by_ref_signatures(sigs, || {
                let expr = call("gw", Vec::new());
                assert_eq!(
                    expr_invalidation(&expr),
                    Invalidation::All,
                    "top-level locals are globals"
                );
                with_function_scope(|| {
                    assert_eq!(
                        expr_invalidation(&expr),
                        names(&[]),
                        "in-function locals are unreachable from `global`"
                    );
                });
            });
        },
    );
}

/// Increments and inline assignments report their exact write set.
#[test]
fn test_known_writes_stay_exact() {
    let inc = Expr::new(ExprKind::PreIncrement("i".to_string()), Span::dummy());
    assert_eq!(expr_invalidation(&inc), names(&["i"]));
}

/// `yield` gives up: the generator's consumer can run arbitrary code between
/// resumptions.
#[test]
fn test_yield_invalidates_all() {
    let expr = Expr::new(
        ExprKind::Yield {
            key: None,
            value: Some(Box::new(Expr::int_lit(1))),
        },
        Span::dummy(),
    );
    assert_eq!(expr_invalidation(&expr), Invalidation::All);
}
