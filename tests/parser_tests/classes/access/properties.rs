//! Purpose:
//! Integration or regression tests for parser AST coverage of class properties, including property access, property array access, and property assign.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_parse_property_access() {
    let stmts = parse_source("<?php echo $obj->prop;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "prop");
                assert!(matches!(object.kind, ExprKind::Variable(_)));
            }
            _ => panic!("Expected PropertyAccess"),
        },
        _ => panic!("Expected Echo"),
    }
}

#[test]
fn test_parse_property_array_access() {
    let stmts = parse_source("<?php echo $obj->items[0];");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::ArrayAccess { array, index } => {
                assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                match &array.kind {
                    ExprKind::PropertyAccess { object, property } => {
                        assert_eq!(property, "items");
                        assert!(matches!(object.kind, ExprKind::Variable(_)));
                    }
                    other => panic!("Expected PropertyAccess, got {:?}", other),
                }
            }
            other => panic!("Expected ArrayAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_property_assign() {
    let stmts = parse_source("<?php $obj->prop = 42;");
    match &stmts[0].kind {
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => {
            assert_eq!(property, "prop");
            assert!(matches!(object.kind, ExprKind::Variable(_)));
            assert!(matches!(value.kind, ExprKind::IntLiteral(42)));
        }
        _ => panic!("Expected PropertyAssign"),
    }
}

#[test]
fn test_parse_property_compound_assignment() {
    let stmts = parse_source("<?php $obj->prop += 42;");
    match &stmts[0].kind {
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => {
            assert_eq!(property, "prop");
            assert!(matches!(object.kind, ExprKind::Variable(_)));
            match &value.kind {
                ExprKind::BinaryOp { left, op, right } => {
                    assert_eq!(op, &BinOp::Add);
                    assert!(matches!(right.kind, ExprKind::IntLiteral(42)));
                    assert!(matches!(left.kind, ExprKind::PropertyAccess { .. }));
                }
                other => panic!("Expected BinaryOp value, got {:?}", other),
            }
        }
        other => panic!("Expected PropertyAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_property_array_push() {
    let stmts = parse_source("<?php $obj->entries[] = $item;");
    match &stmts[0].kind {
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => {
            assert_eq!(property, "entries");
            assert!(matches!(object.kind, ExprKind::Variable(_)));
            assert!(matches!(value.kind, ExprKind::Variable(_)));
        }
        other => panic!("Expected PropertyArrayPush, got {:?}", other),
    }
}

#[test]
fn test_parse_property_array_assign() {
    let stmts = parse_source("<?php $obj->items[0] = 42;");
    match &stmts[0].kind {
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => {
            assert_eq!(property, "items");
            assert!(matches!(object.kind, ExprKind::Variable(_)));
            assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
            assert!(matches!(value.kind, ExprKind::IntLiteral(42)));
        }
        other => panic!("Expected PropertyArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_property_array_compound_assignment() {
    let stmts = parse_source("<?php $obj->items[0] *= 2;");
    match &stmts[0].kind {
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => {
            assert_eq!(property, "items");
            assert!(matches!(object.kind, ExprKind::Variable(_)));
            assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
            match &value.kind {
                ExprKind::BinaryOp { left, op, right } => {
                    assert_eq!(op, &BinOp::Mul);
                    assert!(matches!(right.kind, ExprKind::IntLiteral(2)));
                    assert!(matches!(left.kind, ExprKind::ArrayAccess { .. }));
                }
                other => panic!("Expected BinaryOp value, got {:?}", other),
            }
        }
        other => panic!("Expected PropertyArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_final_property_flag() {
    let stmts = parse_source("<?php class User { final public $id = 1; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, "id");
            assert!(properties[0].is_final);
            assert!(!properties[0].readonly);
        }
        other => panic!("Expected ClassDecl with final property, got {:?}", other),
    }
}
