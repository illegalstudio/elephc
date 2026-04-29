use super::*;

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_strict_bool_guard() {
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
                    condition: strict_true,
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: strict_false,
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
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_and_guard() {
    let contradiction = Expr::binop(
        Expr::new(ExprKind::Not(Box::new(Expr::var("a"))), Span::dummy()),
        BinOp::Or,
        Expr::new(ExprKind::Not(Box::new(Expr::var("b"))), Span::dummy()),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b")),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: contradiction,
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
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_negated_and_guard() {
    let conjunction = Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b"));
    let negated_conjunction = Expr::new(ExprKind::Not(Box::new(conjunction)), Span::dummy());
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: negated_conjunction,
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b")),
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
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_or_false_branch() {
    let outer = Expr::binop(
        Expr::new(ExprKind::Not(Box::new(Expr::var("a"))), Span::dummy()),
        BinOp::Or,
        Expr::var("b"),
    );
    let inner = Expr::binop(
        Expr::var("a"),
        BinOp::And,
        Expr::new(ExprKind::Not(Box::new(Expr::var("b"))), Span::dummy()),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: outer,
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: inner,
                            then_body: vec![Stmt::echo(Expr::int_lit(7))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
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
