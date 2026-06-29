//! Purpose:
//! Regression tests for optimizer dce guards composite_guards elseif_suffixes behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Verifies that when an `elseif` clause becomes unreachable because its guard is always
/// false given the parent condition, the optimizer rebuilds the `elseif` tail as a nested
/// `if` inside `else`. The `elseif`'s body becomes the then-body of the rebuilt `if`,
/// and the `elseif`'s condition is negated to become the new condition.
#[test]
fn test_eliminate_dead_code_rebuilds_empty_elseif_tail_as_needed_guard() {
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
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: touch.clone(),
                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                    elseif_clauses: vec![(
                        tap.clone(),
                        vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())],
                    )],
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
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: touch,
                then_body: vec![Stmt::echo(Expr::int_lit(7))],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::new(
                    StmtKind::If {
                        condition: Expr::new(ExprKind::Not(Box::new(tap)), Span::dummy()),
                        then_body: vec![Stmt::echo(Expr::int_lit(9))],
                        elseif_clauses: Vec::new(),
                        else_body: None,
                    },
                    Span::dummy(),
                )]),
            },
            Span::dummy(),
        )]
    );
}

/// Verifies that when an `elseif` clause's guard is a cumulative false guard (identical to the
/// negated parent condition), the optimizer removes the unreachable `elseif` clause and
/// preserves the remaining conditional chain by moving subsequent `elseif` clauses into a
/// nested `if` inside `else`. The negated cumulative-false guard becomes the `else` branch's
/// condition.
#[test]
fn test_eliminate_dead_code_prunes_unreachable_elseif_suffix_from_cumulative_false_guards() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("flag"),
                    then_body: vec![Stmt::echo(Expr::int_lit(1))],
                    elseif_clauses: vec![
                        (
                            Expr::new(ExprKind::Not(Box::new(Expr::var("flag"))), Span::dummy()),
                            vec![Stmt::echo(Expr::int_lit(2))],
                        ),
                        (Expr::var("flag"), vec![Stmt::echo(Expr::int_lit(3))]),
                    ],
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(4))]),
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
        condition,
        then_body,
        elseif_clauses,
        else_body,
    } = &body[0].kind
    else {
        panic!("expected if");
    };
    assert_eq!(*condition, Expr::var("flag"));
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(1))]);
    assert!(elseif_clauses.is_empty());
    assert_eq!(
        else_body,
        &Some(vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(ExprKind::Not(Box::new(Expr::var("flag"))), Span::dummy()),
                then_body: vec![Stmt::echo(Expr::int_lit(2))],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )])
    );
}

/// Verifies that when an `elseif` clause's guard is a negated disjunction `(a || b)` and
/// the parent condition is the original disjunction, the negated guard is pruned as
/// unreachable. The subsequent `elseif` clause (which has a `true` guard) is preserved
/// and moved into a nested `if` inside `else`.
#[test]
fn test_eliminate_dead_code_prunes_unreachable_elseif_suffix_from_negated_composite_guards() {
    let disjunction = Expr::binop(Expr::var("a"), BinOp::Or, Expr::var("b"));
    let negated_disjunction = Expr::new(ExprKind::Not(Box::new(disjunction.clone())), Span::dummy());
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: disjunction,
                    then_body: vec![Stmt::echo(Expr::int_lit(1))],
                    elseif_clauses: vec![
                        (negated_disjunction, vec![Stmt::echo(Expr::int_lit(2))]),
                        (
                            Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                            vec![Stmt::echo(Expr::int_lit(3))],
                        ),
                    ],
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(4))]),
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
        condition,
        then_body,
        elseif_clauses,
        else_body,
    } = &body[0].kind
    else {
        panic!("expected if");
    };
    assert_eq!(*condition, Expr::binop(Expr::var("a"), BinOp::Or, Expr::var("b")));
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(1))]);
    assert!(elseif_clauses.is_empty());
    assert_eq!(
        else_body,
        &Some(vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(
                    ExprKind::Not(Box::new(Expr::binop(Expr::var("a"), BinOp::Or, Expr::var("b")))),
                    Span::dummy(),
                ),
                then_body: vec![Stmt::echo(Expr::int_lit(2))],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )])
    );
}

/// Verifies that when an `elseif` clause's guard is De Morgan-equivalent to the negated
/// parent condition (`!(a && b)` vs `!a || !b`), the unreachable `elseif` clause is
/// pruned. Subsequent `elseif` clauses are preserved and moved into a nested `if`
/// inside `else`.
#[test]
fn test_eliminate_dead_code_prunes_unreachable_elseif_suffix_from_demorgan_equivalent_guards() {
    let conjunction = Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b"));
    let negated_conjunction = Expr::new(ExprKind::Not(Box::new(conjunction.clone())), Span::dummy());
    let demorgan = Expr::binop(
        Expr::new(ExprKind::Not(Box::new(Expr::var("a"))), Span::dummy()),
        BinOp::Or,
        Expr::new(ExprKind::Not(Box::new(Expr::var("b"))), Span::dummy()),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: negated_conjunction,
                    then_body: vec![Stmt::echo(Expr::int_lit(1))],
                    elseif_clauses: vec![
                        (demorgan, vec![Stmt::echo(Expr::int_lit(2))]),
                        (
                            Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                            vec![Stmt::echo(Expr::int_lit(3))],
                        ),
                    ],
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(4))]),
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
        condition,
        then_body,
        elseif_clauses,
        else_body,
    } = &body[0].kind
    else {
        panic!("expected if");
    };
    assert_eq!(
        *condition,
        Expr::new(
            ExprKind::Not(Box::new(Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b")))),
            Span::dummy(),
        )
    );
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(1))]);
    assert!(elseif_clauses.is_empty());
    assert_eq!(
        else_body,
        &Some(vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                then_body: vec![Stmt::echo(Expr::int_lit(3))],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::echo(Expr::int_lit(4))]),
            },
            Span::dummy(),
        )])
    );
}
