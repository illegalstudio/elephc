//! Purpose:
//! Parser tests for `yield`, `yield from`, and assignment-shaped yield expressions.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `from` is parsed contextually after `yield` and must follow PHP's
//!   case-insensitive keyword behavior.

use super::*;

/// Verifies that `<?php yield;` parses to an `ExprStmt` of `Yield { key: None, value: None }`.
/// Yield without a value or key is the minimal yield expression form.
#[test]
fn test_parse_yield_alone() {
    let stmts = parse_source("<?php yield;");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::Yield { key, value } => {
                assert!(key.is_none());
                assert!(value.is_none());
            }
            other => panic!("expected Yield, got {:?}", other),
        },
        other => panic!("expected ExprStmt, got {:?}", other),
    }
}

/// Parses `<?php yield 42;` to `Yield { key: None, value: Some(IntLiteral(42)) }`.
/// Verifies that a bare value expression after `yield` binds to the value field.
#[test]
fn test_parse_yield_value() {
    let stmts = parse_source("<?php yield 42;");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::Yield { key, value } => {
                assert!(key.is_none());
                assert_eq!(value.as_ref().unwrap().kind, ExprKind::IntLiteral(42));
            }
            other => panic!("expected Yield, got {:?}", other),
        },
        other => panic!("expected ExprStmt, got {:?}", other),
    }
}

/// Parses `<?php yield 1 => 2;` to `Yield { key: Some(IntLiteral(1)), value: Some(IntLiteral(2)) }`.
/// Verifies key⇒value syntax for keyed yield expressions.
#[test]
fn test_parse_yield_key_value() {
    let stmts = parse_source("<?php yield 1 => 2;");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::Yield { key, value } => {
                assert_eq!(key.as_ref().unwrap().kind, ExprKind::IntLiteral(1));
                assert_eq!(value.as_ref().unwrap().kind, ExprKind::IntLiteral(2));
            }
            other => panic!("expected Yield, got {:?}", other),
        },
        other => panic!("expected ExprStmt, got {:?}", other),
    }
}

/// Parses `<?php yield from $g;` to `YieldFrom(Variable("g"))`.
/// Verifies that `from` (lowercase) is recognized as the yield-from keyword.
#[test]
fn test_parse_yield_from() {
    let stmts = parse_source("<?php yield from $g;");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::YieldFrom(inner) => {
                assert_eq!(inner.kind, ExprKind::Variable("g".to_string()));
            }
            other => panic!("expected YieldFrom, got {:?}", other),
        },
        other => panic!("expected ExprStmt, got {:?}", other),
    }
}

/// Parses `<?php yield FROM $g;` to `YieldFrom(Variable("g"))`.
/// Verifies PHP's case-insensitive `from` keyword in yield-from expressions.
#[test]
fn test_parse_yield_from_case_insensitive_from() {
    let stmts = parse_source("<?php yield FROM $g;");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::YieldFrom(inner) => {
                assert_eq!(inner.kind, ExprKind::Variable("g".to_string()));
            }
            other => panic!("expected YieldFrom, got {:?}", other),
        },
        other => panic!("expected ExprStmt, got {:?}", other),
    }
}

/// Parses `<?php $x = yield $v;` to an `Assign` to `x` with a `Yield` value.
/// Verifies that yield expressions are valid as assignment-rhs expressions.
#[test]
fn test_parse_yield_in_assignment() {
    let stmts = parse_source("<?php $x = yield $v;");
    match &stmts[0].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "x");
            match &value.kind {
                ExprKind::Yield { key, value } => {
                    assert!(key.is_none());
                    assert_eq!(value.as_ref().unwrap().kind, ExprKind::Variable("v".to_string()));
                }
                other => panic!("expected Yield, got {:?}", other),
            }
        }
        other => panic!("expected Assign, got {:?}", other),
    }
}
