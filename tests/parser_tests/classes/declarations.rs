//! Purpose:
//! Integration or regression tests for parser AST coverage of class declarations, including class decl, new object, and class decl with extends.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_parse_class_decl() {
    let stmts = parse_source("<?php class Point { public $x; private $y = 1; public function get() { return $this->x; } public static function origin() { return new Point(); } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            trait_uses,
            properties,
            methods,
            ..
        } => {
            assert_eq!(name, "Point");
            assert_eq!(extends, &None);
            assert!(implements.is_empty());
            assert!(!is_abstract);
            assert!(trait_uses.is_empty());
            assert_eq!(properties.len(), 2);
            assert_eq!(properties[0].name, "x");
            assert_eq!(properties[0].visibility, Visibility::Public);
            assert_eq!(properties[1].name, "y");
            assert_eq!(properties[1].visibility, Visibility::Private);
            assert!(properties[1].default.is_some());
            assert_eq!(methods.len(), 2);
            assert_eq!(methods[0].name, "get");
            assert!(!methods[0].is_static);
            assert_eq!(methods[1].name, "origin");
            assert!(methods[1].is_static);
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_new_object() {
    let stmts = parse_source("<?php $p = new Point(1, 2);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::NewObject { class_name, args } => {
                assert_eq!(class_name, "Point");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected NewObject"),
        },
        _ => panic!("Expected Assign"),
    }
}

#[test]
fn test_parse_class_decl_with_extends() {
    let stmts =
        parse_source("<?php class Child extends Base { public function run() { return 1; } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            extends,
            methods,
            ..
        } => {
            assert_eq!(name, "Child");
            assert_eq!(extends.as_deref(), Some("Base"));
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "run");
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_interface_decl() {
    let stmts = parse_source(
        "<?php interface Named extends Renderable, Jsonable { public function name(); }",
    );
    match &stmts[0].kind {
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
            ..
        } => {
            assert_eq!(name, "Named");
            assert_eq!(
                extends,
                &vec!["Renderable".to_string(), "Jsonable".to_string()]
            );
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "name");
            assert!(methods[0].is_abstract);
            assert!(!methods[0].has_body);
            assert!(methods[0].body.is_empty());
        }
        _ => panic!("Expected InterfaceDecl"),
    }
}

#[test]
fn test_parse_new_self() {
    let stmts = parse_source("<?php echo new self();");
    match echoed_expr(&stmts) {
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Self_,
            args,
        } => assert!(args.is_empty()),
        other => panic!("expected NewScopedObject Self_, got {:?}", other),
    }
}

#[test]
fn test_parse_new_static() {
    let stmts = parse_source("<?php echo new static();");
    match echoed_expr(&stmts) {
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Static,
            args,
        } => assert!(args.is_empty()),
        other => panic!("expected NewScopedObject Static, got {:?}", other),
    }
}

#[test]
fn test_parse_new_parent_with_args() {
    let stmts = parse_source("<?php echo new parent(1, 2);");
    match echoed_expr(&stmts) {
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Parent,
            args,
        } => assert_eq!(args.len(), 2),
        other => panic!("expected NewScopedObject Parent, got {:?}", other),
    }
}

// --- Static closures ---
