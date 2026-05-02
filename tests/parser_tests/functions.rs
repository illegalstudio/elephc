use super::*;

#[test]
fn test_function_declaration_parses() {
    let stmts = parse_source("<?php function foo($a, $b) { return $a; }");
    if let StmtKind::FunctionDecl {
        name, params, body, ..
    } = &stmts[0].kind
    {
        assert_eq!(name, "foo");
        let param_names: Vec<&str> = params.iter().map(|(n, _, _, _)| n.as_str()).collect();
        assert_eq!(param_names, &["a", "b"]);
        assert_eq!(body.len(), 1);
    } else {
        panic!("expected FunctionDecl");
    }
}

#[test]
fn test_parse_mixed_case_php_keywords() {
    let stmts = parse_source("<?php FUNCTION Foo() { RETURN TRUE; } IF (FALSE) { ECHO 1; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { name, body, .. } => {
            assert_eq!(name, "Foo");
            assert!(matches!(body[0].kind, StmtKind::Return(Some(_))));
        }
        other => panic!("expected FunctionDecl, got {:?}", other),
    }
    assert!(matches!(stmts[1].kind, StmtKind::If { .. }));
}

#[test]
fn test_function_no_params() {
    let stmts = parse_source("<?php function noop() { return; }");
    if let StmtKind::FunctionDecl { params, .. } = &stmts[0].kind {
        assert!(params.is_empty());
    }
}

#[test]
fn test_parse_closure() {
    let stmts = parse_source("<?php $fn = function($x) { return $x; };");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params, is_arrow, ..
        } = &value.kind
        {
            let param_names: Vec<&str> = params.iter().map(|(n, _, _, _)| n.as_str()).collect();
            assert_eq!(param_names, &["x"]);
            assert!(!is_arrow);
        } else {
            panic!("expected Closure");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_typed_closure_param() {
    let stmts = parse_source("<?php $fn = function(int &$x) { return $x; };");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params, is_arrow, ..
        } = &value.kind
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "x");
            assert_eq!(params[0].1, Some(TypeExpr::Int));
            assert!(params[0].3);
            assert!(!is_arrow);
        } else {
            panic!("expected Closure");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_closure_return_type_after_use() {
    let stmts =
        parse_source("<?php $x = 1; $fn = function(int $n) use ($x): string { return \"ok\"; };");
    assert_eq!(stmts.len(), 2);
    if let StmtKind::Assign { value, .. } = &stmts[1].kind {
        if let ExprKind::Closure {
            params,
            captures,
            return_type,
            is_arrow,
            ..
        } = &value.kind
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "n");
            assert_eq!(params[0].1, Some(TypeExpr::Int));
            assert_eq!(captures, &["x".to_string()]);
            assert_eq!(return_type.as_ref(), Some(&TypeExpr::Str));
            assert!(!is_arrow);
        } else {
            panic!("expected Closure");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_arrow_function() {
    let stmts = parse_source("<?php $fn = fn($x) => $x * 2;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params, is_arrow, ..
        } = &value.kind
        {
            let param_names: Vec<&str> = params.iter().map(|(n, _, _, _)| n.as_str()).collect();
            assert_eq!(param_names, &["x"]);
            assert!(is_arrow);
        } else {
            panic!("expected Closure (arrow)");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_arrow_function_return_type() {
    let stmts = parse_source("<?php $fn = fn(int $x): int => $x + 1;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params,
            return_type,
            is_arrow,
            ..
        } = &value.kind
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "x");
            assert_eq!(params[0].1, Some(TypeExpr::Int));
            assert_eq!(return_type.as_ref(), Some(&TypeExpr::Int));
            assert!(is_arrow);
        } else {
            panic!("expected Closure (arrow)");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_typed_arrow_function_param() {
    let stmts = parse_source("<?php $fn = fn(string $label) => $label;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params, is_arrow, ..
        } = &value.kind
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "label");
            assert_eq!(params[0].1, Some(TypeExpr::Str));
            assert!(is_arrow);
        } else {
            panic!("expected Closure (arrow)");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_closure_call() {
    let stmts = parse_source("<?php $fn(1, 2);");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::ExprStmt(expr) = &stmts[0].kind {
        if let ExprKind::ClosureCall { var, args } = &expr.kind {
            assert_eq!(var, "fn");
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected ClosureCall");
        }
    } else {
        panic!("expected ExprStmt");
    }
}

#[test]
fn test_parse_named_function_call() {
    let stmts = parse_source("<?php greet(name: \"Alice\", age: 30);");
    if let StmtKind::ExprStmt(expr) = &stmts[0].kind {
        if let ExprKind::FunctionCall { name, args } = &expr.kind {
            assert_eq!(name.as_str(), "greet");
            assert_eq!(args.len(), 2);
            assert!(matches!(
                args[0].kind,
                ExprKind::NamedArg { ref name, .. } if name == "name"
            ));
            assert!(matches!(
                args[1].kind,
                ExprKind::NamedArg { ref name, .. } if name == "age"
            ));
        } else {
            panic!("expected FunctionCall");
        }
    } else {
        panic!("expected ExprStmt");
    }
}

#[test]
fn test_parse_named_constructor_call() {
    let stmts = parse_source("<?php $user = new User(id: 42);");
    if let StmtKind::Assign { value: expr, .. } = &stmts[0].kind {
        if let ExprKind::NewObject { class_name, args } = &expr.kind {
            assert_eq!(class_name.as_str(), "User");
            assert_eq!(args.len(), 1);
            assert!(matches!(
                args[0].kind,
                ExprKind::NamedArg { ref name, .. } if name == "id"
            ));
        } else {
            panic!("expected NewObject");
        }
    } else {
        panic!("expected Assign");
    }
}

// --- Default parameter values ---

#[test]
fn test_parse_function_default_params() {
    let stmts = parse_source("<?php function foo($a, $b = 10) { return $a + $b; }");
    if let StmtKind::FunctionDecl { params, .. } = &stmts[0].kind {
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "a");
        assert!(params[0].2.is_none());
        assert_eq!(params[1].0, "b");
        assert!(params[1].2.is_some());
    } else {
        panic!("expected FunctionDecl");
    }
}

// --- Bitwise operator precedence ---

#[test]
fn test_parse_ref_param() {
    let stmts = parse_source("<?php function foo(&$x) { }");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::FunctionDecl { name, params, .. } => {
            assert_eq!(name, "foo");
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "x");
            assert!(params[0].3, "Expected param to be pass-by-reference");
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_mixed_ref_params() {
    let stmts = parse_source("<?php function foo(&$a, $b, &$c) { }");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(params.len(), 3);
            assert!(params[0].3, "First param should be ref");
            assert!(!params[1].3, "Second param should not be ref");
            assert!(params[2].3, "Third param should be ref");
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_non_ref_param() {
    let stmts = parse_source("<?php function foo($x) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert!(!params[0].3, "Normal param should not be ref");
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_typed_function_param_and_return_type() {
    let stmts = parse_source("<?php function foo(int $x): string { return \"ok\"; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            name,
            params,
            return_type,
            ..
        } => {
            assert_eq!(name, "foo");
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "x");
            assert_eq!(params[0].1, Some(TypeExpr::Int));
            assert_eq!(return_type.as_ref(), Some(&TypeExpr::Str));
        }
        other => panic!("Expected FunctionDecl, got {:?}", other),
    }
}

#[test]
fn test_parse_union_and_nullable_function_types() {
    let stmts = parse_source("<?php function describe(int|string $value): ?int { return null; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            params,
            return_type,
            ..
        } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::Union(vec![TypeExpr::Int, TypeExpr::Str]))
            );
            assert_eq!(
                return_type.as_ref(),
                Some(&TypeExpr::Nullable(Box::new(TypeExpr::Int)))
            );
        }
        other => panic!("Expected FunctionDecl, got {:?}", other),
    }
}

#[test]
fn test_parse_typed_ref_param() {
    let stmts = parse_source("<?php function bump(int &$x) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "x");
            assert_eq!(params[0].1, Some(TypeExpr::Int));
            assert!(params[0].3);
        }
        other => panic!("Expected FunctionDecl, got {:?}", other),
    }
}

#[test]
fn test_parse_iterable_type() {
    let stmts = parse_source("<?php function walk(iterable $items): iterable { return $items; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            name,
            params,
            return_type,
            ..
        } => {
            assert_eq!(name, "walk");
            assert_eq!(params[0].1, Some(TypeExpr::Iterable));
            assert_eq!(return_type.as_ref(), Some(&TypeExpr::Iterable));
        }
        other => panic!("Expected FunctionDecl, got {:?}", other),
    }
}

// --- Variadic and Spread ---

#[test]
fn test_parse_variadic_function() {
    let stmts = parse_source("<?php function foo(...$args) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            ..
        } => {
            assert_eq!(name, "foo");
            assert!(params.is_empty());
            assert_eq!(variadic.as_deref(), Some("args"));
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_variadic_with_regular_params() {
    let stmts = parse_source("<?php function foo($a, $b, ...$rest) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            params, variadic, ..
        } => {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].0, "a");
            assert_eq!(params[1].0, "b");
            assert_eq!(variadic.as_deref(), Some("rest"));
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_no_variadic() {
    let stmts = parse_source("<?php function foo($a) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { variadic, .. } => {
            assert!(variadic.is_none());
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_typed_variadic_param_fails() {
    assert!(parse_fails("<?php function foo(int ...$xs) { }"));
}

#[test]
fn test_parse_spread_in_function_call() {
    let stmts = parse_source("<?php foo(...$arr);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::FunctionCall { args, .. } => {
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0].kind, ExprKind::Spread(_)));
            }
            _ => panic!("Expected FunctionCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_spread_in_array_literal() {
    let stmts = parse_source("<?php $x = [...$a, ...$b];");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::ArrayLiteral(elems) => {
                assert_eq!(elems.len(), 2);
                assert!(matches!(&elems[0].kind, ExprKind::Spread(_)));
                assert!(matches!(&elems[1].kind, ExprKind::Spread(_)));
            }
            _ => panic!("Expected ArrayLiteral"),
        },
        _ => panic!("Expected Assign"),
    }
}

#[test]
fn test_parse_array_access_on_function_call_result() {
    let stmts = parse_source("<?php echo getColor()[0];");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::ArrayAccess { array, index } => {
                assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                match &array.kind {
                    ExprKind::FunctionCall { name, args } => {
                        assert_eq!(name.as_str(), "getColor");
                        assert!(args.is_empty());
                    }
                    other => panic!("Expected FunctionCall, got {:?}", other),
                }
            }
            other => panic!("Expected ArrayAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_first_class_callable_function() {
    let stmts = parse_source("<?php $f = strlen(...);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
                assert_eq!(name.as_str(), "strlen");
            }
            other => panic!("Expected function first-class callable, got {:?}", other),
        },
        other => panic!("Expected assignment, got {:?}", other),
    }
}

#[test]
fn test_parse_dunder_function_magic_constant() {
    let stmts = parse_source("<?php echo __FUNCTION__;");
    assert_eq!(
        echoed_expr(&stmts),
        &ExprKind::MagicConstant(MagicConstant::Function)
    );
}
