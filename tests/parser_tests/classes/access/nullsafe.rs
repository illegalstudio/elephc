//! Purpose:
//! Integration or regression tests for parser AST coverage of class access, including nullsafe property access, nullsafe method call, and chained nullsafe access.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_parse_nullsafe_property_access() {
    let stmts = parse_source("<?php echo $obj?->prop;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::NullsafePropertyAccess { object, property } => {
                assert_eq!(property, "prop");
                assert!(matches!(object.kind, ExprKind::Variable(_)));
            }
            other => panic!("Expected NullsafePropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_nullsafe_method_call() {
    let stmts = parse_source("<?php $obj?->run(1, 2);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::NullsafeMethodCall {
                object,
                method,
                args,
            } => {
                assert_eq!(method, "run");
                assert_eq!(args.len(), 2);
                assert!(matches!(object.kind, ExprKind::Variable(_)));
            }
            other => panic!("Expected NullsafeMethodCall, got {:?}", other),
        },
        other => panic!("Expected ExprStmt, got {:?}", other),
    }
}

#[test]
fn test_parse_chained_nullsafe_access() {
    let stmts = parse_source("<?php echo $user?->profile?->name;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::NullsafePropertyAccess { object, property } => {
                assert_eq!(property, "name");
                match &object.kind {
                    ExprKind::NullsafePropertyAccess { object, property } => {
                        assert_eq!(property, "profile");
                        assert!(matches!(object.kind, ExprKind::Variable(ref name) if name == "user"));
                    }
                    other => panic!("Expected nested NullsafePropertyAccess, got {:?}", other),
                }
            }
            other => panic!("Expected NullsafePropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_mixed_nullsafe_then_member_chain() {
    let stmts = parse_source("<?php echo $a?->b->c;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "c");
                match &object.kind {
                    ExprKind::NullsafePropertyAccess { object, property } => {
                        assert_eq!(property, "b");
                        assert!(matches!(object.kind, ExprKind::Variable(ref name) if name == "a"));
                    }
                    other => panic!("Expected NullsafePropertyAccess, got {:?}", other),
                }
            }
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_nullsafe_middle_member_chain() {
    let stmts = parse_source("<?php echo $a->b?->c->d;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "d");
                match &object.kind {
                    ExprKind::NullsafePropertyAccess { object, property } => {
                        assert_eq!(property, "c");
                        match &object.kind {
                            ExprKind::PropertyAccess { object, property } => {
                                assert_eq!(property, "b");
                                assert!(
                                    matches!(object.kind, ExprKind::Variable(ref name) if name == "a")
                                );
                            }
                            other => panic!("Expected PropertyAccess, got {:?}", other),
                        }
                    }
                    other => panic!("Expected NullsafePropertyAccess, got {:?}", other),
                }
            }
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_nullsafe_chain_with_array_suffix() {
    let stmts = parse_source("<?php echo $a?->b[0]->c;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "c");
                match &object.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                        match &array.kind {
                            ExprKind::NullsafePropertyAccess { object, property } => {
                                assert_eq!(property, "b");
                                assert!(
                                    matches!(object.kind, ExprKind::Variable(ref name) if name == "a")
                                );
                            }
                            other => panic!("Expected NullsafePropertyAccess, got {:?}", other),
                        }
                    }
                    other => panic!("Expected ArrayAccess, got {:?}", other),
                }
            }
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}
