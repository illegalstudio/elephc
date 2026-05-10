//! Purpose:
//! Regression tests for optimizer dce switches tail_paths behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
fn test_eliminate_dead_code_drops_trailing_empty_switch_cases() {
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
                StmtKind::Switch {
                    subject: touch.clone(),
                    cases: vec![
                        (
                            vec![Expr::int_lit(1)],
                            vec![
                                Stmt::echo(Expr::int_lit(7)),
                                Stmt::new(StmtKind::Break(1), Span::dummy()),
                            ],
                        ),
                        (
                            vec![Expr::int_lit(2)],
                            vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())],
                        ),
                    ],
                    default: None,
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
            StmtKind::Switch {
                subject: touch,
                cases: vec![(
                    vec![Expr::int_lit(1)],
                    vec![
                        Stmt::echo(Expr::int_lit(7)),
                        Stmt::new(StmtKind::Break(1), Span::dummy()),
                    ],
                )],
                default: None,
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_switch_exit_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Switch {
                        subject: Expr::var("flag"),
                        cases: vec![
                            (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
                            (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
                        ],
                        default: Some(vec![Stmt::echo(Expr::int_lit(6))]),
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
            StmtKind::Switch {
                subject: Expr::var("flag"),
                cases: vec![
                    (vec![Expr::int_lit(1)], vec![Stmt::echo(Expr::int_lit(7))]),
                    (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
                ],
                default: Some(vec![Stmt::echo(Expr::int_lit(6)), Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )]
    );
}

#[test]
fn test_eliminate_dead_code_sinks_tail_into_switch_break_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Switch {
                        subject: Expr::var("flag"),
                        cases: vec![
                            (
                                vec![Expr::int_lit(1)],
                                vec![
                                    Stmt::echo(Expr::int_lit(7)),
                                    Stmt::new(StmtKind::Break(1), Span::dummy()),
                                ],
                            ),
                            (vec![Expr::int_lit(2)], vec![Stmt::echo(Expr::int_lit(8))]),
                        ],
                        default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(10)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(
        cases[0].1,
        vec![
            Stmt::echo(Expr::int_lit(7)),
            Stmt::echo(Expr::int_lit(10)),
            Stmt::new(StmtKind::Break(1), Span::dummy()),
        ]
    );
    assert_eq!(
        cases[1].1,
        vec![Stmt::echo(Expr::int_lit(8))]
    );
    assert_eq!(
        default.as_ref(),
        Some(&vec![Stmt::echo(Expr::int_lit(9)), Stmt::echo(Expr::int_lit(10))])
    );
    assert_eq!(body.len(), 1);
}
