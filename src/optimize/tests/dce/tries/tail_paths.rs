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

/// Verifies that DCE does not eliminate statements that follow a `try` block when the
/// try has no强制性 finally body and control can fall through past it. Statements
/// after a fallthrough try must be preserved because execution reaches them unconditionally.
#[test]
fn test_eliminate_dead_code_keeps_statements_after_fallthrough_try() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
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

/// Verifies that DCE sinks a tail statement (echo) into the try body when the try has no catch
/// that can interrupt control flow. A call that may throw followed by a fallthrough statement
/// means the tail can be reached via the non-exception path, so DCE rewrites the try to include
/// the tail as a sinked statement while the potentially-throwing call remains in the try body.
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
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
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
