//! Purpose:
//! Regression tests for optimizer dce guards outer_guards boolean_guards behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Tests that a nested if inside a strict-boolean-true outer guard is pruned when the
/// inner condition is a strict-boolean-false guard (the inner then branch is dead code).
/// After DCE, the outer if's then_body collapses to the inner if's else_body.
#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_strict_bool_guard() {
    let strict_true = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
        },
        Span::dummy(),
    );
    let strict_false = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
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
                    condition: strict_true,
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: strict_false,
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

/// Tests that a nested if inside an AND-guarded outer branch is pruned when the inner
/// condition is a logical contradiction (NOT a OR NOT b is false when a AND b is true).
/// After DCE, the outer if's then_body collapses to the inner if's else_body.
#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_and_guard() {
    let contradiction = Expr::binop(
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
                    condition: Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b")),
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: contradiction,
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

/// Tests that a nested if inside a negated-AND outer guard is pruned when the inner
/// condition is the same conjunction (a AND b is true, so negated_conjunction is false).
/// After DCE, the outer if's then_body collapses to the inner if's else_body.
#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_negated_and_guard() {
    let conjunction = Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b"));
    let negated_conjunction = Expr::new(ExprKind::Not(Box::new(conjunction)), Span::dummy());
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
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b")),
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

/// Tests that a nested if inside an OR-guarded outer else-branch is pruned when the inner
/// condition's then-branch is unreachable (a && !b is true when !a || b is false).
/// After DCE, the outer if's else_body collapses to the inner if's then_body.
#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_outer_or_false_branch() {
    let outer = Expr::binop(
        Expr::new(ExprKind::Not(Box::new(Expr::var("a"))), Span::dummy()),
        BinOp::Or,
        Expr::var("b"),
    );
    let inner = Expr::binop(
        Expr::var("a"),
        BinOp::And,
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
                    condition: outer,
                    then_body: vec![Stmt::echo(Expr::int_lit(9))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(
                        StmtKind::If {
                            condition: inner,
                            then_body: vec![Stmt::echo(Expr::int_lit(7))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
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
    let StmtKind::If {
        else_body: Some(else_body),
        ..
    } = &body[0].kind
    else {
        panic!("expected if with else");
    };
    assert_eq!(else_body, &vec![Stmt::echo(Expr::int_lit(7))]);
}
