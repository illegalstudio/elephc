//! Purpose:
//! Integration or regression tests for parser AST coverage of never, including never return type, never return type is case insensitive, and never return type on instance method.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Parses `function fail(): never { throw new \Exception(); }` and verifies the
/// `StmtKind::FunctionDecl` carries `return_type = Some(TypeExpr::Never)`.
#[test]
fn test_parse_never_return_type() {
    let stmts = parse_source("<?php function fail(): never { throw new \\Exception(); }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            name,
            return_type,
            ..
        } => {
            assert_eq!(name, "fail");
            assert_eq!(return_type.as_ref(), Some(&TypeExpr::Never));
        }
        other => panic!("Expected FunctionDecl, got {:?}", other),
    }
}

/// Parses `function fail(): NEVER { ... }` (all caps) and verifies the parser
/// normalizes it to `TypeExpr::Never`, confirming case-insensitive handling.
#[test]
fn test_parse_never_return_type_is_case_insensitive() {
    let stmts = parse_source("<?php function fail(): NEVER { throw new \\Exception(); }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { return_type, .. } => {
            assert_eq!(return_type.as_ref(), Some(&TypeExpr::Never));
        }
        other => panic!("Expected FunctionDecl, got {:?}", other),
    }
}

/// Parses a class with `public function fail(): never { ... }` and asserts the
/// method name, return type, and that `is_static = false`.
#[test]
fn test_parse_never_return_type_on_instance_method() {
    let stmts = parse_source(
        "<?php class Failer { public function fail(): never { throw new \\Exception(); } }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl { name, methods, .. } => {
            assert_eq!(name, "Failer");
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "fail");
            assert_eq!(methods[0].return_type.as_ref(), Some(&TypeExpr::Never));
            assert!(!methods[0].is_static);
        }
        other => panic!("Expected ClassDecl, got {:?}", other),
    }
}

/// Parses a class with `public static function fail(): never { ... }` and asserts
/// the method name, return type, and that `is_static = true`.
#[test]
fn test_parse_never_return_type_on_static_method() {
    let stmts = parse_source(
        "<?php class Failer { public static function fail(): never { throw new \\Exception(); } }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl { name, methods, .. } => {
            assert_eq!(name, "Failer");
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "fail");
            assert_eq!(methods[0].return_type.as_ref(), Some(&TypeExpr::Never));
            assert!(methods[0].is_static);
        }
        other => panic!("Expected ClassDecl, got {:?}", other),
    }
}

/// Parses an interface with `public function fail(): never;` and verifies the
/// `InterfaceDecl` method name and `TypeExpr::Never` return type.
#[test]
fn test_parse_never_return_type_on_interface_method() {
    let stmts = parse_source(
        "<?php interface Failer { public function fail(): never; }",
    );
    match &stmts[0].kind {
        StmtKind::InterfaceDecl { name, methods, .. } => {
            assert_eq!(name, "Failer");
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "fail");
            assert_eq!(methods[0].return_type.as_ref(), Some(&TypeExpr::Never));
        }
        other => panic!("Expected InterfaceDecl, got {:?}", other),
    }
}
