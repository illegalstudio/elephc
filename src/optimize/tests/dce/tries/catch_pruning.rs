use super::*;

#[test]
fn test_eliminate_dead_code_drops_unreachable_catches_after_non_throwing_try() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::echo(Expr::int_lit(7))],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec!["Exception".into()],
                        variable: Some("e".into()),
                        body: vec![Stmt::echo(Expr::int_lit(9))],
                    }],
                    finally_body: None,
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
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_drops_unreachable_catches_before_finally() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::echo(Expr::int_lit(7))],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec!["Exception".into()],
                        variable: Some("e".into()),
                        body: vec![Stmt::echo(Expr::int_lit(9))],
                    }],
                    finally_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
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
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(8))]);
}

#[test]
fn test_eliminate_dead_code_drops_catches_shadowed_by_throwable() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::Throw(Expr::string_lit("boom")),
                        Span::dummy(),
                    )],
                    catches: vec![
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Throwable".into()],
                            variable: Some("t".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::echo(Expr::int_lit(8))],
                        },
                    ],
                    finally_body: None,
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
    let StmtKind::Try { catches, .. } = &body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(catches[0].exception_types.len(), 1);
    assert_eq!(catches[0].exception_types[0].as_str(), "Throwable");
    assert_eq!(catches[0].variable.as_deref(), Some("t"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_drops_duplicate_shadowed_catch_types() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::Throw(Expr::string_lit("boom")),
                        Span::dummy(),
                    )],
                    catches: vec![
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("first".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("second".into()),
                            body: vec![Stmt::echo(Expr::int_lit(8))],
                        },
                    ],
                    finally_body: None,
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
    let StmtKind::Try { catches, .. } = &body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(catches[0].exception_types.len(), 1);
    assert_eq!(catches[0].exception_types[0].as_str(), "Exception");
    assert_eq!(catches[0].variable.as_deref(), Some("first"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_merges_identical_catches_exposed_by_shadow_drop() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::Throw(Expr::string_lit("boom")),
                        Span::dummy(),
                    )],
                    catches: vec![
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("shadowed".into()),
                            body: vec![Stmt::echo(Expr::int_lit(8))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Error".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                    ],
                    finally_body: None,
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
    let StmtKind::Try { catches, .. } = &body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(catches[0].variable.as_deref(), Some("e"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(catches[0].exception_types.len(), 2);
    assert_eq!(catches[0].exception_types[0].as_str(), "Error");
    assert_eq!(catches[0].exception_types[1].as_str(), "Exception");
}
