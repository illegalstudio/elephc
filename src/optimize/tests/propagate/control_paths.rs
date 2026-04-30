use super::*;

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
                        Stmt::new(StmtKind::Break(1), Span::dummy()),
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
fn test_propagate_constants_ignores_unreachable_catch_after_non_throwing_try() {
    let program = vec![
        Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::assign("base", Expr::int_lit(2))],
                catches: vec![crate::parser::ast::CatchClause {
                    exception_types: vec![Name::from("Exception")],
                    variable: Some("e".to_string()),
                    body: vec![Stmt::assign("base", Expr::int_lit(9))],
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
