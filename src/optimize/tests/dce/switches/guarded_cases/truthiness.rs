use super::*;

#[test]
fn test_eliminate_dead_code_prunes_truthy_switch_cases_and_default() {
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
                        StmtKind::Switch {
                            subject: Expr::var("flag"),
                            cases: vec![
                                (
                                    vec![Expr::new(ExprKind::BoolLiteral(false), Span::dummy())],
                                    vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())],
                                ),
                                (
                                    vec![Expr::new(ExprKind::BoolLiteral(true), Span::dummy())],
                                    vec![Stmt::new(
                                        StmtKind::If {
                                            condition: Expr::var("flag"),
                                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                            elseif_clauses: Vec::new(),
                                            else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                                        },
                                        Span::dummy(),
                                    )],
                                ),
                            ],
                            default: Some(vec![Stmt::echo(Expr::int_lit(10))]),
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
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, vec![Expr::new(ExprKind::BoolLiteral(true), Span::dummy())]);
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
    assert!(default.is_none());
}

#[test]
fn test_eliminate_dead_code_prunes_falsy_scalar_labels_from_truthy_switch_subject() {
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
                        StmtKind::Switch {
                            subject: Expr::var("flag"),
                            cases: vec![
                                (
                                    vec![Expr::int_lit(0), Expr::string_lit("")],
                                    vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())],
                                ),
                                (
                                    vec![
                                        Expr::var("other"),
                                        Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                                    ],
                                    vec![Stmt::echo(Expr::int_lit(8))],
                                ),
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
    let StmtKind::Switch { cases, default, .. } = &then_body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(
        cases[0].0,
        vec![Expr::var("other"), Expr::new(ExprKind::BoolLiteral(true), Span::dummy())]
    );
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
    assert!(default.is_none());
}

#[test]
fn test_eliminate_dead_code_combines_exclusion_and_truthy_switch_guards() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("value"),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::new(
                                ExprKind::BinaryOp {
                                    left: Box::new(Expr::var("value")),
                                    op: BinOp::StrictNotEq,
                                    right: Box::new(Expr::int_lit(1)),
                                },
                                Span::dummy(),
                            ),
                            then_body: vec![Stmt::new(
                                StmtKind::Switch {
                                    subject: Expr::var("value"),
                                    cases: vec![
                                        (
                                            vec![Expr::int_lit(1), Expr::int_lit(0)],
                                            vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break, Span::dummy())],
                                        ),
                                        (
                                            vec![Expr::int_lit(2), Expr::new(ExprKind::BoolLiteral(true), Span::dummy())],
                                            vec![Stmt::echo(Expr::int_lit(8))],
                                        ),
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
        panic!("expected outer if");
    };
    let (cases, default) = match &then_body[0].kind {
        StmtKind::If { then_body, .. } => match &then_body[0].kind {
            StmtKind::Switch { cases, default, .. } => (cases, default),
            _ => panic!("expected switch in inner if"),
        },
        StmtKind::Switch { cases, default, .. } => (cases, default),
        _ => panic!("expected inner if or switch"),
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(
        cases[0].0,
        vec![Expr::int_lit(2), Expr::new(ExprKind::BoolLiteral(true), Span::dummy())]
    );
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
    assert!(default.is_none());
}
