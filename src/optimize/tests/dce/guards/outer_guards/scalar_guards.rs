use super::*;

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_guard() {
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
                        StmtKind::If {
                            condition: Expr::new(
                                ExprKind::Not(Box::new(Expr::var("flag"))),
                                Span::dummy(),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_invalidates_outer_guard_after_local_write() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("flag"),
                    then_body: vec![
                        Stmt::assign("flag", Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                        Stmt::new(
                            StmtKind::If {
                                condition: Expr::var("flag"),
                                then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                elseif_clauses: Vec::new(),
                                else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                            },
                            Span::dummy(),
                        ),
                    ],
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
    let StmtKind::If { .. } = &then_body[1].kind else {
        panic!("expected inner if to remain after guard invalidation");
    };
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_null_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("value"),
                        BinOp::StrictEq,
                        Expr::new(ExprKind::Null, Span::dummy()),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("value"),
                                BinOp::StrictNotEq,
                                Expr::new(ExprKind::Null, Span::dummy()),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_zero_guard() {
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
                        StmtKind::If {
                            condition: Expr::var("value"),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_empty_string_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("value"),
                        BinOp::StrictEq,
                        Expr::string_lit(""),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("value"),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_string_zero_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(
                        Expr::var("value"),
                        BinOp::StrictEq,
                        Expr::string_lit("0"),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("value"),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_zero_float_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::float_lit(0.0)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("value"),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}
