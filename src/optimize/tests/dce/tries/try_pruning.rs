//! Purpose:
//! Regression tests for optimizer dce tries try_pruning behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
// Verifies that DCE drops the `echo 9` statement after a `try { if { throw } else { return 7 } } catch { return 8 }` block.
// The try-catch is considered exhaustive (all paths return or throw), so subsequent statements are unreachable.
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
// Verifies that DCE removes an empty try-catch shell when both bodies contain only pure (side-effect-free) statements.
// After eliminating the dead pure bodies, the try-catch itself becomes a no-op and is removed, leaving an empty function.
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
// Verifies that an unknown truthy switch entry is preserved before a matching case.
// When the switch subject (`flag`) is unknown at compile time, a truthy guard in a case pattern must not be pruned,
// because the case list contains an unknown variable (`other`) that may be truthy at runtime.
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
// Verifies that a write to `flag` inside a try body invalidates an outer guard before the catch body is analyzed.
// The catch clause reads `flag` in a conditional; the write in the try body is reachable, so the guard is kept.
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
// Verifies that a write to `flag` from a switch throw path invalidates an outer guard before the catch body is analyzed.
// Unlike the previous test, the write occurs inside a switch case that throws; DCE must still invalidate the guard.
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
// Verifies that DCE ignores writes to `flag` that occur on an unreachable switch throw path before the catch body.
// The switch case that writes to `flag` (`case 2`) is dominated by a guard that makes it unreachable,
// so the write does not invalidate the outer guard and the catch body can simplify to the else branch.
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
// Verifies that an outer guard is preserved when only a non-throw path writes to the guard variable.
// The write to `flag` occurs on the else branch of an inner if (non-throw), while the then branch throws.
// The catch clause reads `flag`; since the non-throw path is reachable, the guard is kept and the catch body simplifies to the then branch.
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
