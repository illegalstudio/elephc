use super::*;

#[test]
fn test_eliminate_dead_code_prunes_exhaustive_switch_true_default_from_cumulative_guards() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                    cases: vec![
                        (
                            vec![Expr::var("flag")],
                            vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                        ),
                        (
                            vec![Expr::new(
                                ExprKind::Not(Box::new(Expr::var("flag"))),
                                Span::dummy(),
                            )],
                            vec![Stmt::echo(Expr::int_lit(8)), Stmt::new(StmtKind::Break(1), Span::dummy())],
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
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].0, vec![Expr::var("flag")]);
    assert_eq!(
        cases[1].0,
        vec![Expr::new(
            ExprKind::Not(Box::new(Expr::var("flag"))),
            Span::dummy(),
        )]
    );
    assert!(default.is_none());
}

#[test]
fn test_eliminate_dead_code_uses_cumulative_switch_true_guards_inside_case_body() {
    let ab = Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b"));
    let composite = Expr::binop(
        Expr::binop(ab.clone(), BinOp::Or, Expr::var("c")),
        BinOp::And,
        Expr::var("d"),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("d"),
                    then_body: vec![Stmt::new(
                        StmtKind::Switch {
                            subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                            cases: vec![
                                (
                                    vec![composite],
                                    vec![Stmt::echo(Expr::int_lit(1)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                                ),
                                (
                                    vec![Expr::new(ExprKind::Not(Box::new(Expr::var("c"))), Span::dummy())],
                                    vec![
                                        Stmt::new(
                                            StmtKind::If {
                                                condition: ab,
                                                then_body: vec![Stmt::echo(Expr::int_lit(2))],
                                                elseif_clauses: Vec::new(),
                                                else_body: Some(vec![Stmt::echo(Expr::int_lit(3))]),
                                            },
                                            Span::dummy(),
                                        ),
                                        Stmt::new(StmtKind::Break(1), Span::dummy()),
                                    ],
                                ),
                            ],
                            default: Some(vec![Stmt::echo(Expr::int_lit(4))]),
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
    let StmtKind::Switch { cases, default, .. } = &then_body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[1].1, vec![Stmt::echo(Expr::int_lit(3)), Stmt::new(StmtKind::Break(1), Span::dummy())]);
    assert!(default.is_none());
}
