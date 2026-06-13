//! Purpose:
//! Integration or regression tests for parser AST coverage of includes, including word logical typed assignment rhs requires parentheses, include parses, and require parses.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets cover successful AST shapes plus malformed syntax that must fail during parsing.

use super::*;

/// Verifies that `<?php int $x = true or false;` fails to parse because the RHS of a
/// typed assignment requires parentheses — the `or` keyword has lower precedence than
/// the `=` sign, which would incorrectly parse as `(int $x = true) or false`.
#[test]
fn test_word_logical_typed_assignment_rhs_requires_parentheses() {
    assert!(parse_fails("<?php int $x = true or false;"));
}

/// Verifies that `<?php include 'file.php';` parses to an `Include` with path StringLiteral
/// "file.php", once=false, required=false.
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

/// Verifies that `<?php @include 'file.php';` parses with error suppression applied to the include.
#[test]
fn test_error_suppressed_include_parses() {
    let stmts = parse_source("<?php @include 'file.php';");
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

/// Verifies that `<?php require 'file.php';` parses with required=true, once=false.
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

/// Verifies that `<?php include_once 'file.php';` parses with once=true, required=false.
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

/// Verifies that `<?php require_once 'file.php';` parses with once=true, required=true.
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

/// Verifies that `<?php include('file.php');` (parenthesized path) parses to an `Include`
/// with a string literal path. Parenthesized include paths are valid PHP.
#[test]
fn test_include_with_parens_parses() {
    let stmts = parse_source("<?php include('file.php');");
    if let StmtKind::Include { path, .. } = &stmts[0].kind {
        assert_path_string_literal(path, "file.php");
    } else {
        panic!("expected Include");
    }
}

/// Verifies that `<?php require __DIR__ . '/lib/x.php';` parses with a binary concatenation
/// of `__DIR__` magic constant and a string literal as the include path.
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

/// Verifies that `<?php require BASE . '/x.php';` parses with a binary concatenation of
/// a constant reference and a string literal as the include path.
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

/// Asserts that `expr` is the include IIFE `(static function () { include; return 1; })()` and
/// returns the wrapped `Include`'s `(once, required)` flags for the caller to check.
fn assert_include_iife(expr: &ExprKind) -> (bool, bool) {
    let closure = match expr {
        ExprKind::ExprCall { callee, args } => {
            assert!(args.is_empty(), "include IIFE takes no arguments");
            &callee.kind
        }
        other => panic!("Expected ExprCall, got {:?}", other),
    };
    let body = match closure {
        ExprKind::Closure {
            body, is_static, ..
        } => {
            assert!(is_static, "include closure should be static");
            body
        }
        other => panic!("Expected Closure, got {:?}", other),
    };
    assert_eq!(body.len(), 2, "closure body is [include, return 1]");
    assert!(
        matches!(
            &body[1].kind,
            StmtKind::Return(Some(value)) if matches!(value.kind, ExprKind::IntLiteral(1))
        ),
        "closure falls through to `return 1`",
    );
    match &body[0].kind {
        StmtKind::Include { once, required, .. } => (*once, *required),
        other => panic!("Expected Include, got {:?}", other),
    }
}

/// Verifies that `return require X;` becomes `return (static fn) { include; return 1; }()`, so
/// the returned value is the included file's own `return` (or `1` when it has none).
#[test]
fn test_return_require_wraps_in_include_iife() {
    let stmts = parse_source("<?php function f() { return require 'helper.php'; }");
    let body = match &stmts[0].kind {
        StmtKind::FunctionDecl { body, .. } => body,
        other => panic!("Expected FunctionDecl, got {:?}", other),
    };
    match &body[0].kind {
        StmtKind::Return(Some(value)) => {
            assert_eq!(assert_include_iife(&value.kind), (false, true));
        }
        other => panic!("Expected Return, got {:?}", other),
    }
}

/// Verifies that `$x = require_once X;` assigns the include IIFE so `$x` receives the included
/// file's value, and that the once/required flags are carried through.
#[test]
fn test_assign_require_wraps_in_include_iife() {
    let stmts = parse_source("<?php $x = require_once 'helper.php';");
    match &stmts[0].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "x");
            assert_eq!(assert_include_iife(&value.kind), (true, true));
        }
        other => panic!("Expected Assign, got {:?}", other),
    }
}
