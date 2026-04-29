use super::*;

#[test]
fn test_eliminate_dead_code_drops_statements_after_try_finally_exit() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        catches: Vec::new(),
                        finally_body: Some(vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(8))),
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(body[0].kind, StmtKind::Try { .. }));
}

#[test]
fn test_eliminate_dead_code_preserves_outer_guard_for_finally_when_only_other_locals_change() {
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
                        StmtKind::Try {
                            try_body: vec![Stmt::assign("other", Expr::int_lit(1))],
                            catches: Vec::new(),
                            finally_body: Some(vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::var("flag"),
                                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                    elseif_clauses: Vec::new(),
                                    else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                                },
                                Span::dummy(),
                            )]),
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
    assert_eq!(
        then_body,
        &vec![Stmt::assign("other", Expr::int_lit(1)), Stmt::echo(Expr::int_lit(7))]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_safe_finally_path() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::echo(Expr::int_lit(7))],
                        catches: Vec::new(),
                        finally_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(8)), Stmt::echo(Expr::int_lit(9))]);
}
