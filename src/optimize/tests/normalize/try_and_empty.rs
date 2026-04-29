use super::*;

#[test]
fn test_normalize_control_flow_replaces_empty_if_with_effectful_condition_eval() {
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: call.clone(),
            then_body: Vec::new(),
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::new(StmtKind::ExprStmt(call), Span::dummy())]
    );
}

#[test]
fn test_normalize_control_flow_drops_empty_ifdef_shell() {
    let program = vec![Stmt::new(
        StmtKind::IfDef {
            symbol: "DEBUG".into(),
            then_body: Vec::new(),
            else_body: Some(Vec::new()),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert!(pruned.is_empty());
}

#[test]
fn test_normalize_control_flow_replaces_empty_switch_with_subject_eval() {
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: call.clone(),
            cases: Vec::new(),
            default: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::new(StmtKind::ExprStmt(call), Span::dummy())]
    );
}

#[test]
fn test_normalize_control_flow_inlines_empty_try_finally_body() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: Vec::new(),
            catches: vec![crate::parser::ast::CatchClause {
                exception_types: vec![Name::unqualified("Exception")],
                variable: Some("e".into()),
                body: vec![Stmt::echo(Expr::int_lit(7))],
            }],
            finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(9))]);
}

#[test]
fn test_normalize_control_flow_inlines_default_only_switch() {
    let program = vec![Stmt::new(
        StmtKind::Switch {
            subject: Expr::var("flag"),
            cases: Vec::new(),
            default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(9))]);
}

#[test]
fn test_normalize_control_flow_nests_elseif_chain_after_empty_head() {
    let program = vec![Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: Vec::new(),
            elseif_clauses: vec![(
                Expr::var("b"),
                vec![Stmt::echo(Expr::int_lit(7))],
            )],
            else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(
        pruned,
        vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(
                    ExprKind::Not(Box::new(Expr::var("a"))),
                    Span::dummy(),
                ),
                then_body: vec![Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("b"),
                        then_body: vec![Stmt::echo(Expr::int_lit(7))],
                        elseif_clauses: Vec::new(),
                        else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                    },
                    Span::dummy(),
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_normalize_control_flow_inlines_non_throwing_try_catch() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::echo(Expr::int_lit(7))],
            catches: vec![crate::parser::ast::CatchClause {
                exception_types: vec![Name::unqualified("Exception")],
                variable: Some("e".into()),
                body: vec![Stmt::echo(Expr::int_lit(9))],
            }],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
fn test_normalize_control_flow_inlines_non_throwing_try_finally_fallthrough() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::echo(Expr::int_lit(7))],
            catches: Vec::new(),
            finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned, vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(9))]);
}

#[test]
fn test_normalize_control_flow_keeps_non_throwing_try_finally_with_return() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::Return(Some(Expr::int_lit(7))),
                Span::dummy(),
            )],
            catches: Vec::new(),
            finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    assert!(matches!(pruned[0].kind, StmtKind::Try { .. }));
}

#[test]
fn test_normalize_control_flow_folds_outer_finally_into_single_inner_try() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::ExprStmt(Expr::new(
                            ExprKind::Throw(Box::new(Expr::new(
                                ExprKind::NewObject {
                                    class_name: Name::unqualified("A"),
                                    args: Vec::new(),
                                },
                                Span::dummy(),
                            ))),
                            Span::dummy(),
                        )),
                        Span::dummy(),
                    )],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec![Name::unqualified("A")],
                        variable: Some("e".into()),
                        body: vec![Stmt::echo(Expr::int_lit(7))],
                    }],
                    finally_body: None,
                },
                Span::dummy(),
            )],
            catches: Vec::new(),
            finally_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::Try {
        try_body,
        catches,
        finally_body,
    } = &pruned[0].kind
    else {
        panic!("expected normalized try");
    };
    assert_eq!(
        try_body,
        &vec![Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::Throw(Box::new(Expr::new(
                    ExprKind::NewObject {
                        class_name: Name::unqualified("A"),
                        args: Vec::new(),
                    },
                    Span::dummy(),
                ))),
                Span::dummy(),
            )),
            Span::dummy(),
        )]
    );
    assert_eq!(catches.len(), 1);
    assert_eq!(
        catches[0].exception_types,
        vec![Name::unqualified("A")]
    );
    assert_eq!(catches[0].variable.as_deref(), Some("e"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(finally_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_normalize_control_flow_hoists_non_throwing_try_prefix() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![
                Stmt::echo(Expr::int_lit(7)),
                Stmt::new(StmtKind::Throw(Expr::var("boom")), Span::dummy()),
            ],
            catches: vec![crate::parser::ast::CatchClause {
                exception_types: vec![Name::unqualified("Exception")],
                variable: Some("e".into()),
                body: vec![Stmt::echo(Expr::int_lit(9))],
            }],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 2);
    assert_eq!(pruned[0], Stmt::echo(Expr::int_lit(7)));
    assert!(matches!(pruned[1].kind, StmtKind::Try { .. }));
}
