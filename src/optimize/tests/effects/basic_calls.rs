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

/// Verifies that `eval` is modeled as an observable, throwing dynamic barrier.
#[test]
fn test_effect_analysis_treats_eval_as_dynamic_barrier() {
    let expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from("eval"),
            args: vec![Expr::string_lit("$x = 5;")],
        },
        Span::dummy(),
    );

    assert!(expr_has_side_effects(&expr));
    assert!(expr_effect(&expr).may_throw);
    assert!(expr_is_observable(&expr));
}

/// Verifies argument unpacking keeps surrounding catches because runtime validation may throw.
#[test]
fn test_effect_analysis_treats_spread_as_potentially_throwing() {
    let spread = Expr::new(
        ExprKind::Spread(Box::new(Expr::var("arguments"))),
        Span::dummy(),
    );

    assert!(!expr_has_side_effects(&spread));
    assert!(expr_effect(&spread).may_throw);
    assert!(expr_is_observable(&spread));
}

/// Verifies unknown property and array reads remain conservative dynamic barriers.
#[test]
fn test_effect_analysis_keeps_unknown_property_and_array_reads_observable() {
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

    assert!(expr_has_side_effects(&property));
    assert!(expr_effect(&property).may_throw);
    assert!(expr_has_side_effects(&array));
    assert!(expr_effect(&array).may_throw);
    assert!(expr_is_observable(&array));
}

/// Verifies literal array reads distinguish present offsets from warning-only misses.
#[test]
fn test_effect_analysis_refines_literal_array_reads() {
    let array = Expr::new(
        ExprKind::ArrayLiteral(vec![Expr::int_lit(10), Expr::int_lit(20)]),
        Span::dummy(),
    );
    let present = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(array.clone()),
            index: Box::new(Expr::int_lit(1)),
        },
        Span::dummy(),
    );
    let missing = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(array),
            index: Box::new(Expr::int_lit(4)),
        },
        Span::dummy(),
    );

    assert!(!expr_is_observable(&present));
    assert!(!expr_effect(&present).may_throw);
    assert!(expr_is_observable(&missing));
    assert!(!expr_effect(&missing).may_throw);
}

/// Verifies a read-only runtime registry probe no longer inherits the all-effects fallback.
#[test]
fn test_effect_analysis_refines_read_only_builtin_metadata() {
    let expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from("function_exists"),
            args: vec![Expr::string_lit("strlen")],
        },
        Span::dummy(),
    );

    assert!(!expr_has_side_effects(&expr));
    assert!(!expr_effect(&expr).may_throw);
    assert!(!expr_is_observable(&expr));
}

/// Verifies catchable builtin validation errors remain visible to try/catch pruning.
#[test]
fn test_effect_analysis_preserves_throwing_builtin_metadata() {
    let expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from("clamp"),
            args: vec![
                Expr::int_lit(5),
                Expr::int_lit(10),
                Expr::int_lit(0),
            ],
        },
        Span::dummy(),
    );

    assert!(expr_effect(&expr).may_throw);
    assert!(expr_is_observable(&expr));

    let statement = Stmt::new(StmtKind::ExprStmt(expr), Span::dummy());
    assert_eq!(
        prune_constant_control_flow(vec![statement.clone()]),
        vec![statement.clone()]
    );
    assert_eq!(eliminate_dead_code(vec![statement.clone()]), vec![statement]);
}

/// Verifies callback builtins combine a known callback summary with intrinsic array work.
#[test]
fn test_effect_analysis_refines_builtin_with_known_pure_callback() {
    let callback = Expr::new(
        ExprKind::FirstClassCallable(CallableTarget::Function(Name::from("strlen"))),
        Span::dummy(),
    );
    let expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from("array_map"),
            args: vec![
                callback,
                Expr::new(
                    ExprKind::ArrayLiteral(vec![Expr::string_lit("a")]),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    );

    assert!(!expr_has_side_effects(&expr));
    assert!(!expr_effect(&expr).may_throw);
    assert!(!expr_is_observable(&expr));
}

/// Verifies that a user-defined function whose body consists solely of a pure
/// builtin call (`strlen`) is classified as `Effect::PURE`.
#[test]
fn test_program_function_effects_recognize_pure_user_functions() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "len3".to_string(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
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
                param_attributes: Vec::new(),
                variadic: None,
                variadic_by_ref: false,
                variadic_type: None,
                return_type: None,
                by_ref_return: false,
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
                param_attributes: Vec::new(),
                variadic: None,
                variadic_by_ref: false,
                variadic_type: None,
                return_type: None,
                by_ref_return: false,
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
