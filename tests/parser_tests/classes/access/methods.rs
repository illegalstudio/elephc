//! Purpose:
//! Integration or regression tests for parser AST coverage of class methods, including method call, first class callable instance method, and static method call.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Parses `$obj->self()` and verifies a semi-reserved keyword (`self`) is accepted as a
/// method name after `->`, producing a `MethodCall` with method "self" (PHP 8 semi-reserved).
#[test]
fn test_parse_keyword_method_call() {
    let stmts = parse_source("<?php $obj->self();");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::MethodCall { method, .. } => assert_eq!(method, "self"),
            other => panic!("Expected MethodCall, got {:?}", other),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

/// Parses `$obj->list` and verifies a semi-reserved keyword (`list`) is accepted as a
/// property name after `->`, producing a `PropertyAccess` with property "list".
#[test]
fn test_parse_keyword_property_access() {
    let stmts = parse_source("<?php echo $obj->list;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { property, .. } => assert_eq!(property, "list"),
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        _ => panic!("Expected Echo"),
    }
}

/// Parses `Factory::new()` and verifies a semi-reserved keyword (`new`) is accepted as a
/// static method name after `::`, producing a `StaticMethodCall` with method "new".
#[test]
fn test_parse_keyword_static_method_call() {
    let stmts = parse_source("<?php Factory::new();");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::StaticMethodCall { method, .. } => assert_eq!(method, "new"),
            other => panic!("Expected StaticMethodCall, got {:?}", other),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

/// Parses `$obj->run(1, 2)` and verifies `MethodCall` AST with correct method name,
/// argument count, and object expression kind (Variable).
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

/// Parses `$obj->run(...)` and verifies `FirstClassCallable(Method)` AST with
/// spread args, correct method name, and object expression kind (Variable).
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

/// Parses `Factory::make(1)` and verifies `StaticMethodCall` AST with named
/// receiver, method name, and single argument.
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

/// Parses `RegexIterator::MATCH` and verifies keyword-like class constants after `::`.
#[test]
fn test_parse_scoped_constant_named_like_keyword() {
    let stmts = parse_source("<?php echo RegexIterator::MATCH;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::ScopedConstantAccess { receiver, name } => {
                assert_eq!(receiver, &StaticReceiver::Named("RegexIterator".into()));
                assert_eq!(name, "MATCH");
            }
            other => panic!("Expected scoped constant access, got {:?}", other),
        },
        _ => panic!("Expected Echo"),
    }
}

/// Parses `static::make(...)` and verifies `FirstClassCallable(StaticMethod)` AST
/// with `Static` receiver (not named) and spread args.
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
