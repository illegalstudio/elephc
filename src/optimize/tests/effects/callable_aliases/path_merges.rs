//! Purpose:
//! Regression tests for optimizer effects callable_aliases path_merges behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
fn test_effect_analysis_tracks_pure_iife_expr_calls() {
    let expr = Expr::new(
        ExprKind::ExprCall {
            callee: Box::new(Expr::new(
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
                                    Stmt::new(StmtKind::Break(1), Span::dummy()),
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
