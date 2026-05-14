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
