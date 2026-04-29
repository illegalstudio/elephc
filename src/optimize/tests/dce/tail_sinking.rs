use super::*;

#[test]
fn test_eliminate_dead_code_reduces_empty_if_chain_to_needed_condition_checks() {
    let touch = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let tap = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("tap"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
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
                StmtKind::If {
                    condition: touch.clone(),
                    then_body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin.clone()), Span::dummy())],
                    elseif_clauses: vec![(
                        tap.clone(),
                        vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())],
                    )],
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
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(ExprKind::Not(Box::new(touch)), Span::dummy()),
                then_body: vec![Stmt::new(StmtKind::ExprStmt(tap), Span::dummy())],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_if_fallthrough_branch() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        elseif_clauses: Vec::new(),
                        else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
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
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(7))),
                    Span::dummy(),
                )],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::echo(Expr::int_lit(8)), Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_implicit_else_path() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        elseif_clauses: Vec::new(),
                        else_body: None,
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
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(7))),
                    Span::dummy(),
                )],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_ifdef_fallthrough_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::IfDef {
                        symbol: "DEBUG".into(),
                        then_body: vec![Stmt::echo(Expr::int_lit(7))],
                        else_body: Some(vec![Stmt::new(
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
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::IfDef {
                symbol: "DEBUG".into(),
                then_body: vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(9))],
                else_body: Some(vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(8))),
                    Span::dummy(),
                )]),
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_reduces_empty_if_to_effectful_condition_eval() {
    let touch = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
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
                StmtKind::If {
                    condition: touch.clone(),
                    then_body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin.clone()), Span::dummy())],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())]),
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
    assert_eq!(body.len(), 1);
    assert_eq!(
        body[0],
        Stmt::new(StmtKind::ExprStmt(touch), Span::dummy()),
    );
}
