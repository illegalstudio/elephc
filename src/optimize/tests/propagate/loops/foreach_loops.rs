use super::*;

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
