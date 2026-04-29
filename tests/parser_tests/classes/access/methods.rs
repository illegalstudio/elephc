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
