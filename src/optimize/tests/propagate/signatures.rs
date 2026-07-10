//! Purpose:
//! Regression tests for the by-ref signature pre-scan feeding targeted call
//! invalidation: user function signatures, method unions across classes and
//! traits, constructor by-ref detection, and the builtin-registry fallback.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Method by-ref positions union across all same-named methods so dynamic
//!   dispatch stays safe; builtins resolve even without an installed scan.

use super::*;
use crate::optimize::propagate::{
    any_ctor_by_ref, collect_by_ref_signatures, function_by_ref_params, is_user_function,
    method_by_ref_params, with_by_ref_signatures,
};

/// Builds a `FunctionDecl` with the given `(name, is_ref)` parameter list.
fn function_with_params(name: &str, params: Vec<(&str, bool)>) -> Stmt {
    Stmt::new(
        StmtKind::FunctionDecl {
            name: name.to_string(),
            params: params
                .into_iter()
                .map(|(param, is_ref)| (param.to_string(), None, None, is_ref))
                .collect(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: Vec::new(),
        },
        Span::dummy(),
    )
}

/// Builds a public non-static method with the given `(name, is_ref)` params.
fn method_with_params(name: &str, params: Vec<(&str, bool)>) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: params
            .into_iter()
            .map(|(param, is_ref)| (param.to_string(), None, None, is_ref))
            .collect(),
        variadic: None,
        variadic_type: None,
        return_type: None,
        by_ref_return: false,
        body: Vec::new(),
        span: Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a class declaration with the given methods and properties.
fn class_with(name: &str, methods: Vec<ClassMethod>, properties: Vec<ClassProperty>) -> Stmt {
    Stmt::new(
        StmtKind::ClassDecl {
            name: name.to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties,
            methods,
            constants: Vec::new(),
        },
        Span::dummy(),
    )
}

/// Builds a by-ref instance property declaration.
fn by_ref_property(name: &str) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Public,
        set_visibility: None,
        type_expr: None,
        hooks: crate::parser::ast::PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: true,
        default: None,
        span: Span::dummy(),
        attributes: Vec::new(),
    }
}

/// A user function's by-ref parameter positions come from its declaration.
#[test]
fn test_user_function_by_ref_params_collected() {
    let program = vec![function_with_params("f", vec![("a", true), ("b", false)])];
    let sigs = collect_by_ref_signatures(&program);

    with_by_ref_signatures(sigs, || {
        assert_eq!(
            function_by_ref_params("f"),
            Some(vec![("a".to_string(), true), ("b".to_string(), false)])
        );
        assert!(is_user_function("f"));
        assert!(!is_user_function("sort"));
        assert!(!is_user_function("nope_missing"));
    });
}

/// Same-named methods union their by-ref positions across classes and traits
/// (dynamic dispatch cannot tell them apart).
#[test]
fn test_method_by_ref_params_union_across_declarations() {
    let program = vec![
        class_with("A", vec![method_with_params("m", vec![("x", true)])], Vec::new()),
        Stmt::new(
            StmtKind::TraitDecl {
                name: "T".to_string(),
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: vec![method_with_params("m", vec![("x", false), ("y", true)])],
                constants: Vec::new(),
            },
            Span::dummy(),
        ),
    ];
    let sigs = collect_by_ref_signatures(&program);

    with_by_ref_signatures(sigs, || {
        assert_eq!(
            method_by_ref_params("m"),
            Some(vec![("x".to_string(), true), ("y".to_string(), true)])
        );
        assert_eq!(method_by_ref_params("missing"), None);
    });
}

/// A by-ref constructor parameter or a by-ref property flags `any_ctor_by_ref`.
#[test]
fn test_ctor_by_ref_detection() {
    let plain = collect_by_ref_signatures(&[class_with(
        "P",
        vec![method_with_params("__construct", vec![("v", false)])],
        Vec::new(),
    )]);
    with_by_ref_signatures(plain, || assert!(!any_ctor_by_ref()));

    let ref_param = collect_by_ref_signatures(&[class_with(
        "R",
        vec![method_with_params("__construct", vec![("v", true)])],
        Vec::new(),
    )]);
    with_by_ref_signatures(ref_param, || assert!(any_ctor_by_ref()));

    let ref_prop = collect_by_ref_signatures(&[class_with(
        "B",
        Vec::new(),
        vec![by_ref_property("value")],
    )]);
    with_by_ref_signatures(ref_prop, || assert!(any_ctor_by_ref()));
}

/// Builtins resolve through the registry even when no scan is installed, and
/// user declarations shadow nothing (disjoint namespaces are the checker's job).
#[test]
fn test_builtin_by_ref_params_from_registry() {
    let sort = function_by_ref_params("sort").expect("sort is a registry builtin");
    assert!(
        sort.first().is_some_and(|(_, by_ref)| *by_ref),
        "sort's first parameter is by-ref"
    );

    let strlen = function_by_ref_params("strlen").expect("strlen is a registry builtin");
    assert!(
        strlen.iter().all(|(_, by_ref)| !by_ref),
        "strlen takes no by-ref parameters"
    );

    assert_eq!(function_by_ref_params("definitely_not_a_symbol"), None);
}

/// A function-variant group unions the by-ref positions of its variants, so a
/// call through the group name stays safe whichever variant is linked.
#[test]
fn test_function_variant_group_unions_variant_signatures() {
    let program = vec![
        function_with_params("f__v1", vec![("a", true)]),
        function_with_params("f__v2", vec![("a", false), ("b", true)]),
        Stmt::new(
            StmtKind::FunctionVariantGroup {
                name: "f".to_string(),
                variants: vec!["f__v1".to_string(), "f__v2".to_string()],
            },
            Span::dummy(),
        ),
    ];
    let sigs = collect_by_ref_signatures(&program);

    with_by_ref_signatures(sigs, || {
        assert_eq!(
            function_by_ref_params("f"),
            Some(vec![("a".to_string(), true), ("b".to_string(), true)])
        );
    });
}

/// `propagate_args` substitutes constants into by-value positions but leaves
/// by-ref positions untouched: they must stay lvalues for the backend to take
/// the slot address (and for PHP validity).
#[test]
fn test_propagate_args_masks_by_ref_positions() {
    let mut env = ConstantEnv::new();
    env.insert("x".to_string(), PropagatedValue::Scalar(ScalarValue::Int(5)));
    env.insert("y".to_string(), PropagatedValue::Scalar(ScalarValue::Int(7)));
    let sig = vec![("a".to_string(), true), ("b".to_string(), false)];

    let args = propagate_args(
        vec![Expr::var("x"), Expr::var("y")],
        Some(&env),
        Some(&sig),
    );

    assert_eq!(args[0], Expr::var("x"), "by-ref position must stay an lvalue");
    assert_eq!(args[1], Expr::int_lit(7), "by-value position substitutes");
}

/// Named arguments match by parameter name, not position.
#[test]
fn test_propagate_args_masks_named_by_ref_arguments() {
    let mut env = ConstantEnv::new();
    env.insert("x".to_string(), PropagatedValue::Scalar(ScalarValue::Int(5)));
    env.insert("y".to_string(), PropagatedValue::Scalar(ScalarValue::Int(7)));
    let sig = vec![("a".to_string(), true), ("b".to_string(), false)];

    let named = |param: &str, var: &str| {
        Expr::new(
            ExprKind::NamedArg {
                name: param.to_string(),
                value: Box::new(Expr::var(var)),
            },
            Span::dummy(),
        )
    };
    let args = propagate_args(
        vec![named("b", "y"), named("a", "x")],
        Some(&env),
        Some(&sig),
    );

    let named_int = |param: &str, value: i64| {
        Expr::new(
            ExprKind::NamedArg {
                name: param.to_string(),
                value: Box::new(Expr::int_lit(value)),
            },
            Span::dummy(),
        )
    };
    assert_eq!(args[0], named_int("b", 7), "by-value named argument substitutes");
    assert_eq!(args[1], named("a", "x"), "by-ref named argument stays an lvalue");
}
