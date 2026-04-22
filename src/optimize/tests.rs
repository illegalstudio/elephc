use super::*;
use crate::names::Name;
use crate::parser::ast::{ClassProperty, StaticReceiver, Visibility};
use crate::span::Span;

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

#[test]
fn test_effect_analysis_treats_property_and_array_reads_as_pure() {
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
    assert!(!expr_has_side_effects(&array));
    assert!(!expr_effect(&array).may_throw);
}

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

#[test]
fn test_program_static_method_effects_recognize_pure_static_methods() {
    let program = vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Util".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: vec![ClassMethod {
                name: "len3".to_string(),
                visibility: Visibility::Public,
                is_static: true,
                is_abstract: false,
                has_body: true,
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
                span: Span::dummy(),
            }],
        },
        Span::dummy(),
    )];

    let (_, static_method_effects, _) = compute_program_callable_effects(&program);

    assert_eq!(
        static_method_effects.get("Util::len3"),
        Some(&Effect::PURE)
    );
}

#[test]
fn test_program_static_method_effects_resolve_self_receiver() {
    let program = vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Util".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: vec![
                ClassMethod {
                    name: "len3".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    has_body: true,
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
                    span: Span::dummy(),
                },
                ClassMethod {
                    name: "relay".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    has_body: true,
                    params: Vec::new(),
                    variadic: None,
                    return_type: None,
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
                },
            ],
        },
        Span::dummy(),
    )];

    let (_, static_method_effects, _) = compute_program_callable_effects(&program);

    assert_eq!(
        static_method_effects.get("Util::relay"),
        Some(&Effect::PURE)
    );
}

#[test]
fn test_program_static_method_effects_resolve_parent_receiver() {
    let program = vec![
        Stmt::new(
            StmtKind::ClassDecl {
                name: "Base".to_string(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: vec![ClassMethod {
                    name: "len3".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    has_body: true,
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
                    span: Span::dummy(),
                }],
            },
            Span::dummy(),
        ),
        Stmt::new(
            StmtKind::ClassDecl {
                name: "Child".to_string(),
                extends: Some(Name::from("Base")),
                implements: Vec::new(),
                is_abstract: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: vec![ClassMethod {
                    name: "relay".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    has_body: true,
                    params: Vec::new(),
                    variadic: None,
                    return_type: None,
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
                }],
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

#[test]
fn test_program_private_instance_method_effects_recognize_private_methods() {
    let program = vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Util".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: vec![ClassMethod {
                name: "len3".to_string(),
                visibility: Visibility::Private,
                is_static: false,
                is_abstract: false,
                has_body: true,
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
                span: Span::dummy(),
            }],
        },
        Span::dummy(),
    )];

    let (_, _, private_instance_method_effects) = compute_program_callable_effects(&program);

    assert_eq!(
        private_instance_method_effects.get("Util::len3"),
        Some(&Effect::PURE)
    );
}

#[test]
fn test_effect_analysis_tracks_pure_iife_expr_calls() {
    let expr = Expr::new(
        ExprKind::ExprCall {
            callee: Box::new(Expr::new(
                ExprKind::Closure {
                    params: Vec::new(),
                    variadic: None,
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
                    is_arrow: false,
                    captures: Vec::new(),
                },
                Span::dummy(),
            )),
            args: Vec::new(),
        },
        Span::dummy(),
    );

    assert!(!expr_has_side_effects(&expr));
    assert!(!expr_effect(&expr).may_throw);
    assert!(!expr_is_observable(&expr));
}

#[test]
fn test_program_function_effects_track_closure_alias_locals() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::Closure {
                                params: Vec::new(),
                                variadic: None,
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
                                is_arrow: false,
                                captures: Vec::new(),
                            },
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "f".to_string(),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

#[test]
fn test_program_function_effects_track_callable_alias_through_ternary() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: vec![("flag".to_string(), None, None, false)],
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::Ternary {
                                condition: Box::new(Expr::var("flag")),
                                then_expr: Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                )),
                                else_expr: Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                )),
                            },
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "f".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

#[test]
fn test_program_function_effects_track_callable_alias_through_match() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: vec![("flag".to_string(), None, None, false)],
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::Match {
                                subject: Box::new(Expr::var("flag")),
                                arms: vec![(
                                    vec![Expr::int_lit(1)],
                                    Expr::new(
                                        ExprKind::FirstClassCallable(
                                            CallableTarget::Function(Name::from("strlen")),
                                        ),
                                        Span::dummy(),
                                    ),
                                )],
                                default: Some(Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                ))),
                            },
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "f".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

#[test]
fn test_program_function_effects_track_callable_alias_through_null_coalesce() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::NullCoalesce {
                                value: Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                )),
                                default: Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                )),
                            },
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "f".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

#[test]
fn test_effect_analysis_tracks_named_first_class_callable_expr_calls() {
    let expr = Expr::new(
        ExprKind::ExprCall {
            callee: Box::new(Expr::new(
                ExprKind::FirstClassCallable(CallableTarget::Function(Name::from("strlen"))),
                Span::dummy(),
            )),
            args: vec![Expr::string_lit("abc")],
        },
        Span::dummy(),
    );

    assert!(!expr_has_side_effects(&expr));
    assert!(!expr_effect(&expr).may_throw);
    assert!(!expr_is_observable(&expr));
}

#[test]
fn test_program_function_effects_track_callable_alias_locals() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::FirstClassCallable(CallableTarget::Function(Name::from(
                                "strlen",
                            ))),
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Assign {
                        name: "g".to_string(),
                        value: Expr::var("f"),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "g".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

#[test]
fn test_program_function_effects_merge_callable_aliases_across_if_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: vec![("flag".to_string(), None, None, false)],
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![Stmt::new(
                            StmtKind::Assign {
                                name: "g".to_string(),
                                value: Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                ),
                            },
                            Span::dummy(),
                        )],
                        elseif_clauses: Vec::new(),
                        else_body: Some(vec![Stmt::new(
                            StmtKind::Assign {
                                name: "g".to_string(),
                                value: Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                ),
                            },
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "g".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

#[test]
fn test_program_function_effects_merge_callable_aliases_across_try_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::new(
                            StmtKind::Assign {
                                name: "g".to_string(),
                                value: Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                ),
                            },
                            Span::dummy(),
                        )],
                        catches: vec![crate::parser::ast::CatchClause {
                            exception_types: vec![Name::from("Exception")],
                            variable: Some("e".to_string()),
                            body: vec![Stmt::new(
                                StmtKind::Assign {
                                    name: "g".to_string(),
                                    value: Expr::new(
                                        ExprKind::FirstClassCallable(CallableTarget::Function(
                                            Name::from("strlen"),
                                        )),
                                        Span::dummy(),
                                    ),
                                },
                                Span::dummy(),
                            )],
                        }],
                        finally_body: Some(vec![Stmt::new(
                            StmtKind::ExprStmt(Expr::string_lit("done")),
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "g".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

#[test]
fn test_program_function_effects_merge_callable_aliases_across_switch_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: vec![("flag".to_string(), None, None, false)],
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Switch {
                        subject: Expr::var("flag"),
                        cases: vec![
                            (
                                vec![Expr::int_lit(1)],
                                vec![
                                    Stmt::new(
                                        StmtKind::Assign {
                                            name: "g".to_string(),
                                            value: Expr::new(
                                                ExprKind::FirstClassCallable(
                                                    CallableTarget::Function(Name::from("strlen")),
                                                ),
                                                Span::dummy(),
                                            ),
                                        },
                                        Span::dummy(),
                                    ),
                                    Stmt::new(StmtKind::Break, Span::dummy()),
                                ],
                            ),
                            (vec![Expr::int_lit(2)], Vec::new()),
                        ],
                        default: Some(vec![Stmt::new(
                            StmtKind::Assign {
                                name: "g".to_string(),
                                value: Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                ),
                            },
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "g".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

#[test]
fn test_fold_nested_integer_arithmetic() {
    let program = vec![Stmt::new(
        StmtKind::Echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::int_lit(2)),
                        op: BinOp::Add,
                        right: Box::new(Expr::int_lit(3)),
                    },
                    Span::dummy(),
                )),
                op: BinOp::Mul,
                right: Box::new(Expr::int_lit(4)),
            },
            Span::dummy(),
        )),
        Span::dummy(),
    )];

    let folded = fold_constants(program);

    assert_eq!(folded, vec![Stmt::echo(Expr::int_lit(20))]);
}

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
                        Stmt::new(StmtKind::Break, Span::dummy()),
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
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

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
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

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

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_for_loop() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("i", Expr::int_lit(0)))),
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(3))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![Stmt::echo(Expr::var("i"))],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_inside_while_loop_body() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::assign("i", Expr::int_lit(0)),
        Stmt::new(
            StmtKind::While {
                condition: Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(2)),
                body: vec![
                    Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
                    Stmt::new(
                        StmtKind::ExprStmt(Expr::new(
                            ExprKind::PostIncrement("i".to_string()),
                            Span::dummy(),
                        )),
                        Span::dummy(),
                    ),
                ],
            },
            Span::dummy(),
        ),
    ];

    let propagated = propagate_constants(program);

    let StmtKind::While { body, .. } = &propagated[2].kind else {
        panic!("expected while");
    };

    assert_eq!(
        body[0],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_loop_with_switch() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("i", Expr::int_lit(0)))),
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(3))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![Stmt::new(
                    StmtKind::Switch {
                        subject: Expr::var("i"),
                        cases: vec![(
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::var("i")),
                                Stmt::new(StmtKind::Break, Span::dummy()),
                            ],
                        )],
                        default: Some(vec![Stmt::echo(Expr::var("i"))]),
                    },
                    Span::dummy(),
                )],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_loop_with_try() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("i", Expr::int_lit(0)))),
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(3))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::echo(Expr::var("i"))],
                        catches: vec![crate::parser::ast::CatchClause {
                            exception_types: vec![Name::from("Exception")],
                            variable: Some("e".to_string()),
                            body: vec![Stmt::echo(Expr::int_lit(9))],
                        }],
                        finally_body: Some(vec![]),
                    },
                    Span::dummy(),
                )],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_foreach_loop() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::new(
            StmtKind::Foreach {
                array: Expr::new(
                    ExprKind::ArrayLiteral(vec![
                        Expr::int_lit(1),
                        Expr::int_lit(2),
                        Expr::int_lit(3),
                    ]),
                    Span::dummy(),
                ),
                key_var: Some("k".to_string()),
                value_var: "value".to_string(),
                body: vec![Stmt::echo(Expr::var("value"))],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_tracks_stable_for_init_assignments() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::assign("i", Expr::int_lit(0)),
        Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("exp", Expr::int_lit(3)))),
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(2))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![Stmt::echo(Expr::binop(
                    Expr::var("base"),
                    BinOp::Pow,
                    Expr::var("exp"),
                ))],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::var("exp")),
    ];

    let propagated = propagate_constants(program);

    let StmtKind::For { body, .. } = &propagated[2].kind else {
        panic!("expected for");
    };

    assert_eq!(
        body[0],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
    assert_eq!(
        propagated[3],
        Stmt::echo(Expr::new(ExprKind::IntLiteral(3), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_nested_loops() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::assign("i", Expr::int_lit(0)),
        Stmt::new(
            StmtKind::For {
                init: None,
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(2))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![
                    Stmt::assign("j", Expr::int_lit(0)),
                    Stmt::new(
                        StmtKind::While {
                            condition: Expr::binop(
                                Expr::var("j"),
                                BinOp::Lt,
                                Expr::int_lit(2),
                            ),
                            body: vec![
                                Stmt::echo(Expr::var("j")),
                                Stmt::new(
                                    StmtKind::ExprStmt(Expr::new(
                                        ExprKind::PostIncrement("j".to_string()),
                                        Span::dummy(),
                                    )),
                                    Span::dummy(),
                                ),
                            ],
                        },
                        Span::dummy(),
                    ),
                ],
            },
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

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_loop_local_array_writes() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("i", Expr::int_lit(0)))),
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(3))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![
                    Stmt::new(
                        StmtKind::ArrayPush {
                            array: "items".to_string(),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::ArrayAssign {
                            array: "items".to_string(),
                            index: Expr::int_lit(0),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                ],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

#[test]
fn test_propagate_constants_preserves_unmodified_scalar_across_loop_property_writes() {
    let program = vec![
        Stmt::assign("base", Expr::int_lit(2)),
        Stmt::new(
            StmtKind::For {
                init: Some(Box::new(Stmt::assign("i", Expr::int_lit(0)))),
                condition: Some(Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(3))),
                update: Some(Box::new(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::PostIncrement("i".to_string()),
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ))),
                body: vec![
                    Stmt::new(
                        StmtKind::PropertyAssign {
                            object: Box::new(Expr::var("box")),
                            property: "last".to_string(),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::PropertyArrayPush {
                            object: Box::new(Expr::var("box")),
                            property: "items".to_string(),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::PropertyArrayAssign {
                            object: Box::new(Expr::var("box")),
                            property: "items".to_string(),
                            index: Expr::int_lit(0),
                            value: Expr::var("i"),
                        },
                        Span::dummy(),
                    ),
                ],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}

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

#[test]
fn test_fold_constant_pow_to_float_literal() {
    let program = vec![Stmt::echo(Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::int_lit(2)),
            op: BinOp::Pow,
            right: Box::new(Expr::int_lit(3)),
        },
        Span::dummy(),
    ))];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![Stmt::echo(Expr::new(
            ExprKind::FloatLiteral(8.0),
            Span::dummy(),
        ))]
    );
}

#[test]
fn test_skip_division_by_zero_fold() {
    let expr = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::int_lit(5)),
            op: BinOp::Div,
            right: Box::new(Expr::int_lit(0)),
        },
        Span::dummy(),
    );

    let folded = fold_constants(vec![Stmt::echo(expr.clone())]);

    assert_eq!(folded, vec![Stmt::echo(expr)]);
}

#[test]
fn test_fold_string_concat_and_property_default() {
    let property = ClassProperty {
        name: "label".to_string(),
        visibility: Visibility::Public,
        readonly: false,
        default: Some(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::string_lit("hello ")),
                op: BinOp::Concat,
                right: Box::new(Expr::string_lit("world")),
            },
            Span::dummy(),
        )),
        span: Span::dummy(),
    };

    let folded = fold_constants(vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Greeter".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: vec![property],
            methods: Vec::new(),
        },
        Span::dummy(),
    )]);

    let StmtKind::ClassDecl { properties, .. } = &folded[0].kind else {
        panic!("expected class declaration");
    };
    assert_eq!(
        properties[0].default,
        Some(Expr::string_lit("hello world"))
    );
}

#[test]
fn test_fold_strict_and_numeric_comparisons() {
    let program = vec![
        Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::int_lit(2)),
                op: BinOp::StrictEq,
                right: Box::new(Expr::int_lit(2)),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::float_lit(2.5)),
                op: BinOp::Lt,
                right: Box::new(Expr::float_lit(3.0)),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::int_lit(2)),
                op: BinOp::Spaceship,
                right: Box::new(Expr::int_lit(3)),
            },
            Span::dummy(),
        )),
    ];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
            Stmt::echo(Expr::int_lit(-1)),
        ]
    );
}

#[test]
fn test_fold_null_coalesce_and_ternary_only_for_scalar_constants() {
    let program = vec![
        Stmt::echo(Expr::new(
            ExprKind::NullCoalesce {
                value: Box::new(Expr::new(ExprKind::Null, Span::dummy())),
                default: Box::new(Expr::string_lit("fallback")),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Ternary {
                condition: Box::new(Expr::string_lit("0")),
                then_expr: Box::new(Expr::int_lit(10)),
                else_expr: Box::new(Expr::int_lit(20)),
            },
            Span::dummy(),
        )),
    ];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![
            Stmt::echo(Expr::string_lit("fallback")),
            Stmt::echo(Expr::int_lit(20)),
        ]
    );
}

#[test]
fn test_fold_logical_ops_and_not_using_php_truthiness() {
    let program = vec![
        Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::string_lit("0")),
                op: BinOp::Or,
                right: Box::new(Expr::string_lit("hello")),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Not(Box::new(Expr::string_lit("0"))),
            Span::dummy(),
        )),
    ];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
        ]
    );
}

#[test]
fn test_fold_scalar_casts_when_result_is_unambiguous() {
    let program = vec![
        Stmt::echo(Expr::new(
            ExprKind::Cast {
                target: CastType::Int,
                expr: Box::new(Expr::float_lit(3.7)),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Cast {
                target: CastType::Float,
                expr: Box::new(Expr::string_lit("3.14")),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Cast {
                target: CastType::Bool,
                expr: Box::new(Expr::string_lit("0")),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Cast {
                target: CastType::String,
                expr: Box::new(Expr::int_lit(42)),
            },
            Span::dummy(),
        )),
    ];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![
            Stmt::echo(Expr::int_lit(3)),
            Stmt::echo(Expr::float_lit(3.14)),
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
            Stmt::echo(Expr::string_lit("42")),
        ]
    );
}

#[test]
fn test_keep_ambiguous_string_casts_unfolded() {
    let expr = Expr::new(
        ExprKind::Cast {
            target: CastType::Int,
            expr: Box::new(Expr::string_lit("42abc")),
        },
        Span::dummy(),
    );

    let folded = fold_constants(vec![Stmt::echo(expr.clone())]);

    assert_eq!(folded, vec![Stmt::echo(expr)]);
}

#[test]
fn test_prune_constant_if_chain() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
            then_body: vec![Stmt::echo(Expr::int_lit(1))],
            elseif_clauses: vec![
                (
                    Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                    vec![Stmt::echo(Expr::int_lit(2))],
                ),
                (
                    Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                    vec![Stmt::echo(Expr::int_lit(3))],
                ),
            ],
            else_body: Some(vec![Stmt::echo(Expr::int_lit(4))]),
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(3))]);
}

#[test]
fn test_prune_while_false_and_do_while_false() {
    let program = vec![
        Stmt::new(
            StmtKind::While {
                condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                body: vec![Stmt::echo(Expr::int_lit(1))],
            },
            Span::dummy(),
        ),
        Stmt::new(
            StmtKind::DoWhile {
                body: vec![Stmt::echo(Expr::int_lit(2))],
                condition: Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
            },
            Span::dummy(),
        ),
    ];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(2))]);
}

#[test]
fn test_prune_for_false_keeps_init_only() {
    let program = vec![Stmt::new(
        StmtKind::For {
            init: Some(Box::new(Stmt::assign("i", Expr::int_lit(1)))),
            condition: Some(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
            update: Some(Box::new(Stmt::assign("i", Expr::int_lit(2)))),
            body: vec![Stmt::echo(Expr::int_lit(3))],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::assign("i", Expr::int_lit(1))]);
}

#[test]
fn test_prune_block_drops_statements_after_return() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(StmtKind::Return(Some(Expr::int_lit(7))), Span::dummy()),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(body[0].kind, StmtKind::Return(_)));
}

#[test]
fn test_prune_drops_pure_expr_stmt() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::ExprStmt(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(8))),
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(7)),
            ],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert_eq!(body[0], Stmt::echo(Expr::int_lit(7)));
}

#[test]
fn test_prune_ternary_drops_unused_pure_branch() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Ternary {
                condition: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                then_expr: Box::new(Expr::var("answer")),
                else_expr: Box::new(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(8))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::assign("x", Expr::var("answer"))]);
}

#[test]
fn test_prune_short_circuit_drops_unused_pure_rhs() {
    let program = vec![Stmt::echo(Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
            op: BinOp::Or,
            right: Box::new(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(8))),
        },
        Span::dummy(),
    ))];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy()))]
    );
}

#[test]
fn test_prune_block_drops_statements_after_exhaustive_if() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        elseif_clauses: Vec::new(),
                        else_body: Some(vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(8))),
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    let StmtKind::If { .. } = &body[0].kind else {
        panic!("expected if");
    };
}

#[test]
fn test_prune_block_drops_statements_after_exhaustive_switch() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Switch {
                        subject: Expr::var("flag"),
                        cases: vec![(
                            vec![Expr::int_lit(1)],
                            vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(7))),
                                Span::dummy(),
                            )],
                        )],
                        default: Some(vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(8))),
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::FunctionDecl { body, .. } = &pruned[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    let StmtKind::If { .. } = &body[0].kind else {
        panic!("expected normalized if");
    };
}

#[test]
fn test_prune_switch_case_body_drops_statements_after_break() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(1),
            cases: vec![(
                vec![Expr::int_lit(1)],
                vec![
                    Stmt::new(StmtKind::Break, Span::dummy()),
                    Stmt::echo(Expr::int_lit(9)),
                ],
            )],
            default: None,
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    assert!(pruned.is_empty());
}

#[test]
fn test_prune_match_expr_to_selected_arm() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Match {
                subject: Box::new(Expr::int_lit(3)),
                arms: vec![
                    (vec![Expr::int_lit(1)], Expr::int_lit(10)),
                    (vec![Expr::int_lit(3)], Expr::int_lit(20)),
                ],
                default: Some(Box::new(Expr::int_lit(30))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::assign("x", Expr::int_lit(20))]);
}

#[test]
fn test_prune_match_uses_strict_case_comparison() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Match {
                subject: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
                arms: vec![(vec![Expr::int_lit(1)], Expr::int_lit(10))],
                default: Some(Box::new(Expr::int_lit(20))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::assign("x", Expr::int_lit(20))]);
}

#[test]
fn test_prune_match_drops_fully_shadowed_duplicate_arm() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Match {
                subject: Box::new(Expr::var("value")),
                arms: vec![
                    (vec![Expr::int_lit(1)], Expr::int_lit(10)),
                    (vec![Expr::int_lit(1)], Expr::int_lit(20)),
                ],
                default: Some(Box::new(Expr::int_lit(30))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::Assign { value, .. } = &pruned[0].kind else {
        panic!("expected assign");
    };
    let ExprKind::Match { arms, default, .. } = &value.kind else {
        panic!("expected match");
    };
    assert_eq!(arms.len(), 1);
    assert_eq!(arms[0].0, vec![Expr::int_lit(1)]);
    assert_eq!(arms[0].1, Expr::int_lit(10));
    assert_eq!(default.as_deref(), Some(&Expr::int_lit(30)));
}

#[test]
fn test_prune_match_drops_shadowed_patterns_from_later_arm() {
    let program = vec![Stmt::assign(
        "x",
        Expr::new(
            ExprKind::Match {
                subject: Box::new(Expr::var("value")),
                arms: vec![
                    (vec![Expr::int_lit(1)], Expr::int_lit(10)),
                    (
                        vec![Expr::int_lit(1), Expr::int_lit(2)],
                        Expr::int_lit(20),
                    ),
                ],
                default: Some(Box::new(Expr::int_lit(30))),
            },
            Span::dummy(),
        ),
    )];

    let pruned = prune_constant_control_flow(program);

    let StmtKind::Assign { value, .. } = &pruned[0].kind else {
        panic!("expected assign");
    };
    let ExprKind::Match { arms, default, .. } = &value.kind else {
        panic!("expected match");
    };
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].0, vec![Expr::int_lit(1)]);
    assert_eq!(arms[1].0, vec![Expr::int_lit(2)]);
    assert_eq!(arms[1].1, Expr::int_lit(20));
    assert_eq!(default.as_deref(), Some(&Expr::int_lit(30)));
}

#[test]
fn test_prune_switch_drops_leading_non_matching_cases() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(3),
            cases: vec![
                (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(10))]),
                (
                    vec![Expr::int_lit(3)],
                    vec![Stmt::echo(Expr::int_lit(20)), Stmt::new(StmtKind::Break, Span::dummy())],
                ),
            ],
            default: Some(vec![Stmt::echo(Expr::int_lit(30))]),
        },
        Span::dummy(),
    )];

    let pruned = prune_constant_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(20))]);
}

#[test]
fn test_eliminate_dead_code_drops_statements_after_exhaustive_try_catch() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::new(
                            StmtKind::If {
                                condition: Expr::var("flag"),
                                then_body: vec![Stmt::new(
                                    StmtKind::Throw(Expr::string_lit("boom")),
                                    Span::dummy(),
                                )],
                                elseif_clauses: Vec::new(),
                                else_body: Some(vec![Stmt::new(
                                    StmtKind::Return(Some(Expr::int_lit(7))),
                                    Span::dummy(),
                                )]),
                            },
                            Span::dummy(),
                        )],
                        catches: vec![crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(8))),
                                Span::dummy(),
                            )],
                        }],
                        finally_body: None,
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(normalize_control_flow(program));

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(body[0].kind, StmtKind::Try { .. }));
}

#[test]
fn test_eliminate_dead_code_drops_statements_after_try_finally_exit() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        catches: Vec::new(),
                        finally_body: Some(vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(8))),
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(body[0].kind, StmtKind::Try { .. }));
}

#[test]
fn test_eliminate_dead_code_keeps_statements_after_fallthrough_try() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::echo(Expr::int_lit(7))],
                        catches: vec![crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(8))),
                                Span::dummy(),
                            )],
                        }],
                        finally_body: None,
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 2);
    assert_eq!(body[1], Stmt::echo(Expr::int_lit(9)));
}

#[test]
fn test_eliminate_dead_code_drops_empty_switch_shell_created_by_branch_dce() {
    let touch = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let pure_builtin = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strlen"),
            args: vec![Expr::string_lit("abc")],
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: touch.clone(),
                    cases: vec![(
                        vec![Expr::int_lit(1)],
                        vec![
                            Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy()),
                            Stmt::new(StmtKind::Break, Span::dummy()),
                        ],
                    )],
                    default: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert_eq!(
        body[0],
        Stmt::new(StmtKind::ExprStmt(touch), Span::dummy()),
    );
}

#[test]
fn test_eliminate_dead_code_drops_empty_try_shell_created_by_branch_dce() {
    let pure_builtin = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strlen"),
            args: vec![Expr::string_lit("abc")],
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin.clone()), Span::dummy())],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec!["Exception".into()],
                        variable: Some("e".into()),
                        body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())],
                    }],
                    finally_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert!(body.is_empty());
}

#[test]
fn test_eliminate_dead_code_drops_unreachable_catches_after_non_throwing_try() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::echo(Expr::int_lit(7))],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec!["Exception".into()],
                        variable: Some("e".into()),
                        body: vec![Stmt::echo(Expr::int_lit(9))],
                    }],
                    finally_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_drops_unreachable_catches_before_finally() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::echo(Expr::int_lit(7))],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec!["Exception".into()],
                        variable: Some("e".into()),
                        body: vec![Stmt::echo(Expr::int_lit(9))],
                    }],
                    finally_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(8))]);
}

#[test]
fn test_eliminate_dead_code_drops_catches_shadowed_by_throwable() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::Throw(Expr::string_lit("boom")),
                        Span::dummy(),
                    )],
                    catches: vec![
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Throwable".into()],
                            variable: Some("t".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::echo(Expr::int_lit(8))],
                        },
                    ],
                    finally_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::Try { catches, .. } = &body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(catches[0].exception_types.len(), 1);
    assert_eq!(catches[0].exception_types[0].as_str(), "Throwable");
    assert_eq!(catches[0].variable.as_deref(), Some("t"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_drops_duplicate_shadowed_catch_types() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::Throw(Expr::string_lit("boom")),
                        Span::dummy(),
                    )],
                    catches: vec![
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("first".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("second".into()),
                            body: vec![Stmt::echo(Expr::int_lit(8))],
                        },
                    ],
                    finally_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::Try { catches, .. } = &body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(catches[0].exception_types.len(), 1);
    assert_eq!(catches[0].exception_types[0].as_str(), "Exception");
    assert_eq!(catches[0].variable.as_deref(), Some("first"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_merges_identical_catches_exposed_by_shadow_drop() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::Throw(Expr::string_lit("boom")),
                        Span::dummy(),
                    )],
                    catches: vec![
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("shadowed".into()),
                            body: vec![Stmt::echo(Expr::int_lit(8))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Error".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                    ],
                    finally_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::Try { catches, .. } = &body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(catches[0].variable.as_deref(), Some("e"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(catches[0].exception_types.len(), 2);
    assert_eq!(catches[0].exception_types[0].as_str(), "Error");
    assert_eq!(catches[0].exception_types[1].as_str(), "Exception");
}

#[test]
fn test_eliminate_dead_code_drops_switch_case_shadowed_by_terminating_duplicate_pattern() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::var("x"),
                    cases: vec![
                        (
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::int_lit(7)),
                                Stmt::new(StmtKind::Break, Span::dummy()),
                            ],
                        ),
                        (
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::int_lit(8)),
                                Stmt::new(StmtKind::Break, Span::dummy()),
                            ],
                        ),
                    ],
                    default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, vec![Expr::int_lit(1)]);
    assert_eq!(
        cases[0].1,
        vec![
            Stmt::echo(Expr::int_lit(7)),
            Stmt::new(StmtKind::Break, Span::dummy()),
        ]
    );
    assert_eq!(default, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_merges_fallthrough_body_from_fully_shadowed_switch_case() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::var("x"),
                    cases: vec![
                        (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
                        (
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::int_lit(8)),
                                Stmt::new(StmtKind::Break, Span::dummy()),
                            ],
                        ),
                    ],
                    default: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, vec![Expr::int_lit(1)]);
    assert_eq!(
        cases[0].1,
        vec![
            Stmt::echo(Expr::int_lit(7)),
            Stmt::echo(Expr::int_lit(8)),
            Stmt::new(StmtKind::Break, Span::dummy()),
        ]
    );
    assert_eq!(default, &None);
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("flag"),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::new(
                                ExprKind::Not(Box::new(Expr::var("flag"))),
                                Span::dummy(),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_invalidates_outer_guard_after_local_write() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("flag"),
                    then_body: vec![
                        Stmt::assign("flag", Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                        Stmt::new(
                            StmtKind::If {
                                condition: Expr::var("flag"),
                                then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                elseif_clauses: Vec::new(),
                                else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                            },
                            Span::dummy(),
                        ),
                    ],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    let StmtKind::If { .. } = &then_body[1].kind else {
        panic!("expected inner if to remain after guard invalidation");
    };
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_strict_bool_guard() {
    let strict_true = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
        },
        Span::dummy(),
    );
    let strict_false = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: strict_true,
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: strict_false,
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_invalidates_outer_strict_bool_guard_after_local_write() {
    let strict_true = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: strict_true.clone(),
                    then_body: vec![
                        Stmt::assign("flag", Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                        Stmt::new(
                            StmtKind::If {
                                condition: strict_true,
                                then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                elseif_clauses: Vec::new(),
                                else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                            },
                            Span::dummy(),
                        ),
                    ],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    let StmtKind::If { .. } = &then_body[1].kind else {
        panic!("expected strict inner if to remain after guard invalidation");
    };
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_and_guard() {
    let contradiction = Expr::binop(
        Expr::new(ExprKind::Not(Box::new(Expr::var("a"))), Span::dummy()),
        BinOp::Or,
        Expr::new(ExprKind::Not(Box::new(Expr::var("b"))), Span::dummy()),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b")),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: contradiction,
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_or_false_branch() {
    let outer = Expr::binop(
        Expr::new(ExprKind::Not(Box::new(Expr::var("a"))), Span::dummy()),
        BinOp::Or,
        Expr::var("b"),
    );
    let inner = Expr::binop(
        Expr::var("a"),
        BinOp::And,
        Expr::new(ExprKind::Not(Box::new(Expr::var("b"))), Span::dummy()),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: outer,
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: inner,
                            then_body: vec![Stmt::echo(Expr::int_lit(7))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                        },
                        Span::dummy(),
                    )]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_null_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("value"),
                        BinOp::StrictEq,
                        Expr::new(ExprKind::Null, Span::dummy()),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("value"),
                                BinOp::StrictNotEq,
                                Expr::new(ExprKind::Null, Span::dummy()),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_zero_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::int_lit(0)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("value"),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_zero_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::int_lit(0)),
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::int_lit(0)),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_null_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("value"),
                        BinOp::StrictNotEq,
                        Expr::new(ExprKind::Null, Span::dummy()),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("value"),
                                BinOp::StrictEq,
                                Expr::new(ExprKind::Null, Span::dummy()),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_empty_string_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("value"),
                        BinOp::StrictEq,
                        Expr::string_lit(""),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("value"),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_empty_string_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("value"),
                        BinOp::StrictEq,
                        Expr::string_lit(""),
                    ),
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("value"),
                                BinOp::StrictEq,
                                Expr::string_lit(""),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_string_zero_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("value"),
                        BinOp::StrictEq,
                        Expr::string_lit("0"),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("value"),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_string_zero_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("value"),
                        BinOp::StrictEq,
                        Expr::string_lit("0"),
                    ),
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("value"),
                                BinOp::StrictEq,
                                Expr::string_lit("0"),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_zero_float_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::float_lit(0.0)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("value"),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_float_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::float_lit(1.5)),
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::float_lit(1.5)),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_switch_bool_guard_case() {
    let strict_true = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
        },
        Span::dummy(),
    );
    let strict_false = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                    cases: vec![(
                        vec![strict_true],
                        vec![
                            Stmt::new(
                                StmtKind::If {
                                    condition: strict_false,
                                    then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                    elseif_clauses: Vec::new(),
                                    else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                                },
                                Span::dummy(),
                            ),
                            Stmt::new(StmtKind::Break, Span::dummy()),
                        ],
                    )],
                    default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(
        cases[0].1,
        vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())]
    );
    assert_eq!(default, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_drops_impossible_switch_cases_from_outer_exact_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::int_lit(0)),
                    then_body: vec![Stmt::new(
                        StmtKind::Switch {
                            subject: Expr::var("value"),
                            cases: vec![
                                (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
                                (vec![Expr::int_lit(0)], vec![Stmt::echo(Expr::int_lit(8))]),
                            ],
                            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    let StmtKind::Switch { cases, .. } = &then_body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, vec![Expr::int_lit(0)]);
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
}

#[test]
fn test_eliminate_dead_code_drops_impossible_switch_true_cases_from_outer_guard() {
    let strict_true = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
        },
        Span::dummy(),
    );
    let strict_false = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: strict_true.clone(),
                    then_body: vec![Stmt::new(
                        StmtKind::Switch {
                            subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                            cases: vec![
                                (vec![strict_false], vec![Stmt::echo(Expr::int_lit(7))]),
                                (vec![strict_true], vec![Stmt::echo(Expr::int_lit(8))]),
                            ],
                            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    let StmtKind::Switch { cases, .. } = &then_body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
}

#[test]
fn test_eliminate_dead_code_invalidates_switch_bool_guard_after_local_write() {
    let strict_true = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                    cases: vec![(
                        vec![strict_true.clone()],
                        vec![
                            Stmt::assign("flag", Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                            Stmt::new(
                                StmtKind::If {
                                    condition: strict_true,
                                    then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                    elseif_clauses: Vec::new(),
                                    else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                                },
                                Span::dummy(),
                            ),
                            Stmt::new(StmtKind::Break, Span::dummy()),
                        ],
                    )],
                    default: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::Switch { cases, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    let StmtKind::If { .. } = &cases[0].1[1].kind else {
        panic!("expected inner if to remain after switch guard invalidation");
    };
}

#[test]
fn test_eliminate_dead_code_invalidates_outer_guard_before_catch_body() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("flag"),
                    then_body: vec![Stmt::new(
                        StmtKind::Try {
                            try_body: vec![
                                Stmt::assign("flag", Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                                Stmt::new(
                                    StmtKind::Throw(Expr::new(
                                        ExprKind::NewObject {
                                            class_name: Name::unqualified("Exception"),
                                            args: vec![Expr::string_lit("boom")],
                                        },
                                        Span::dummy(),
                                    )),
                                    Span::dummy(),
                                ),
                            ],
                            catches: vec![crate::parser::ast::CatchClause {
                                exception_types: vec![Name::unqualified("Exception")],
                                variable: Some("e".into()),
                                body: vec![Stmt::new(
                                    StmtKind::If {
                                        condition: Expr::var("flag"),
                                        then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                        elseif_clauses: Vec::new(),
                                        else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                                    },
                                    Span::dummy(),
                                )],
                            }],
                            finally_body: None,
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    let StmtKind::Try { catches, .. } = &then_body[0].kind else {
        panic!("expected try");
    };
    let StmtKind::If { .. } = &catches[0].body[0].kind else {
        panic!("expected catch inner if to remain after try write invalidation");
    };
}

#[test]
fn test_eliminate_dead_code_preserves_outer_guard_for_catch_when_only_non_throw_path_writes() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("flag"),
                    then_body: vec![Stmt::new(
                        StmtKind::Try {
                            try_body: vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::var("other"),
                                    then_body: vec![Stmt::assign(
                                        "flag",
                                        Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                                    )],
                                    elseif_clauses: Vec::new(),
                                    else_body: Some(vec![Stmt::new(
                                        StmtKind::Throw(Expr::new(
                                            ExprKind::NewObject {
                                                class_name: Name::unqualified("Exception"),
                                                args: vec![Expr::string_lit("boom")],
                                            },
                                            Span::dummy(),
                                        )),
                                        Span::dummy(),
                                    )]),
                                },
                                Span::dummy(),
                            )],
                            catches: vec![crate::parser::ast::CatchClause {
                                exception_types: vec![Name::unqualified("Exception")],
                                variable: Some("e".into()),
                                body: vec![Stmt::new(
                                    StmtKind::If {
                                        condition: Expr::var("flag"),
                                        then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                        elseif_clauses: Vec::new(),
                                        else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                                    },
                                    Span::dummy(),
                                )],
                            }],
                            finally_body: None,
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    let StmtKind::Try { catches, .. } = &then_body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_preserves_outer_guard_for_finally_when_only_other_locals_change() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("flag"),
                    then_body: vec![Stmt::new(
                        StmtKind::Try {
                            try_body: vec![Stmt::assign("other", Expr::int_lit(1))],
                            catches: Vec::new(),
                            finally_body: Some(vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::var("flag"),
                                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                    elseif_clauses: Vec::new(),
                                    else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                                },
                                Span::dummy(),
                            )]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(
        then_body,
        &vec![Stmt::assign("other", Expr::int_lit(1)), Stmt::echo(Expr::int_lit(7))]
    );
}

#[test]
fn test_eliminate_dead_code_reduces_empty_if_chain_to_needed_condition_checks() {
    let touch = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let tap = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("tap"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let pure_builtin = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strlen"),
            args: vec![Expr::string_lit("abc")],
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: touch.clone(),
                    then_body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin.clone()), Span::dummy())],
                    elseif_clauses: vec![(
                        tap.clone(),
                        vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())],
                    )],
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(ExprKind::Not(Box::new(touch)), Span::dummy()),
                then_body: vec![Stmt::new(StmtKind::ExprStmt(tap), Span::dummy())],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_drops_trailing_empty_switch_cases() {
    let touch = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let pure_builtin = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strlen"),
            args: vec![Expr::string_lit("abc")],
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: touch.clone(),
                    cases: vec![
                        (
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::int_lit(7)),
                                Stmt::new(StmtKind::Break, Span::dummy()),
                            ],
                        ),
                        (
                            vec![Expr::int_lit(2)],
                            vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())],
                        ),
                    ],
                    default: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::Switch {
                subject: touch,
                cases: vec![(
                    vec![Expr::int_lit(1)],
                    vec![
                        Stmt::echo(Expr::int_lit(7)),
                        Stmt::new(StmtKind::Break, Span::dummy()),
                    ],
                )],
                default: None,
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_rebuilds_empty_elseif_tail_as_needed_guard() {
    let touch = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let tap = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("tap"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let pure_builtin = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strlen"),
            args: vec![Expr::string_lit("abc")],
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: touch.clone(),
                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                    elseif_clauses: vec![(
                        tap.clone(),
                        vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())],
                    )],
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: touch,
                then_body: vec![Stmt::echo(Expr::int_lit(7))],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::new(
                    StmtKind::If {
                        condition: Expr::new(ExprKind::Not(Box::new(tap)), Span::dummy()),
                        then_body: vec![Stmt::echo(Expr::int_lit(9))],
                        elseif_clauses: Vec::new(),
                        else_body: None,
                    },
                    Span::dummy(),
                )]),
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_if_fallthrough_branch() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        elseif_clauses: Vec::new(),
                        else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(7))),
                    Span::dummy(),
                )],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::echo(Expr::int_lit(8)), Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_implicit_else_path() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        elseif_clauses: Vec::new(),
                        else_body: None,
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(7))),
                    Span::dummy(),
                )],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_switch_exit_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Switch {
                        subject: Expr::var("flag"),
                        cases: vec![
                            (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
                            (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
                        ],
                        default: Some(vec![Stmt::echo(Expr::int_lit(6))]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::Switch {
                subject: Expr::var("flag"),
                cases: vec![
                    (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
                    (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
                ],
                default: Some(vec![Stmt::echo(Expr::int_lit(6)), Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_try_fallthrough_paths() {
    let may_throw = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("may_throw"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![
                            Stmt::new(StmtKind::ExprStmt(may_throw), Span::dummy()),
                            Stmt::echo(Expr::int_lit(7)),
                        ],
                        catches: vec![crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(8))),
                                Span::dummy(),
                            )],
                        }],
                        finally_body: None,
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![
                    Stmt::new(
                        StmtKind::ExprStmt(Expr::new(
                            ExprKind::FunctionCall {
                                name: Name::unqualified("may_throw"),
                                args: Vec::new(),
                            },
                            Span::dummy(),
                        )),
                        Span::dummy(),
                    ),
                    Stmt::echo(Expr::int_lit(7)),
                    Stmt::echo(Expr::int_lit(9)),
                ],
                catches: vec![crate::parser::ast::CatchClause {
                    exception_types: vec!["Exception".into()],
                    variable: Some("e".into()),
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::int_lit(8))),
                        Span::dummy(),
                    )],
                }],
                finally_body: None,
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_safe_finally_path() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::echo(Expr::int_lit(7))],
                        catches: Vec::new(),
                        finally_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(8)), Stmt::echo(Expr::int_lit(9))]);
}

#[test]
fn test_switch_tail_reachability_tracks_suffix_paths() {
    let cases = vec![
        (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
        (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
    ];
    let default = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let reachability = analyze_switch_tail_paths(&cases, &default);

    assert_eq!(
        reachability.case_tail_paths,
        vec![TailPathKind::FallsThrough, TailPathKind::FallsThrough]
    );
    assert_eq!(reachability.default_tail_path, Some(TailPathKind::FallsThrough));
}

#[test]
fn test_build_switch_cfg_tracks_case_successors() {
    let cases = vec![
        (vec![Expr::int_lit(1)], Vec::new()),
        (
            vec![Expr::int_lit(2)],
            vec![Stmt::new(StmtKind::Break, Span::dummy())],
        ),
    ];
    let default = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let cfg = build_switch_cfg(&cases, &default);

    assert_eq!(cfg.case_entries, vec![0, 1]);
    assert_eq!(cfg.default_entry, Some(2));
    assert_eq!(
        cfg.blocks,
        vec![
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Block(1)],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Breaks],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::FallsThrough],
            },
        ]
    );
}

#[test]
fn test_classify_switch_cfg_paths_follows_fallthrough_chain() {
    let cases = vec![
        (vec![Expr::int_lit(1)], Vec::new()),
        (vec![Expr::int_lit(2)], Vec::new()),
    ];
    let default = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let cfg = build_switch_cfg(&cases, &default);

    assert_eq!(
        classify_switch_cfg_paths(&cfg),
        vec![
            BasicBlockSuccessor::FallsThrough,
            BasicBlockSuccessor::FallsThrough,
        ]
    );
    assert_eq!(
        classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(cfg.default_entry.unwrap())),
        BasicBlockSuccessor::FallsThrough
    );
}

#[test]
fn test_switch_tail_reachability_tracks_break_and_fallthrough_paths() {
    let cases = vec![
        (
            vec![Expr::int_lit(1)],
            vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())],
        ),
        (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
    ];
    let default = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let reachability = analyze_switch_tail_paths(&cases, &default);

    assert_eq!(
        reachability.case_tail_paths,
        vec![TailPathKind::Breaks, TailPathKind::FallsThrough]
    );
    assert_eq!(reachability.default_tail_path, Some(TailPathKind::FallsThrough));
}

#[test]
fn test_switch_tail_reachability_marks_mixed_break_paths_unknown() {
    let cases = vec![(
        vec![Expr::int_lit(1)],
        vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::new(StmtKind::Break, Span::dummy())],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(7))),
                    Span::dummy(),
                )]),
            },
            Span::dummy(),
        )],
    )];

    let reachability = analyze_switch_tail_paths(&cases, &None);

    assert_eq!(reachability.case_tail_paths, vec![TailPathKind::Unknown]);
    assert_eq!(reachability.default_tail_path, None);
}

#[test]
fn test_if_tail_reachability_tracks_fallthrough_and_implicit_else() {
    let elseif_clauses = vec![
        (
            Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
            vec![Stmt::new(StmtKind::Return(Some(Expr::int_lit(7))), Span::dummy())],
        ),
        (
            Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
            vec![Stmt::echo(Expr::int_lit(8))],
        ),
    ];

    let reachability = analyze_if_tail_paths(
        &[Stmt::new(StmtKind::Return(Some(Expr::int_lit(1))), Span::dummy())],
        &elseif_clauses,
        &None,
    );

    assert!(!reachability.then_sinks_tail);
    assert_eq!(reachability.elseif_sinks_tail, vec![false, true]);
    assert!(!reachability.else_sinks_tail);
    assert!(reachability.implicit_else_sinks_tail);
}

#[test]
fn test_build_if_cfg_tracks_condition_and_body_successors() {
    let elseif_clauses = vec![(
        Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
        vec![Stmt::new(StmtKind::Return(Some(Expr::int_lit(7))), Span::dummy())],
    )];
    let else_body = Some(vec![Stmt::echo(Expr::int_lit(9))]);

    let cfg = build_if_cfg(
        &[Stmt::echo(Expr::int_lit(1))],
        &elseif_clauses,
        &else_body,
    );

    assert_eq!(cfg.body_entries, vec![2, 3]);
    assert_eq!(cfg.else_entry, Some(4));
    assert_eq!(cfg.implicit_else_successor, BasicBlockSuccessor::Unknown);
    assert_eq!(
        cfg.blocks,
        vec![
            BasicBlock {
                successors: vec![
                    BasicBlockSuccessor::Block(2),
                    BasicBlockSuccessor::Block(1),
                ],
            },
            BasicBlock {
                successors: vec![
                    BasicBlockSuccessor::Block(3),
                    BasicBlockSuccessor::Block(4),
                ],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::FallsThrough],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Exits],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::FallsThrough],
            },
        ]
    );
}

#[test]
fn test_classify_if_cfg_paths_tracks_branch_bodies() {
    let elseif_clauses = vec![(
        Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
        vec![Stmt::echo(Expr::int_lit(8))],
    )];

    let cfg = build_if_cfg(
        &[Stmt::new(StmtKind::Return(Some(Expr::int_lit(1))), Span::dummy())],
        &elseif_clauses,
        &None,
    );

    assert_eq!(
        classify_if_cfg_paths(&cfg),
        vec![BasicBlockSuccessor::Exits, BasicBlockSuccessor::FallsThrough]
    );
}

#[test]
fn test_ifdef_tail_reachability_tracks_implicit_else() {
    let reachability = analyze_ifdef_tail_paths(
        &[Stmt::echo(Expr::int_lit(7))],
        &Some(vec![Stmt::new(
            StmtKind::Return(Some(Expr::int_lit(8))),
            Span::dummy(),
        )]),
    );

    assert!(reachability.then_sinks_tail);
    assert!(!reachability.else_sinks_tail);
    assert!(!reachability.implicit_else_sinks_tail);
}

#[test]
fn test_try_tail_reachability_prefers_finally_only_when_safe() {
    let safe_try = vec![Stmt::echo(Expr::int_lit(7))];
    let safe_finally = Some(vec![Stmt::echo(Expr::int_lit(8))]);

    let safe = analyze_try_tail_paths(&safe_try, &Vec::new(), &safe_finally);
    assert_eq!(safe.try_tail_path, TailPathKind::FallsThrough);
    assert_eq!(safe.finally_tail_path, Some(TailPathKind::FallsThrough));
    assert!(safe.can_sink_into_finally);

    let catch_body = vec![crate::parser::ast::CatchClause {
        exception_types: vec!["Exception".into()],
        variable: Some("e".into()),
        body: vec![Stmt::new(StmtKind::Return(Some(Expr::int_lit(9))), Span::dummy())],
    }];
    let with_catch = analyze_try_tail_paths(&safe_try, &catch_body, &safe_finally);
    assert_eq!(with_catch.try_tail_path, TailPathKind::FallsThrough);
    assert_eq!(with_catch.catch_tail_paths, vec![TailPathKind::NoTail]);
    assert_eq!(with_catch.finally_tail_path, Some(TailPathKind::FallsThrough));
    assert!(!with_catch.can_sink_into_finally);
}

#[test]
fn test_build_try_cfg_tracks_try_catch_and_finally_successors() {
    let catches = vec![
        crate::parser::ast::CatchClause {
            exception_types: vec!["Exception".into()],
            variable: Some("e".into()),
            body: vec![Stmt::new(StmtKind::Break, Span::dummy())],
        },
        crate::parser::ast::CatchClause {
            exception_types: vec!["RuntimeException".into()],
            variable: Some("e".into()),
            body: vec![Stmt::new(
                StmtKind::Return(Some(Expr::int_lit(9))),
                Span::dummy(),
            )],
        },
    ];
    let finally_body = Some(vec![Stmt::echo(Expr::int_lit(10))]);

    let cfg = build_try_cfg(&[Stmt::echo(Expr::int_lit(7))], &catches, &finally_body);

    assert_eq!(cfg.try_entry, 0);
    assert_eq!(cfg.catch_entries, vec![1, 2]);
    assert_eq!(cfg.finally_entry, Some(3));
    assert_eq!(
        cfg.blocks,
        vec![
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Block(3)],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Breaks],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::Exits],
            },
            BasicBlock {
                successors: vec![BasicBlockSuccessor::FallsThrough],
            },
        ]
    );
}

#[test]
fn test_classify_try_cfg_paths_tracks_try_and_catch_bodies() {
    let catches = vec![
        crate::parser::ast::CatchClause {
            exception_types: vec!["Exception".into()],
            variable: Some("e".into()),
            body: vec![Stmt::echo(Expr::int_lit(8))],
        },
        crate::parser::ast::CatchClause {
            exception_types: vec!["RuntimeException".into()],
            variable: Some("e".into()),
            body: vec![Stmt::new(
                StmtKind::Return(Some(Expr::int_lit(9))),
                Span::dummy(),
            )],
        },
    ];
    let finally_body = Some(vec![Stmt::echo(Expr::int_lit(10))]);

    let cfg = build_try_cfg(&[Stmt::echo(Expr::int_lit(7))], &catches, &finally_body);

    assert_eq!(
        classify_try_cfg_paths(&cfg),
        vec![
            BasicBlockSuccessor::FallsThrough,
            BasicBlockSuccessor::FallsThrough,
            BasicBlockSuccessor::Exits,
        ]
    );
    assert_eq!(
        classify_cfg_successor(&cfg.blocks, BasicBlockSuccessor::Block(cfg.finally_entry.unwrap())),
        BasicBlockSuccessor::FallsThrough
    );
}

#[test]
fn test_try_tail_reachability_tracks_catch_fallthrough_without_finally() {
    let catches = vec![
        crate::parser::ast::CatchClause {
            exception_types: vec!["Exception".into()],
            variable: Some("e".into()),
            body: vec![Stmt::echo(Expr::int_lit(8))],
        },
        crate::parser::ast::CatchClause {
            exception_types: vec!["RuntimeException".into()],
            variable: Some("e".into()),
            body: vec![Stmt::new(StmtKind::Return(Some(Expr::int_lit(9))), Span::dummy())],
        },
    ];

    let reachability = analyze_try_tail_paths(
        &[Stmt::echo(Expr::int_lit(7))],
        &catches,
        &None,
    );

    assert_eq!(reachability.try_tail_path, TailPathKind::FallsThrough);
    assert_eq!(
        reachability.catch_tail_paths,
        vec![TailPathKind::FallsThrough, TailPathKind::NoTail]
    );
    assert_eq!(reachability.finally_tail_path, None);
    assert!(!reachability.can_sink_into_finally);
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_ifdef_fallthrough_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::IfDef {
                        symbol: "DEBUG".into(),
                        then_body: vec![Stmt::echo(Expr::int_lit(7))],
                        else_body: Some(vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(8))),
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::IfDef {
                symbol: "DEBUG".into(),
                then_body: vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(9))],
                else_body: Some(vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(8))),
                    Span::dummy(),
                )]),
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_switch_break_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Switch {
                        subject: Expr::var("flag"),
                        cases: vec![
                            (
                                vec![Expr::int_lit(1)],
                                vec![
                                    Stmt::echo(Expr::int_lit(7)),
                                    Stmt::new(StmtKind::Break, Span::dummy()),
                                ],
                            ),
                            (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
                        ],
                        default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(10)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(
        cases[0].1,
        vec![
            Stmt::echo(Expr::int_lit(7)),
            Stmt::echo(Expr::int_lit(10)),
            Stmt::new(StmtKind::Break, Span::dummy()),
        ]
    );
    assert_eq!(
        cases[1].1,
        vec![Stmt::echo(Expr::int_lit(8))]
    );
    assert_eq!(
        default.as_ref(),
        Some(&vec![Stmt::echo(Expr::int_lit(9)), Stmt::echo(Expr::int_lit(10))])
    );
    assert_eq!(body.len(), 1);
}

#[test]
fn test_eliminate_dead_code_reduces_empty_if_to_effectful_condition_eval() {
    let touch = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let pure_builtin = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strlen"),
            args: vec![Expr::string_lit("abc")],
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: touch.clone(),
                    then_body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin.clone()), Span::dummy())],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())]),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert_eq!(
        body[0],
        Stmt::new(StmtKind::ExprStmt(touch), Span::dummy()),
    );
}

#[test]
fn test_normalize_control_flow_replaces_empty_if_with_effectful_condition_eval() {
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: call.clone(),
            then_body: Vec::new(),
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::new(StmtKind::ExprStmt(call), Span::dummy())]
    );
}

#[test]
fn test_normalize_control_flow_drops_empty_ifdef_shell() {
    let program = vec![Stmt::new(
        StmtKind::IfDef {
            symbol: "DEBUG".into(),
            then_body: Vec::new(),
            else_body: Some(Vec::new()),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert!(pruned.is_empty());
}

#[test]
fn test_normalize_control_flow_replaces_empty_switch_with_subject_eval() {
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: call.clone(),
            cases: Vec::new(),
            default: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::new(StmtKind::ExprStmt(call), Span::dummy())]
    );
}

#[test]
fn test_normalize_control_flow_inlines_empty_try_finally_body() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: Vec::new(),
            catches: vec![crate::parser::ast::CatchClause {
                exception_types: vec![Name::unqualified("Exception")],
                variable: Some("e".into()),
                body: vec![Stmt::echo(Expr::int_lit(7))],
            }],
            finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(9))]);
}

#[test]
fn test_normalize_control_flow_inverts_single_live_else_branch() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("flag"),
            then_body: Vec::new(),
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(
                    ExprKind::Not(Box::new(Expr::var("flag"))),
                    Span::dummy(),
                ),
                then_body: vec![Stmt::echo(Expr::int_lit(7))],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_normalize_control_flow_inlines_default_only_switch() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("flag"),
            cases: Vec::new(),
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(9))]);
}

#[test]
fn test_normalize_control_flow_nests_elseif_chain_after_empty_head() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: Vec::new(),
            elseif_clauses: vec![(
                Expr::var("b"),
                vec![Stmt::echo(Expr::int_lit(7))],
            )],
            else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(
                    ExprKind::Not(Box::new(Expr::var("a"))),
                    Span::dummy(),
                ),
                then_body: vec![Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("b"),
                        then_body: vec![Stmt::echo(Expr::int_lit(7))],
                        elseif_clauses: Vec::new(),
                        else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                    },
                    Span::dummy(),
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_normalize_control_flow_canonicalizes_elseif_chain_into_nested_else_if() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: vec![Stmt::echo(Expr::int_lit(1))],
            elseif_clauses: vec![(
                Expr::var("b"),
                vec![Stmt::echo(Expr::int_lit(2))],
            )],
            else_body: Some(vec![Stmt::echo(Expr::int_lit(3))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::If {
        condition,
        then_body,
        elseif_clauses,
        else_body,
    } = &pruned[0].kind
    else {
        panic!("expected if");
    };
    assert_eq!(*condition, Expr::var("a"));
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(1))]);
    assert!(elseif_clauses.is_empty());

    let else_body = else_body.as_ref().expect("expected nested else body");
    assert_eq!(else_body.len(), 1);
    let StmtKind::If {
        condition,
        then_body,
        elseif_clauses,
        else_body,
    } = &else_body[0].kind
    else {
        panic!("expected nested if");
    };
    assert_eq!(*condition, Expr::var("b"));
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(2))]);
    assert!(elseif_clauses.is_empty());
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(3))]));
}

#[test]
fn test_normalize_control_flow_merges_identical_if_chain_bodies_into_or_condition() {
    let shared_body = vec![Stmt::echo(Expr::int_lit(7))];
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: shared_body.clone(),
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("b"),
                    then_body: shared_body.clone(),
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                },
                Span::dummy(),
            )]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            assert_eq!(
                *condition,
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::var("a")),
                        op: BinOp::Or,
                        right: Box::new(Expr::var("b")),
                    },
                    Span::dummy(),
                )
            );
            assert_eq!(then_body, &shared_body);
            assert!(elseif_clauses.is_empty());
            assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_merges_identical_if_chain_tail_into_inverted_and() {
    let shared_tail = vec![Stmt::echo(Expr::int_lit(9))];
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: shared_tail.clone(),
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("b"),
                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(shared_tail.clone()),
                },
                Span::dummy(),
            )]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            assert_eq!(
                *condition,
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::new(
                            ExprKind::Not(Box::new(Expr::var("a"))),
                            Span::dummy(),
                        )),
                        op: BinOp::And,
                        right: Box::new(Expr::var("b")),
                    },
                    Span::dummy(),
                )
            );
            assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
            assert!(elseif_clauses.is_empty());
            assert_eq!(else_body, &Some(shared_tail));
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_recursively_merges_longer_if_chain_heads() {
    let shared_body = vec![Stmt::echo(Expr::int_lit(7))];
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: shared_body.clone(),
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("b"),
                    then_body: shared_body.clone(),
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("c"),
                            then_body: shared_body.clone(),
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                        },
                        Span::dummy(),
                    )]),
                },
                Span::dummy(),
            )]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            assert_eq!(
                *condition,
                combine_if_chain_conditions(
                    Expr::var("a"),
                    combine_if_chain_conditions(Expr::var("b"), Expr::var("c")),
                )
            );
            assert_eq!(then_body, &shared_body);
            assert!(elseif_clauses.is_empty());
            assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_materializes_constant_switch_match() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(2),
            cases: vec![
                (
                    vec![Expr::int_lit(1)],
                    vec![Stmt::echo(Expr::int_lit(5)), Stmt::new(StmtKind::Break, Span::dummy())],
                ),
                (
                    vec![Expr::int_lit(2)],
                    vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())],
                ),
            ],
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_normalize_control_flow_materializes_constant_switch_fallthrough() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(1),
            cases: vec![
                (vec![Expr::int_lit(1)], Vec::new()),
                (
                    vec![Expr::int_lit(2)],
                    vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())],
                ),
            ],
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_normalize_control_flow_materializes_constant_switch_default() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::int_lit(3),
            cases: vec![(
                vec![Expr::int_lit(1)],
                vec![Stmt::echo(Expr::int_lit(5)), Stmt::new(StmtKind::Break, Span::dummy())],
            )],
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(9))]);
}

#[test]
fn test_normalize_control_flow_rewrites_single_case_switch_to_if() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![(
                vec![Expr::int_lit(1)],
                vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())],
            )],
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            assert!(elseif_clauses.is_empty());
            assert_eq!(
                *condition,
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::var("x")),
                        op: BinOp::Eq,
                        right: Box::new(Expr::int_lit(1)),
                    },
                    Span::dummy(),
                )
            );
            assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
            assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_merges_adjacent_identical_switch_cases() {
    let shared_body = vec![
        Stmt::echo(Expr::int_lit(7)),
        Stmt::new(StmtKind::Break, Span::dummy()),
    ];
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![
                (vec![Expr::int_lit(1)], shared_body.clone()),
                (vec![Expr::int_lit(2)], shared_body.clone()),
                (
                    vec![Expr::int_lit(3)],
                    vec![Stmt::echo(Expr::int_lit(9)), Stmt::new(StmtKind::Break, Span::dummy())],
                ),
            ],
            default: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            assert_eq!(*subject, Expr::var("x"));
            assert_eq!(cases.len(), 2);
            assert_eq!(cases[0].0, vec![Expr::int_lit(1), Expr::int_lit(2)]);
            assert_eq!(cases[0].1, shared_body);
            assert_eq!(cases[1].0, vec![Expr::int_lit(3)]);
            assert_eq!(
                cases[1].1,
                vec![Stmt::echo(Expr::int_lit(9)), Stmt::new(StmtKind::Break, Span::dummy())]
            );
            assert!(default.is_none());
        }
        other => panic!("expected normalized switch, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_merges_fallthrough_switch_labels_into_next_case() {
    let shared_body = vec![
        Stmt::echo(Expr::int_lit(7)),
        Stmt::new(StmtKind::Break, Span::dummy()),
    ];
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("x"),
            cases: vec![
                (vec![Expr::int_lit(1)], Vec::new()),
                (vec![Expr::int_lit(2)], Vec::new()),
                (vec![Expr::int_lit(3)], shared_body.clone()),
            ],
            default: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            assert_eq!(
                *condition,
                combine_if_chain_conditions(
                    combine_if_chain_conditions(
                        Expr::new(
                            ExprKind::BinaryOp {
                                left: Box::new(Expr::var("x")),
                                op: BinOp::Eq,
                                right: Box::new(Expr::int_lit(1)),
                            },
                            Span::dummy(),
                        ),
                        Expr::new(
                            ExprKind::BinaryOp {
                                left: Box::new(Expr::var("x")),
                                op: BinOp::Eq,
                                right: Box::new(Expr::int_lit(2)),
                            },
                            Span::dummy(),
                        ),
                    ),
                    Expr::new(
                        ExprKind::BinaryOp {
                            left: Box::new(Expr::var("x")),
                            op: BinOp::Eq,
                            right: Box::new(Expr::int_lit(3)),
                        },
                        Span::dummy(),
                    ),
                )
            );
            assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
            assert!(elseif_clauses.is_empty());
            assert!(else_body.is_none());
        }
        other => panic!("expected normalized if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_merges_adjacent_identical_catches() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Throw(Box::new(Expr::new(
                        ExprKind::NewObject {
                            class_name: Name::unqualified("Exception"),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )),
                Span::dummy(),
            )],
            catches: vec![
                crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("A")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(7))],
                },
                crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("B")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(7))],
                },
            ],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::Try { catches, .. } = &pruned[0].kind else {
        panic!("expected normalized try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(
        catches[0].exception_types,
        vec![Name::unqualified("A"), Name::unqualified("B")]
    );
    assert_eq!(catches[0].variable.as_deref(), Some("e"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_normalize_control_flow_deduplicates_merged_catch_exception_types() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Throw(Box::new(Expr::new(
                        ExprKind::NewObject {
                            class_name: Name::unqualified("Exception"),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )),
                Span::dummy(),
            )],
            catches: vec![
                crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("A"), Name::unqualified("B")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(7))],
                },
                crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("B"), Name::unqualified("C")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(7))],
                },
            ],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::Try { catches, .. } = &pruned[0].kind else {
        panic!("expected normalized try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(
        catches[0].exception_types,
        vec![
            Name::unqualified("A"),
            Name::unqualified("B"),
            Name::unqualified("C")
        ]
    );
    assert_eq!(catches[0].variable.as_deref(), Some("e"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_normalize_control_flow_sorts_catch_exception_types() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Throw(Box::new(Expr::new(
                        ExprKind::NewObject {
                            class_name: Name::unqualified("Exception"),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )),
                Span::dummy(),
            )],
            catches: vec![crate::parser::ast::CatchClause {
                exception_types: vec![
                    Name::unqualified("Zed"),
                    Name::unqualified("Alpha"),
                    Name::unqualified("Mid"),
                ],
                variable: Some("e".into()),
                body: vec![Stmt::echo(Expr::int_lit(7))],
            }],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::Try { catches, .. } = &pruned[0].kind else {
        panic!("expected normalized try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(
        catches[0].exception_types,
        vec![
            Name::unqualified("Alpha"),
            Name::unqualified("Mid"),
            Name::unqualified("Zed")
        ]
    );
}

#[test]
fn test_normalize_control_flow_inlines_non_throwing_try_catch() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::echo(Expr::int_lit(7))],
            catches: vec![crate::parser::ast::CatchClause {
                exception_types: vec![Name::unqualified("Exception")],
                variable: Some("e".into()),
                body: vec![Stmt::echo(Expr::int_lit(9))],
            }],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_normalize_control_flow_inlines_non_throwing_try_finally_fallthrough() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::echo(Expr::int_lit(7))],
            catches: Vec::new(),
            finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(9))]);
}

#[test]
fn test_normalize_control_flow_keeps_non_throwing_try_finally_with_return() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::Return(Some(Expr::int_lit(7))),
                Span::dummy(),
            )],
            catches: Vec::new(),
            finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    assert!(matches!(pruned[0].kind, StmtKind::Try { .. }));
}

#[test]
fn test_normalize_control_flow_folds_outer_finally_into_single_inner_try() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::ExprStmt(Expr::new(
                            ExprKind::Throw(Box::new(Expr::new(
                                ExprKind::NewObject {
                                    class_name: Name::unqualified("A"),
                                    args: Vec::new(),
                                },
                                Span::dummy(),
                            ))),
                            Span::dummy(),
                        )),
                        Span::dummy(),
                    )],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec![Name::unqualified("A")],
                        variable: Some("e".into()),
                        body: vec![Stmt::echo(Expr::int_lit(7))],
                    }],
                    finally_body: None,
                },
                Span::dummy(),
            )],
            catches: Vec::new(),
            finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::Try {
        try_body,
        catches,
        finally_body,
    } = &pruned[0].kind
    else {
        panic!("expected normalized try");
    };
    assert_eq!(
        try_body,
        &vec![Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::Throw(Box::new(Expr::new(
                    ExprKind::NewObject {
                        class_name: Name::unqualified("A"),
                        args: Vec::new(),
                    },
                    Span::dummy(),
                ))),
                Span::dummy(),
            )),
            Span::dummy(),
        )]
    );
    assert_eq!(catches.len(), 1);
    assert_eq!(
        catches[0].exception_types,
        vec![Name::unqualified("A")]
    );
    assert_eq!(catches[0].variable.as_deref(), Some("e"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(finally_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_normalize_control_flow_hoists_non_throwing_try_prefix() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![
                Stmt::echo(Expr::int_lit(7)),
                Stmt::new(StmtKind::Throw(Expr::var("boom")), Span::dummy()),
            ],
            catches: vec![crate::parser::ast::CatchClause {
                exception_types: vec![Name::unqualified("Exception")],
                variable: Some("e".into()),
                body: vec![Stmt::echo(Expr::int_lit(9))],
            }],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 2);
    assert_eq!(pruned[0], Stmt::echo(Expr::int_lit(7)));
    assert!(matches!(pruned[1].kind, StmtKind::Try { .. }));
}

#[test]
fn test_normalize_control_flow_flattens_nested_single_path_ifs() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("b"),
                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    match &pruned[0].kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            assert!(elseif_clauses.is_empty());
            assert!(else_body.is_none());
            assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
            assert_eq!(
                *condition,
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::var("a")),
                        op: BinOp::And,
                        right: Box::new(Expr::var("b")),
                    },
                    Span::dummy(),
                )
            );
        }
        other => panic!("expected flattened if, got {:?}", other),
    }
}

#[test]
fn test_normalize_control_flow_collapses_identical_if_branches_to_condition_effects_plus_body() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("tick"),
                    args: Vec::new(),
                },
                Span::dummy(),
            ),
            then_body: vec![Stmt::echo(Expr::int_lit(7))],
            elseif_clauses: Vec::new(),
            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![
            Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::unqualified("tick"),
                        args: Vec::new(),
                    },
                    Span::dummy(),
                )),
                Span::dummy(),
            ),
            Stmt::echo(Expr::int_lit(7))
        ]
    );
}
