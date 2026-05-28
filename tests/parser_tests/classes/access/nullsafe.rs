//! Purpose:
//! Integration or regression tests for parser AST coverage of class access, including nullsafe property access, nullsafe method call, and chained nullsafe access.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Parses `<?php echo $obj?->prop;` and verifies that the AST produces a
/// `NullsafePropertyAccess` node with a variable object and "prop" property.
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

/// Parses `<?php echo $obj?->{$name};` and verifies that the AST produces a
/// `NullsafeDynamicPropertyAccess` with variable object and variable property.
#[test]
fn test_parse_nullsafe_dynamic_property_access() {
    let stmts = parse_source("<?php echo $obj?->{$name};");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                assert!(matches!(object.kind, ExprKind::Variable(ref name) if name == "obj"));
                assert!(matches!(property.kind, ExprKind::Variable(ref name) if name == "name"));
            }
            other => panic!("Expected NullsafeDynamicPropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

/// Parses `<?php $obj?->run(1, 2);` and verifies that the AST produces a
/// `NullsafeMethodCall` node with method "run" and two arguments.
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

/// Parses `<?php echo $user?->profile?->name;` and verifies that chained nullsafe
/// access produces a nested `NullsafePropertyAccess` AST structure: outer accesses
/// "name", inner accesses "profile" on variable "$user".
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

/// Parses `<?php echo $a?->b->c;` and verifies that a nullsafe access followed by
/// regular property access produces `PropertyAccess` (for "c") wrapping
/// `NullsafePropertyAccess` (for "b") wrapping variable "$a".
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

/// Parses `<?php echo $a->b?->c->d;` and verifies that regular property access
/// followed by nullsafe access and then regular access produces the correct
/// nesting: `PropertyAccess` ("d") over `NullsafePropertyAccess` ("c") over
/// `PropertyAccess` ("b") over variable "$a".
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

/// Parses `<?php echo $a?->b[0]->c;` and verifies that nullsafe property access
/// with an array subscript in the middle produces `PropertyAccess` ("c") over
/// `ArrayAccess` over `NullsafePropertyAccess` ("b") over variable "$a".
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
