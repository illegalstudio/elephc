//! Purpose:
//! Regression tests for optimizer effects methods behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Tests that a static method whose body contains only a call to a pure builtin
/// (strlen) is classified as PURE in static_method_effects.
#[test]
fn test_program_static_method_effects_recognize_pure_static_methods() {
    let program = vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Util".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: vec![ClassMethod {
                name: "len3".to_string(),
                visibility: Visibility::Public,
                is_static: true,
                is_abstract: false,
                is_final: false,
                has_body: true,
                params: Vec::new(),
                variadic: None,
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
                span: Span::dummy(),
                attributes: Vec::new(),
            }],
        constants: Vec::new(),
        },
        Span::dummy(),
    )];

    let (_, static_method_effects, _) = compute_program_callable_effects(&program);

    assert_eq!(
        static_method_effects.get("Util::len3"),
        Some(&Effect::PURE)
    );
}

/// Tests that a static method calling another static method via `self::` receiver
/// is correctly resolved and classified as PURE, provided the called method is pure.
#[test]
fn test_program_static_method_effects_resolve_self_receiver() {
    let program = vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Util".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: vec![
                ClassMethod {
                    name: "len3".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: Vec::new(),
                    variadic: None,
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
                    span: Span::dummy(),
                    attributes: Vec::new(),
                },
                ClassMethod {
                    name: "relay".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: Vec::new(),
                    variadic: None,
                    variadic_type: None,
                    return_type: None,
                    by_ref_return: false,
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::StaticMethodCall {
                                receiver: StaticReceiver::Self_,
                                method: "len3".to_string(),
                                args: Vec::new(),
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    )],
                    span: Span::dummy(),
                    attributes: Vec::new(),
                },
            ],
        constants: Vec::new(),
        },
        Span::dummy(),
    )];

    let (_, static_method_effects, _) = compute_program_callable_effects(&program);

    assert_eq!(
        static_method_effects.get("Util::relay"),
        Some(&Effect::PURE)
    );
}

/// Tests that a static method in a child class calling a parent static method via
/// `parent::` receiver is correctly resolved and classified as PURE, provided the
/// called method is pure.
#[test]
fn test_program_static_method_effects_resolve_parent_receiver() {
    let program = vec![
        Stmt::new(
            StmtKind::ClassDecl {
                name: "Base".to_string(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: vec![ClassMethod {
                    name: "len3".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: Vec::new(),
                    variadic: None,
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
                    span: Span::dummy(),
                    attributes: Vec::new(),
                }],
            constants: Vec::new(),
            },
            Span::dummy(),
        ),
        Stmt::new(
            StmtKind::ClassDecl {
                name: "Child".to_string(),
                extends: Some(Name::from("Base")),
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: vec![ClassMethod {
                    name: "relay".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: Vec::new(),
                    variadic: None,
                    variadic_type: None,
                    return_type: None,
                    by_ref_return: false,
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::StaticMethodCall {
                                receiver: StaticReceiver::Parent,
                                method: "len3".to_string(),
                                args: Vec::new(),
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    )],
                    span: Span::dummy(),
                    attributes: Vec::new(),
                }],
            constants: Vec::new(),
            },
            Span::dummy(),
        ),
    ];

    let (_, static_method_effects, _) = compute_program_callable_effects(&program);

    assert_eq!(
        static_method_effects.get("Child::relay"),
        Some(&Effect::PURE)
    );
}

/// Tests that a private instance (non-static) method whose body contains only a call
/// to a pure builtin (strlen) is classified as PURE in private_instance_method_effects.
#[test]
fn test_program_private_instance_method_effects_recognize_private_methods() {
    let program = vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Util".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: vec![ClassMethod {
                name: "len3".to_string(),
                visibility: Visibility::Private,
                is_static: false,
                is_abstract: false,
                is_final: false,
                has_body: true,
                params: Vec::new(),
                variadic: None,
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
                span: Span::dummy(),
                attributes: Vec::new(),
            }],
        constants: Vec::new(),
        },
        Span::dummy(),
    )];

    let (_, _, private_instance_method_effects) = compute_program_callable_effects(&program);

    assert_eq!(
        private_instance_method_effects.get("Util::len3"),
        Some(&Effect::PURE)
    );
}
