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
