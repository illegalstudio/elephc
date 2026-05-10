//! Purpose:
//! Integration or regression tests for parser AST coverage of class static properties, including static property access, static property assignment, and static property compound assignment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_parse_static_property_access() {
    let stmts = parse_source("<?php echo Counter::$count;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                assert_eq!(receiver, &StaticReceiver::Named("Counter".into()));
                assert_eq!(property, "count");
            }
            _ => panic!("Expected StaticPropertyAccess"),
        },
        _ => panic!("Expected Echo"),
    }
}

#[test]
fn test_parse_static_property_assignment() {
    let stmts = parse_source("<?php self::$count = 2;");
    match &stmts[0].kind {
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => {
            assert_eq!(receiver, &StaticReceiver::Self_);
            assert_eq!(property, "count");
            assert!(matches!(value.kind, ExprKind::IntLiteral(2)));
        }
        _ => panic!("Expected StaticPropertyAssign"),
    }
}

#[test]
fn test_parse_static_property_compound_assignment() {
    let stmts = parse_source("<?php Counter::$count += 2;");
    match &stmts[0].kind {
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => {
            assert_eq!(receiver, &StaticReceiver::Named("Counter".into()));
            assert_eq!(property, "count");
            match &value.kind {
                ExprKind::BinaryOp { left, op, right } => {
                    assert_eq!(op, &BinOp::Add);
                    assert!(matches!(right.kind, ExprKind::IntLiteral(2)));
                    match &left.kind {
                        ExprKind::StaticPropertyAccess { receiver, property } => {
                            assert_eq!(receiver, &StaticReceiver::Named("Counter".into()));
                            assert_eq!(property, "count");
                        }
                        other => panic!("Expected StaticPropertyAccess lhs, got {:?}", other),
                    }
                }
                other => panic!("Expected BinaryOp value, got {:?}", other),
            }
        }
        other => panic!("Expected StaticPropertyAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_static_property_array_push() {
    let stmts = parse_source("<?php Counter::$items[] = 2;");
    match &stmts[0].kind {
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => {
            assert_eq!(receiver, &StaticReceiver::Named("Counter".into()));
            assert_eq!(property, "items");
            assert!(matches!(value.kind, ExprKind::IntLiteral(2)));
        }
        other => panic!("Expected StaticPropertyArrayPush, got {:?}", other),
    }
}

#[test]
fn test_parse_static_property_array_assign() {
    let stmts = parse_source("<?php Counter::$items[1] = 3;");
    match &stmts[0].kind {
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => {
            assert_eq!(receiver, &StaticReceiver::Named("Counter".into()));
            assert_eq!(property, "items");
            assert!(matches!(index.kind, ExprKind::IntLiteral(1)));
            assert!(matches!(value.kind, ExprKind::IntLiteral(3)));
        }
        other => panic!("Expected StaticPropertyArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_static_property_array_compound_assignment() {
    let stmts = parse_source("<?php Counter::$items[1] ??= 3;");
    match &stmts[0].kind {
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => {
            assert_eq!(receiver, &StaticReceiver::Named("Counter".into()));
            assert_eq!(property, "items");
            assert!(matches!(index.kind, ExprKind::IntLiteral(1)));
            match &value.kind {
                ExprKind::NullCoalesce { value, default } => {
                    assert!(matches!(default.kind, ExprKind::IntLiteral(3)));
                    match &value.kind {
                        ExprKind::ArrayAccess { array, index } => {
                            assert!(matches!(index.kind, ExprKind::IntLiteral(1)));
                            assert!(matches!(array.kind, ExprKind::StaticPropertyAccess { .. }));
                        }
                        other => panic!("Expected ArrayAccess lhs, got {:?}", other),
                    }
                }
                other => panic!("Expected NullCoalesce value, got {:?}", other),
            }
        }
        other => panic!("Expected StaticPropertyArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_static_property_declaration() {
    let stmts = parse_source("<?php class Counter { public static int $count = 1; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, "count");
            assert_eq!(properties[0].visibility, Visibility::Public);
            assert!(properties[0].is_static);
            assert!(properties[0].type_expr.is_some());
            assert!(properties[0].default.is_some());
        }
        _ => panic!("Expected ClassDecl"),
    }
}
