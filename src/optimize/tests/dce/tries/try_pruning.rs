use super::*;

#[test]
fn test_eliminate_dead_code_drops_statements_after_exhaustive_try_catch() {
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
                            StmtKind::If {
                                condition: Expr::var("flag"),
                                then_body: vec![Stmt::new(
                                    StmtKind::Throw(Expr::string_lit("boom")),
                                    Span::dummy(),
                                )],
                                elseif_clauses: Vec::new(),
                                else_body: Some(vec![Stmt::new(
                                    StmtKind::Return(Some(Expr::int_lit(7))),
                                    Span::dummy(),
                                )]),
                            },
                            Span::dummy(),
                        )],
                        catches: vec![crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::new(
                                StmtKind::Return(Some(Expr::int_lit(8))),
                                Span::dummy(),
                            )],
                        }],
                        finally_body: None,
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(normalize_control_flow(program));

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(body[0].kind, StmtKind::Try { .. }));
}

#[test]
fn test_eliminate_dead_code_drops_empty_try_shell_created_by_branch_dce() {
    let pure_builtin = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strlen"),
            args: vec![Expr::string_lit("abc")],
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
                StmtKind::Try {
                    try_body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin.clone()), Span::dummy())],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec!["Exception".into()],
                        variable: Some("e".into()),
                        body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())],
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
    assert!(body.is_empty());
}

#[test]
fn test_eliminate_dead_code_keeps_unknown_truthy_switch_entry_before_matching_case() {
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
                                    vec![
                                        Expr::var("other"),
                                        Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                                    ],
                                    vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                                ),
                                (
                                    vec![Expr::new(ExprKind::BoolLiteral(true), Span::dummy())],
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
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].0, vec![Expr::var("other")]);
    assert_eq!(
        cases[0].1,
        vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())]
    );
    assert_eq!(cases[1].0, vec![Expr::new(ExprKind::BoolLiteral(true), Span::dummy())]);
    assert_eq!(cases[1].1, vec![Stmt::echo(Expr::int_lit(8))]);
    assert!(default.is_none());
}

#[test]
fn test_eliminate_dead_code_invalidates_outer_guard_before_catch_body() {
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
                            try_body: vec![
                                Stmt::assign("flag", Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                                Stmt::new(
                                    StmtKind::Throw(Expr::new(
                                        ExprKind::NewObject {
                                            class_name: Name::unqualified("Exception"),
                                            args: vec![Expr::string_lit("boom")],
                                        },
                                        Span::dummy(),
                                    )),
                                    Span::dummy(),
                                ),
                            ],
                            catches: vec![crate::parser::ast::CatchClause {
                                exception_types: vec![Name::unqualified("Exception")],
                                variable: Some("e".into()),
                                body: vec![Stmt::new(
                                    StmtKind::If {
                                        condition: Expr::var("flag"),
                                        then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                        elseif_clauses: Vec::new(),
                                        else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                                    },
                                    Span::dummy(),
                                )],
                            }],
                            finally_body: None,
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
    let StmtKind::Try { catches, .. } = &then_body[0].kind else {
        panic!("expected try");
    };
    let StmtKind::If { .. } = &catches[0].body[0].kind else {
        panic!("expected catch inner if to remain after try write invalidation");
    };
}

#[test]
fn test_eliminate_dead_code_invalidates_outer_guard_before_catch_body_from_switch_throw_path() {
    let throw_exception = Stmt::new(
        StmtKind::Throw(Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("Exception"),
                args: vec![Expr::string_lit("boom")],
            },
            Span::dummy(),
        )),
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
                    condition: Expr::var("flag"),
                    then_body: vec![Stmt::new(
                        StmtKind::Try {
                            try_body: vec![Stmt::new(
                                StmtKind::Switch {
                                    subject: Expr::var("value"),
                                    cases: vec![(
                                        vec![Expr::int_lit(1)],
                                        vec![
                                            Stmt::assign(
                                                "flag",
                                                Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                                            ),
                                            throw_exception,
                                        ],
                                    )],
                                    default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                                },
                                Span::dummy(),
                            )],
                            catches: vec![crate::parser::ast::CatchClause {
                                exception_types: vec![Name::unqualified("Exception")],
                                variable: Some("e".into()),
                                body: vec![Stmt::new(
                                    StmtKind::If {
                                        condition: Expr::var("flag"),
                                        then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                        elseif_clauses: Vec::new(),
                                        else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                                    },
                                    Span::dummy(),
                                )],
                            }],
                            finally_body: None,
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
    let StmtKind::Try { catches, .. } = &then_body[0].kind else {
        panic!("expected try");
    };
    let StmtKind::If { .. } = &catches[0].body[0].kind else {
        panic!("expected catch inner if to remain after switch throw-path write invalidation");
    };
}

#[test]
fn test_eliminate_dead_code_ignores_unreachable_switch_throw_path_writes_before_catch_body() {
    let throw_exception = Stmt::new(
        StmtKind::Throw(Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("Exception"),
                args: vec![Expr::string_lit("boom")],
            },
            Span::dummy(),
        )),
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
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::int_lit(1)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("flag"),
                            then_body: vec![Stmt::new(
                                StmtKind::Try {
                                    try_body: vec![Stmt::new(
                                        StmtKind::Switch {
                                            subject: Expr::var("value"),
                                            cases: vec![
                                                (
                                                    vec![Expr::int_lit(2)],
                                                    vec![
                                                        Stmt::assign(
                                                            "flag",
                                                            Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                                                        ),
                                                        throw_exception.clone(),
                                                    ],
                                                ),
                                                (vec![Expr::int_lit(1)], vec![throw_exception]),
                                            ],
                                            default: None,
                                        },
                                        Span::dummy(),
                                    )],
                                    catches: vec![crate::parser::ast::CatchClause {
                                        exception_types: vec![Name::unqualified("Exception")],
                                        variable: Some("e".into()),
                                        body: vec![Stmt::new(
                                            StmtKind::If {
                                                condition: Expr::var("flag"),
                                                then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                                elseif_clauses: Vec::new(),
                                                else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                                            },
                                            Span::dummy(),
                                        )],
                                    }],
                                    finally_body: None,
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
        panic!("expected value guard");
    };
    let try_stmt = match &then_body[0].kind {
        StmtKind::If { then_body, .. } => &then_body[0],
        StmtKind::Try { .. } => &then_body[0],
        _ => panic!("expected flag guard or try"),
    };
    let StmtKind::Try { catches, .. } = &try_stmt.kind else {
        panic!("expected try");
    };
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_eliminate_dead_code_preserves_outer_guard_for_catch_when_only_non_throw_path_writes() {
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
                            try_body: vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::var("other"),
                                    then_body: vec![Stmt::assign(
                                        "flag",
                                        Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                                    )],
                                    elseif_clauses: Vec::new(),
                                    else_body: Some(vec![Stmt::new(
                                        StmtKind::Throw(Expr::new(
                                            ExprKind::NewObject {
                                                class_name: Name::unqualified("Exception"),
                                                args: vec![Expr::string_lit("boom")],
                                            },
                                            Span::dummy(),
                                        )),
                                        Span::dummy(),
                                    )]),
                                },
                                Span::dummy(),
                            )],
                            catches: vec![crate::parser::ast::CatchClause {
                                exception_types: vec![Name::unqualified("Exception")],
                                variable: Some("e".into()),
                                body: vec![Stmt::new(
                                    StmtKind::If {
                                        condition: Expr::var("flag"),
                                        then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                        elseif_clauses: Vec::new(),
                                        else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                                    },
                                    Span::dummy(),
                                )],
                            }],
                            finally_body: None,
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
    let StmtKind::Try { catches, .. } = &then_body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}
