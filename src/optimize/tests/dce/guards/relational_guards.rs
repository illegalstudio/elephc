//! Purpose:
//! Regression tests for cross-variable relational guard atoms in AST DCE.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Covers `$x === $y` complements, exact/range substitution into `$y > $x`,
//!   full exact-int coupling after substitution, NaN-safe false complements,
//!   impure-condition refusal, and write invalidation.

use super::*;

/// Verifies `$x === $y` prunes a nested `$x !== $y` then-branch.
#[test]
fn test_eliminate_dead_code_prunes_nested_if_from_cross_var_strict_eq() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::StrictEq, Expr::var("y")),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("x"),
                                BinOp::StrictNotEq,
                                Expr::var("y"),
                            ),
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
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

/// Verifies `$x === 3` plus `$y > $x` strengthens `$y` so `$y <= 3` is pruned.
#[test]
fn test_eliminate_dead_code_prunes_nested_if_from_relational_exact_substitution() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::StrictEq, Expr::int_lit(3)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("y"), BinOp::Gt, Expr::var("x")),
                            then_body: vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::binop(
                                        Expr::var("y"),
                                        BinOp::LtEq,
                                        Expr::int_lit(3),
                                    ),
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
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(10))]),
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
        panic!("expected outer if");
    };
    let StmtKind::If { then_body, .. } = &then_body[0].kind else {
        panic!("expected middle if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

/// Verifies strict-equality substitution installs exact value, falsiness, and switch facts.
#[test]
fn test_eliminate_dead_code_couples_strict_substitution_into_full_exact_guards() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::StrictEq, Expr::int_lit(0)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("y"),
                                BinOp::StrictEq,
                                Expr::var("x"),
                            ),
                            then_body: vec![
                                Stmt::new(
                                    StmtKind::If {
                                        condition: Expr::var("y"),
                                        then_body: vec![Stmt::echo(Expr::int_lit(10))],
                                        elseif_clauses: Vec::new(),
                                        else_body: Some(vec![Stmt::echo(Expr::int_lit(11))]),
                                    },
                                    Span::dummy(),
                                ),
                                Stmt::new(
                                    StmtKind::Switch {
                                        subject: Expr::var("y"),
                                        cases: vec![
                                            (
                                                vec![Expr::int_lit(1)],
                                                vec![
                                                    Stmt::echo(Expr::int_lit(12)),
                                                    Stmt::new(StmtKind::Break(1), Span::dummy()),
                                                ],
                                            ),
                                            (
                                                vec![Expr::int_lit(0)],
                                                vec![
                                                    Stmt::echo(Expr::int_lit(13)),
                                                    Stmt::new(StmtKind::Break(1), Span::dummy()),
                                                ],
                                            ),
                                        ],
                                        default: Some(vec![Stmt::echo(Expr::int_lit(14))]),
                                    },
                                    Span::dummy(),
                                ),
                                Stmt::new(
                                    StmtKind::If {
                                        condition: Expr::binop(
                                            Expr::var("y"),
                                            BinOp::StrictNotEq,
                                            Expr::int_lit(0),
                                        ),
                                        then_body: vec![Stmt::echo(Expr::int_lit(15))],
                                        elseif_clauses: Vec::new(),
                                        else_body: Some(vec![Stmt::echo(Expr::int_lit(16))]),
                                    },
                                    Span::dummy(),
                                ),
                            ],
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
    let StmtKind::If {
        then_body: substituted_body,
        ..
    } = &body[0].kind
    else {
        panic!("expected combined substitution guard");
    };
    assert_eq!(substituted_body[0], Stmt::echo(Expr::int_lit(11)));
    let StmtKind::Switch { cases, default, .. } = &substituted_body[1].kind else {
        panic!("expected switch");
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].0, vec![Expr::int_lit(0)]);
    assert_eq!(default, &None);
    assert_eq!(substituted_body[2], Stmt::echo(Expr::int_lit(16)));
}

/// Verifies writing `$x` clears relational atoms that mention it.
#[test]
fn test_eliminate_dead_code_invalidates_relational_guard_on_write() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::StrictEq, Expr::var("y")),
                    then_body: vec![
                        Stmt::new(
                            StmtKind::Assign {
                                name: "x".into(),
                                value: Expr::int_lit(0),
                            },
                            Span::dummy(),
                        ),
                        Stmt::new(
                            StmtKind::If {
                                condition: Expr::binop(
                                    Expr::var("x"),
                                    BinOp::StrictNotEq,
                                    Expr::var("y"),
                                ),
                                then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                elseif_clauses: Vec::new(),
                                else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                            },
                            Span::dummy(),
                        ),
                    ],
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
    assert_eq!(then_body.len(), 2);
    let StmtKind::If {
        then_body: nested_then,
        else_body: nested_else,
        ..
    } = &then_body[1].kind
    else {
        panic!("expected nested if to remain after invalidation");
    };
    assert_eq!(nested_then, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(nested_else, &Some(vec![Stmt::echo(Expr::int_lit(8))]));
}

/// Verifies a false int/float relational atom does not invert when the float can be NaN.
#[test]
fn test_eliminate_dead_code_keeps_relational_inverse_for_mixed_nan_domain() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![
                ("x".into(), Some(TypeExpr::Int), None, false),
                ("y".into(), Some(TypeExpr::Float), None, false),
            ],
            param_attributes: vec![Vec::new(), Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::var("y")),
                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("x"),
                                BinOp::LtEq,
                                Expr::var("y"),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(8))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
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
    let StmtKind::If { else_body, .. } = &body[0].kind else {
        panic!("expected outer if");
    };
    let StmtKind::If {
        then_body,
        else_body,
        ..
    } = &else_body.as_ref().expect("expected outer else")[0].kind
    else {
        panic!("expected NaN-sensitive nested if to remain");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(8))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

/// Verifies a call-bearing relation does not create range or relational facts.
#[test]
fn test_eliminate_dead_code_ignores_impure_relational_conditions() {
    let call = || {
        Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("touch"),
                args: Vec::new(),
            },
            Span::dummy(),
        )
    };
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(call(), BinOp::Gt, Expr::int_lit(10)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(call(), BinOp::Gt, Expr::int_lit(5)),
                            then_body: vec![Stmt::echo(Expr::int_lit(7))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
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
        panic!("expected outer if");
    };
    assert!(matches!(then_body[0].kind, StmtKind::If { .. }));
}
