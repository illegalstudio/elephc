use super::*;

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
                            Stmt::new(StmtKind::Break(1), Span::dummy()),
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
        vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())]
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
    let StmtKind::Switch { cases, default, .. } = &then_body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, vec![Expr::int_lit(0)]);
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
    assert!(default.is_none());
}

#[test]
fn test_eliminate_dead_code_drops_impossible_switch_cases_from_outer_excluded_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
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
                                (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
                                (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
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
    assert_eq!(cases[0].0, vec![Expr::int_lit(2)]);
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
    assert_eq!(default, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
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
                            Stmt::new(StmtKind::Break(1), Span::dummy()),
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
