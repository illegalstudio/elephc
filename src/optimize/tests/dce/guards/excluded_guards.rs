use super::*;

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_zero_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::int_lit(0)),
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::int_lit(0)),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )]),
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
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_null_guard() {
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
                        BinOp::StrictNotEq,
                        Expr::new(ExprKind::Null, Span::dummy()),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("value"),
                                BinOp::StrictEq,
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
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_empty_string_guard() {
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
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("value"),
                                BinOp::StrictEq,
                                Expr::string_lit(""),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )]),
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
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_string_zero_guard() {
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
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("value"),
                                BinOp::StrictEq,
                                Expr::string_lit("0"),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )]),
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
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_excluded_float_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::float_lit(1.5)),
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::float_lit(1.5)),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                        },
                        Span::dummy(),
                    )]),
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
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}
