//! Purpose:
//! Regression tests for optimizer dce tries tail_paths behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
fn test_eliminate_dead_code_keeps_statements_after_fallthrough_try() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::echo(Expr::int_lit(7))],
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

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.len(), 2);
    assert_eq!(body[1], Stmt::echo(Expr::int_lit(9)));
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_try_fallthrough_paths() {
    let may_throw = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("may_throw"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![
                            Stmt::new(StmtKind::ExprStmt(may_throw), Span::dummy()),
                            Stmt::echo(Expr::int_lit(7)),
                        ],
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

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![
                    Stmt::new(
                        StmtKind::ExprStmt(Expr::new(
                            ExprKind::FunctionCall {
                                name: Name::unqualified("may_throw"),
                                args: Vec::new(),
                            },
                            Span::dummy(),
                        )),
                        Span::dummy(),
                    ),
                    Stmt::echo(Expr::int_lit(7)),
                    Stmt::echo(Expr::int_lit(9)),
                ],
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
        )]
    );
}
