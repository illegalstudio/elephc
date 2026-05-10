//! Purpose:
//! Regression tests for optimizer dce guards composite_guards composite_regions behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_demorgan_equivalent_guard() {
    let conjunction = Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b"));
    let negated_conjunction = Expr::new(ExprKind::Not(Box::new(conjunction)), Span::dummy());
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
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: negated_conjunction,
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: demorgan,
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
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_loose_comparison_guard() {
    let loose_eq = Expr::binop(Expr::var("value"), BinOp::Eq, Expr::int_lit(0));
    let loose_neq = Expr::binop(Expr::var("value"), BinOp::NotEq, Expr::int_lit(0));
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: loose_eq,
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: loose_neq,
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

#[test]
fn test_eliminate_dead_code_prunes_nested_if_region_from_relational_guard() {
    let greater_than = Expr::binop(Expr::var("value"), BinOp::Gt, Expr::int_lit(10));
    let less_equal = Expr::binop(Expr::var("value"), BinOp::LtEq, Expr::int_lit(10));
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: greater_than,
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: less_equal,
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

#[test]
fn test_eliminate_dead_code_prunes_nested_elseif_from_composite_guard_refinement() {
    let left = Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b"));
    let outer = Expr::binop(left.clone(), BinOp::Or, Expr::var("c"));
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: outer,
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::new(ExprKind::Not(Box::new(Expr::var("c"))), Span::dummy()),
                            then_body: vec![Stmt::new(
                                StmtKind::If {
                                    condition: left,
                                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                    elseif_clauses: vec![(
                                        Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                                        vec![Stmt::echo(Expr::int_lit(8))],
                                    )],
                                    else_body: None,
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
    let StmtKind::If { then_body, else_body, .. } = &then_body[0].kind else {
        panic!("expected nested if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
}

#[test]
fn test_eliminate_dead_code_prunes_nested_subexpr_from_composite_guard_refinement() {
    let ab = Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b"));
    let left = Expr::binop(ab.clone(), BinOp::Or, Expr::var("c"));
    let outer = Expr::binop(left, BinOp::And, Expr::var("d"));
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: outer,
                    then_body: vec![Stmt::new(
                        StmtKind::If {
                            condition: Expr::var("d"),
                            then_body: vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::new(
                                        ExprKind::Not(Box::new(Expr::var("c"))),
                                        Span::dummy(),
                                    ),
                                    then_body: vec![Stmt::new(
                                        StmtKind::If {
                                            condition: ab,
                                            then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                            elseif_clauses: vec![(
                                                Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                                                vec![Stmt::echo(Expr::int_lit(8))],
                                            )],
                                            else_body: None,
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
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(11))]),
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
        panic!("expected outer if");
    };
    let StmtKind::If { then_body, else_body: c_else, .. } = &then_body[0].kind else {
        panic!("expected !c if");
    };
    assert_eq!(then_body, &vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(c_else, &Some(vec![Stmt::echo(Expr::int_lit(9))]));
    assert_eq!(else_body, &Some(vec![Stmt::echo(Expr::int_lit(11))]));
}
