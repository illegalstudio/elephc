//! Purpose:
//! Regression tests for integer range guard reasoning in AST DCE.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Covers transitive relational bounds, strict-int contradictions outside a
//!   range, elseif refinement, switch case pruning, domain safety, overflow
//!   refusal, and write invalidation.

use super::*;

/// Verifies a taken-true `$x > 10` range proves nested `$x > 5` and prunes the dead else.
#[test]
fn test_eliminate_dead_code_prunes_nested_if_from_transitive_range_guard() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(TypeExpr::Int), None, false)],
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(10)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(5)),
                            then_body: vec![Stmt::echo(Expr::int_lit(7))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
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

/// Verifies intersecting `$x >= 0` and `$x <= 0` proves `$x === 1` false.
#[test]
fn test_eliminate_dead_code_prunes_strict_int_outside_intersected_range() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(TypeExpr::Int), None, false)],
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::GtEq, Expr::int_lit(0)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("x"), BinOp::LtEq, Expr::int_lit(0)),
                            then_body: vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::binop(
                                        Expr::var("x"),
                                        BinOp::StrictEq,
                                        Expr::int_lit(1),
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

/// Verifies an outer `$x > 5` range drops impossible `case 0:` labels.
#[test]
fn test_eliminate_dead_code_drops_switch_int_cases_outside_range() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(TypeExpr::Int), None, false)],
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(5)),
                    then_body: vec![Stmt::new(
                        StmtKind::Switch {
                            subject: Expr::var("x"),
                            cases: vec![
                                (vec![Expr::int_lit(0)], vec![Stmt::echo(Expr::int_lit(7))]),
                                (vec![Expr::int_lit(6)], vec![Stmt::echo(Expr::int_lit(8))]),
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
    assert_eq!(cases[0].0, vec![Expr::int_lit(6)]);
    assert_eq!(cases[0].1, vec![Stmt::echo(Expr::int_lit(8))]);
    assert_eq!(default, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

/// Verifies `$x > i64::MAX` does not wrap into a bogus lower bound that prunes live code.
#[test]
fn test_eliminate_dead_code_refuses_overflowing_range_bound() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(TypeExpr::Int), None, false)],
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(i64::MAX)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(5)),
                            then_body: vec![Stmt::echo(Expr::int_lit(7))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
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
    // Without a recorded range, the nested if must survive intact.
    let StmtKind::If {
        then_body: nested_then,
        else_body: nested_else,
        ..
    } = &then_body[0].kind
    else {
        panic!("expected nested if to remain");
    };
    assert_eq!(nested_then, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(nested_else, &Some(vec![Stmt::echo(Expr::int_lit(8))]));
}

/// Verifies assigning to the guarded variable clears its range fact.
#[test]
fn test_eliminate_dead_code_invalidates_range_guard_on_write() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(TypeExpr::Int), None, false)],
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(10)),
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
                                condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(5)),
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

/// Verifies float parameters never acquire discrete integer bounds from relational guards.
#[test]
fn test_eliminate_dead_code_keeps_fractional_gap_branch_for_float_domain() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(TypeExpr::Float), None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(10)),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("x"),
                                BinOp::GtEq,
                                Expr::int_lit(11),
                            ),
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
    let StmtKind::If {
        then_body: nested_then,
        else_body: nested_else,
        ..
    } = &then_body[0].kind
    else {
        panic!("expected float-sensitive nested if to remain");
    };
    assert_eq!(nested_then, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(nested_else, &Some(vec![Stmt::echo(Expr::int_lit(8))]));
}

/// Verifies cumulative false `elseif` bounds isolate zero for an integer parameter.
#[test]
fn test_eliminate_dead_code_uses_integer_ranges_across_elseif_false_prefix() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(TypeExpr::Int), None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::Lt, Expr::int_lit(0)),
                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                    elseif_clauses: vec![(
                        Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(0)),
                        vec![Stmt::echo(Expr::int_lit(8))],
                    )],
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("x"),
                                BinOp::StrictEq,
                                Expr::int_lit(0),
                            ),
                            then_body: vec![Stmt::echo(Expr::int_lit(9))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(10))]),
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
        panic!("expected if");
    };
    let StmtKind::If {
        else_body: elseif_else,
        ..
    } = &else_body.as_ref().expect("expected rebuilt elseif")[0].kind
    else {
        panic!("expected elseif to be rebuilt as a nested if");
    };
    assert_eq!(elseif_else, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

/// Verifies a `foreach` iteration variable cannot inherit the overwritten parameter's int range.
#[test]
fn test_eliminate_dead_code_invalidates_range_for_foreach_iteration_variable() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("x".into(), Some(TypeExpr::Int), None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::binop(Expr::var("x"), BinOp::Gt, Expr::int_lit(10)),
                    then_body: vec![Stmt::new(
                        StmtKind::Foreach {
                            array: Expr::new(
                                ExprKind::ArrayLiteral(vec![Expr::float_lit(10.5)]),
                                Span::dummy(),
                            ),
                            key_var: None,
                            value_var: "x".into(),
                            value_by_ref: false,
                            body: vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::binop(
                                        Expr::var("x"),
                                        BinOp::GtEq,
                                        Expr::int_lit(11),
                                    ),
                                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                    elseif_clauses: Vec::new(),
                                    else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                                },
                                Span::dummy(),
                            )],
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
    let StmtKind::Foreach { body, .. } = &then_body[0].kind else {
        panic!("expected foreach");
    };
    assert!(matches!(body[0].kind, StmtKind::If { .. }));
}

/// Verifies an exact-`int` typed local seeds the discrete domain for later range guards.
#[test]
fn test_eliminate_dead_code_seeds_integer_domain_from_typed_local() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("input".into(), None, None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::TypedAssign {
                        type_expr: TypeExpr::Int,
                        name: "x".into(),
                        value: Expr::var("input"),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::binop(
                            Expr::var("x"),
                            BinOp::Gt,
                            Expr::int_lit(10),
                        ),
                        then_body: vec![Stmt::new(
                            StmtKind::If {
                                condition: Expr::binop(
                                    Expr::var("x"),
                                    BinOp::Gt,
                                    Expr::int_lit(5),
                                ),
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
                ),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[1].kind else {
        panic!("expected outer if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

/// Verifies typed float and nullable-int locals do not seed the discrete integer domain.
#[test]
fn test_eliminate_dead_code_does_not_seed_non_exact_int_typed_locals() {
    for type_expr in [
        TypeExpr::Float,
        TypeExpr::Nullable(Box::new(TypeExpr::Int)),
    ] {
        let program = vec![Stmt::new(
            StmtKind::FunctionDecl {
                name: "main".into(),
                type_params: Vec::new(),
                params: vec![("input".into(), None, None, false)],
                param_attributes: vec![Vec::new()],
                variadic: None,
                variadic_by_ref: false,
                variadic_type: None,
                return_type: None,
                by_ref_return: false,
                body: vec![
                    Stmt::new(
                        StmtKind::TypedAssign {
                            type_expr,
                            name: "x".into(),
                            value: Expr::var("input"),
                        },
                        Span::dummy(),
                    ),
                    Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(
                                Expr::var("x"),
                                BinOp::Gt,
                                Expr::int_lit(10),
                            ),
                            then_body: vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::binop(
                                        Expr::var("x"),
                                        BinOp::GtEq,
                                        Expr::int_lit(11),
                                    ),
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
                    ),
                ],
            },
            Span::dummy(),
        )];

        let eliminated = eliminate_dead_code(program);
        let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
            panic!("expected function");
        };
        let StmtKind::If { then_body, .. } = &body[1].kind else {
            panic!("expected outer if");
        };
        assert!(matches!(then_body[0].kind, StmtKind::If { .. }));
    }
}

/// Verifies a by-reference call invalidates a typed local's seeded integer domain.
#[test]
fn test_eliminate_dead_code_invalidates_typed_local_domain_for_by_ref_call() {
    let mutator = Stmt::new(
        StmtKind::FunctionDecl {
            name: "mutate".into(),
            type_params: Vec::new(),
            params: vec![("value".into(), None, None, true)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::Assign {
                    name: "value".into(),
                    value: Expr::float_lit(10.5),
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    );
    let caller = Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            type_params: Vec::new(),
            params: vec![("input".into(), None, None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::TypedAssign {
                        type_expr: TypeExpr::Int,
                        name: "x".into(),
                        value: Expr::var("input"),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::FunctionCall {
                            name: Name::unqualified("mutate"),
                            args: vec![Expr::var("x")],
                        },
                        Span::dummy(),
                    )),
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::binop(
                            Expr::var("x"),
                            BinOp::Gt,
                            Expr::int_lit(10),
                        ),
                        then_body: vec![Stmt::new(
                            StmtKind::If {
                                condition: Expr::binop(
                                    Expr::var("x"),
                                    BinOp::GtEq,
                                    Expr::int_lit(11),
                                ),
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
                ),
            ],
        },
        Span::dummy(),
    );

    let eliminated = eliminate_dead_code(vec![mutator, caller]);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[1].kind else {
        panic!("expected caller function");
    };
    let StmtKind::If { then_body, .. } = &body[2].kind else {
        panic!("expected outer if");
    };
    assert!(matches!(then_body[0].kind, StmtKind::If { .. }));
}
