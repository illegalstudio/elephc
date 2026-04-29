use super::*;

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
        type_expr: None,
        readonly: false,
        is_final: false,
        is_static: false,
        by_ref: false,
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
            is_final: false,
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
