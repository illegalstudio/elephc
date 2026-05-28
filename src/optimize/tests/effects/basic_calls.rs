//! Purpose:
//! Regression tests for optimizer effects basic_calls behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Verifies that `strlen` is classified as a pure call with no side effects,
/// no exception potential, and no observable behavior.
#[test]
fn test_effect_analysis_recognizes_pure_builtin_calls() {
    let expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from("strlen"),
            args: vec![Expr::string_lit("abc")],
        },
        Span::dummy(),
    );

    assert!(!expr_has_side_effects(&expr));
    assert!(!expr_effect(&expr).may_throw);
    assert!(!expr_is_observable(&expr));
}

/// Verifies that property accesses (`.`) are pure while array accesses (`[]`)
/// are observable and may throw (e.g., undefined index).
#[test]
fn test_effect_analysis_treats_property_reads_as_pure_and_array_reads_as_observable() {
    let property = Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::var("entry")),
            property: "name".to_string(),
        },
        Span::dummy(),
    );
    let array = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::var("items")),
            index: Box::new(Expr::int_lit(0)),
        },
        Span::dummy(),
    );

    assert!(!expr_has_side_effects(&property));
    assert!(!expr_effect(&property).may_throw);
    assert!(expr_has_side_effects(&array));
    assert!(expr_effect(&array).may_throw);
    assert!(expr_is_observable(&array));
}

/// Verifies that a user-defined function whose body consists solely of a pure
/// builtin call (`strlen`) is classified as `Effect::PURE`.
#[test]
fn test_program_function_effects_recognize_pure_user_functions() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "len3".to_string(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Return(Some(Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::from("strlen"),
                        args: vec![Expr::string_lit("abc")],
                    },
                    Span::dummy(),
                ))),
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(function_effects.get("len3"), Some(&Effect::PURE));
}

/// Verifies that a wrapper function calling a function that throws is classified
/// as `PURE` with `side_effects` and `may_throw` — the throw does not make the
/// wrapper non-pure, but it does propagate the exception potential.
#[test]
fn test_program_function_effects_propagate_throwing_calls() {
    let program = vec![
        Stmt::new(
            StmtKind::FunctionDecl {
                name: "boom".to_string(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![Stmt::new(
                    StmtKind::Throw(Expr::new(
                        ExprKind::NewObject {
                            class_name: Name::from("Exception"),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    )),
                    Span::dummy(),
                )],
            },
            Span::dummy(),
        ),
        Stmt::new(
            StmtKind::FunctionDecl {
                name: "wrapper".to_string(),
                params: Vec::new(),
                variadic: None,
                return_type: None,
                body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::FunctionCall {
                            name: Name::from("boom"),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )],
            },
            Span::dummy(),
        ),
    ];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("wrapper"),
        Some(&Effect::PURE.with_side_effects().with_may_throw())
    );
}
