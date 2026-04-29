use super::*;

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
fn test_propagate_constants_tracks_known_match_assignment() {
    let program = vec![
        Stmt::assign("mode", Expr::int_lit(1)),
        Stmt::assign(
            "base",
            Expr::new(
                ExprKind::Match {
                    subject: Box::new(Expr::var("mode")),
                    arms: vec![(vec![Expr::int_lit(1)], Expr::int_lit(2))],
                    default: Some(Box::new(Expr::int_lit(9))),
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::binop(Expr::var("base"), BinOp::Pow, Expr::int_lit(3))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(propagated[1], Stmt::assign("base", Expr::int_lit(2)));
    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(ExprKind::FloatLiteral(8.0), Span::dummy()))
    );
}
