use super::*;

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
                                is_arrow: false,
                                is_static: false,
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
