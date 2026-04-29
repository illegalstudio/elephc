use super::*;

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
