//! Purpose:
//! Integration or regression tests for parser AST coverage of class static members, including static var, parent static receiver, and self static receiver.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_parse_static_var() {
    let stmts = parse_source("<?php static $count = 0;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::StaticVar { name, init } => {
            assert_eq!(name, "count");
            assert_eq!(init.kind, ExprKind::IntLiteral(0));
        }
        _ => panic!("Expected StaticVar"),
    }
}

// --- Pass by reference ---

#[test]
fn test_parse_parent_static_receiver() {
    let stmts = parse_source("<?php parent::boot();");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(receiver, &StaticReceiver::Parent);
                assert_eq!(method, "boot");
                assert!(args.is_empty());
            }
            _ => panic!("Expected StaticMethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_self_static_receiver() {
    let stmts = parse_source("<?php self::boot();");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(receiver, &StaticReceiver::Self_);
                assert_eq!(method, "boot");
                assert!(args.is_empty());
            }
            _ => panic!("Expected StaticMethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_static_static_receiver() {
    let stmts = parse_source("<?php static::boot();");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(receiver, &StaticReceiver::Static);
                assert_eq!(method, "boot");
                assert!(args.is_empty());
            }
            _ => panic!("Expected StaticMethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_first_class_callable_static_method() {
    let stmts = parse_source("<?php Foo::build(...);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
                assert_eq!(method, "build");
                match receiver {
                    StaticReceiver::Named(name) => assert_eq!(name.as_str(), "Foo"),
                    other => panic!("Expected named static receiver, got {:?}", other),
                }
            }
            other => panic!("Expected static first-class callable, got {:?}", other),
        },
        other => panic!("Expected expression statement, got {:?}", other),
    }
}

#[test]
fn test_parse_static_closure_sets_is_static() {
    let stmts = parse_source("<?php $f = static function() { return 1; };");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::Closure { is_static, is_arrow, .. } => {
                assert!(*is_static);
                assert!(!*is_arrow);
            }
            other => panic!("expected Closure, got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}

#[test]
fn test_parse_static_arrow_function_sets_is_static() {
    let stmts = parse_source("<?php $g = static fn($x) => $x;");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::Closure { is_static, is_arrow, .. } => {
                assert!(*is_static);
                assert!(*is_arrow);
            }
            other => panic!("expected Closure, got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}

#[test]
fn test_parse_non_static_closure_keeps_is_static_false() {
    let stmts = parse_source("<?php $f = function() { return 1; };");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::Closure { is_static, .. } => assert!(!*is_static),
            other => panic!("expected Closure, got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}
