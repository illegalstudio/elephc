//! Purpose:
//! Integration or regression tests for parser AST coverage of class methods, including method call, first class callable instance method, and static method call.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_parse_method_call() {
    let stmts = parse_source("<?php $obj->run(1, 2);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => {
                assert_eq!(method, "run");
                assert_eq!(args.len(), 2);
                assert!(matches!(object.kind, ExprKind::Variable(_)));
            }
            _ => panic!("Expected MethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_first_class_callable_instance_method() {
    let stmts = parse_source("<?php $obj->run(...);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
                assert_eq!(method, "run");
                assert!(matches!(object.kind, ExprKind::Variable(_)));
            }
            other => panic!("Expected instance method first-class callable, got {:?}", other),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_static_method_call() {
    let stmts = parse_source("<?php Factory::make(1);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(receiver, &StaticReceiver::Named("Factory".into()));
                assert_eq!(method, "make");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected StaticMethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_first_class_callable_static_method_static_receiver() {
    let stmts = parse_source("<?php static::make(...);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
                assert_eq!(receiver, &StaticReceiver::Static);
                assert_eq!(method, "make");
            }
            other => panic!("Expected static:: first-class callable, got {:?}", other),
        },
        _ => panic!("Expected ExprStmt"),
    }
}
