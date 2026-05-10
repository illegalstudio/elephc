//! Purpose:
//! Regression tests for optimizer dce switches exhaustive_suffixes behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
fn test_eliminate_dead_code_prunes_negated_strict_switch_true_case() {
    let negated_strict_eq = Expr::new(
        ExprKind::Not(Box::new(Expr::binop(
            Expr::var("value"),
            BinOp::StrictEq,
            Expr::int_lit(1),
        ))),
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
                    condition: Expr::binop(Expr::var("value"), BinOp::StrictNotEq, Expr::int_lit(1)),
                    then_body: vec![Stmt::new(
                        StmtKind::Switch {
                            subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                            cases: vec![
                                (
                                    vec![Expr::binop(Expr::var("value"), BinOp::StrictEq, Expr::int_lit(1))],
                                    vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                                ),
                                (vec![negated_strict_eq], vec![Stmt::echo(Expr::int_lit(8))]),
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
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
    assert!(default.is_none());
}

#[test]
fn test_eliminate_dead_code_prunes_exhaustive_negated_and_switch_true_default() {
    let conjunction = Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b"));
    let negated_conjunction = Expr::new(ExprKind::Not(Box::new(conjunction.clone())), Span::dummy());
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                    cases: vec![
                        (
                            vec![conjunction],
                            vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                        ),
                        (
                            vec![negated_conjunction],
                            vec![Stmt::echo(Expr::int_lit(8)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                        ),
                    ],
                    default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 2);
    assert!(default.is_none());
}

#[test]
fn test_eliminate_dead_code_prunes_exhaustive_negated_or_switch_true_default() {
    let disjunction = Expr::binop(Expr::var("a"), BinOp::Or, Expr::var("b"));
    let negated_disjunction = Expr::new(ExprKind::Not(Box::new(disjunction.clone())), Span::dummy());
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                    cases: vec![
                        (
                            vec![disjunction],
                            vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                        ),
                        (
                            vec![negated_disjunction],
                            vec![Stmt::echo(Expr::int_lit(8)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                        ),
                    ],
                    default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 2);
    assert!(default.is_none());
}

#[test]
fn test_eliminate_dead_code_prunes_switch_true_suffix_after_exhaustive_multi_pattern_case() {
    let exhaustive_patterns = vec![
        Expr::var("flag"),
        Expr::new(ExprKind::Not(Box::new(Expr::var("flag"))), Span::dummy()),
    ];
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                    cases: vec![
                        (
                            exhaustive_patterns.clone(),
                            vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())],
                        ),
                        (vec![Expr::var("other")], vec![Stmt::echo(Expr::int_lit(8))]),
                    ],
                    default: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::Switch { cases, default, .. } = &body[0].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, exhaustive_patterns);
    assert_eq!(
        cases[0].1,
        vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())]
    );
    assert!(default.is_none());
}

#[test]
fn test_eliminate_dead_code_prunes_scalar_switch_suffix_after_exhaustive_multi_pattern_case() {
    let exhaustive_patterns = vec![Expr::int_lit(1), Expr::int_lit(2)];
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::new(
                        ExprKind::BinaryOp {
                            left: Box::new(Expr::var("x")),
                            op: BinOp::StrictEq,
                            right: Box::new(Expr::int_lit(2)),
                        },
                        Span::dummy(),
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::Switch {
                            subject: Expr::var("x"),
                            cases: vec![
                                (
                                    exhaustive_patterns.clone(),
                                    vec![
                                        Stmt::echo(Expr::int_lit(7)),
                                        Stmt::new(StmtKind::Break(1), Span::dummy()),
                                    ],
                                ),
                                (vec![Expr::int_lit(3)], vec![Stmt::echo(Expr::int_lit(8))]),
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
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, vec![Expr::int_lit(2)]);
    assert_eq!(
        cases[0].1,
        vec![Stmt::echo(Expr::int_lit(7)), Stmt::new(StmtKind::Break(1), Span::dummy())]
    );
    assert!(default.is_none());
}
