//! Purpose:
//! Integration or regression tests for parser AST coverage of expression modern PHP operators ternary and null coalesce, including short ternary expression, short ternary lower than symbolic or, and short ternary default accepts null coalesce.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_short_ternary_expression() {
    let stmts = parse_source("<?php echo $a ?: $b;");
    let expected = Stmt::echo(Expr::new(
        ExprKind::ShortTernary {
            value: Box::new(Expr::var("a")),
            default: Box::new(Expr::var("b")),
        },
        elephc::span::Span::dummy(),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_short_ternary_lower_than_symbolic_or() {
    let stmts = parse_source("<?php echo $a || $b ?: $c;");
    let expected = Stmt::echo(Expr::new(
        ExprKind::ShortTernary {
            value: Box::new(Expr::binop(Expr::var("a"), BinOp::Or, Expr::var("b"))),
            default: Box::new(Expr::var("c")),
        },
        elephc::span::Span::dummy(),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_short_ternary_default_accepts_null_coalesce() {
    let stmts = parse_source("<?php echo $a ?: $b ?? $c;");
    let expected = Stmt::echo(Expr::new(
        ExprKind::ShortTernary {
            value: Box::new(Expr::var("a")),
            default: Box::new(Expr::new(
                ExprKind::NullCoalesce {
                    value: Box::new(Expr::var("b")),
                    default: Box::new(Expr::var("c")),
                },
                elephc::span::Span::dummy(),
            )),
        },
        elephc::span::Span::dummy(),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_short_ternary_can_nest_in_full_ternary_else_branch() {
    let stmts = parse_source("<?php echo $a ? $b : $c ?: $d;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Ternary { else_expr, .. } => {
                assert!(matches!(else_expr.kind, ExprKind::ShortTernary { .. }));
            }
            other => panic!("expected Ternary, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_null_coalesce_parse() {
    let stmts = parse_source("<?php echo $x ?? 0;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Echo(expr) = &stmts[0].kind {
        if let ExprKind::NullCoalesce { .. } = &expr.kind {
            // good
        } else {
            panic!("expected NullCoalesce, got {:?}", expr.kind);
        }
    } else {
        panic!("expected Echo");
    }
}

#[test]
fn test_null_coalesce_assignment_parse() {
    let stmts = parse_source("<?php $x ??= 10;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "x");
            match &value.kind {
                ExprKind::NullCoalesce { value, default } => {
                    assert_eq!(value.kind, ExprKind::Variable("x".into()));
                    assert_eq!(default.kind, ExprKind::IntLiteral(10));
                }
                other => panic!("expected NullCoalesce, got {:?}", other),
            }
        }
        other => panic!("expected Assign, got {:?}", other),
    }
}

#[test]
fn test_null_coalesce_assignment_rhs_is_expression() {
    let stmts = parse_source("<?php $x ??= $fallback ?? 10;");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::NullCoalesce { default, .. } => {
                assert!(matches!(default.kind, ExprKind::NullCoalesce { .. }));
            }
            other => panic!("expected outer NullCoalesce, got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}

// --- Spaceship operator ---

#[test]
fn test_spaceship_parse() {
    let stmts = parse_source("<?php echo 1 <=> 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::Spaceship,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Constants ---
