//! Purpose:
//! Integration or regression tests for parser AST coverage of includes, including word logical typed assignment rhs requires parentheses, include parses, and require parses.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets cover successful AST shapes plus malformed syntax that must fail during parsing.

use super::*;

#[test]
fn test_word_logical_typed_assignment_rhs_requires_parentheses() {
    assert!(parse_fails("<?php int $x = true or false;"));
}

#[test]
fn test_include_parses() {
    let stmts = parse_source("<?php include 'file.php';");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Include {
        path,
        once,
        required,
    } = &stmts[0].kind
    {
        assert_path_string_literal(path, "file.php");
        assert!(!once);
        assert!(!required);
    } else {
        panic!("expected Include");
    }
}

#[test]
fn test_require_parses() {
    let stmts = parse_source("<?php require 'file.php';");
    if let StmtKind::Include {
        path,
        once,
        required,
    } = &stmts[0].kind
    {
        assert_path_string_literal(path, "file.php");
        assert!(!once);
        assert!(required);
    } else {
        panic!("expected Include (require)");
    }
}

#[test]
fn test_include_once_parses() {
    let stmts = parse_source("<?php include_once 'file.php';");
    if let StmtKind::Include { once, required, .. } = &stmts[0].kind {
        assert!(once);
        assert!(!required);
    } else {
        panic!("expected Include (include_once)");
    }
}

#[test]
fn test_require_once_parses() {
    let stmts = parse_source("<?php require_once 'file.php';");
    if let StmtKind::Include { once, required, .. } = &stmts[0].kind {
        assert!(once);
        assert!(required);
    } else {
        panic!("expected Include (require_once)");
    }
}

#[test]
fn test_include_with_parens_parses() {
    let stmts = parse_source("<?php include('file.php');");
    if let StmtKind::Include { path, .. } = &stmts[0].kind {
        assert_path_string_literal(path, "file.php");
    } else {
        panic!("expected Include");
    }
}

#[test]
fn test_require_with_dunder_dir_concat_parses() {
    let stmts = parse_source("<?php require __DIR__ . '/lib/x.php';");
    if let StmtKind::Include { path, .. } = &stmts[0].kind {
        match &path.kind {
            ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
                assert_eq!(left.kind, ExprKind::MagicConstant(MagicConstant::Dir));
                assert_eq!(right.kind, ExprKind::StringLiteral("/lib/x.php".to_string()));
            }
            other => panic!("expected BinaryOp(Concat) path, got {:?}", other),
        }
    } else {
        panic!("expected Include");
    }
}

#[test]
fn test_require_with_const_ref_parses() {
    let stmts = parse_source("<?php require BASE . '/x.php';");
    if let StmtKind::Include { path, .. } = &stmts[0].kind {
        match &path.kind {
            ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
                match &left.kind {
                    ExprKind::ConstRef(name) => assert_eq!(name.as_str(), "BASE"),
                    other => panic!("expected ConstRef left, got {:?}", other),
                }
                assert_eq!(right.kind, ExprKind::StringLiteral("/x.php".to_string()));
            }
            other => panic!("expected BinaryOp(Concat) path, got {:?}", other),
        }
    } else {
        panic!("expected Include");
    }
}

// --- Exponentiation ---
