//! Purpose:
//! Integration or regression tests for parser AST coverage of functions, including function declaration parses, mixed case php keywords, and function no params.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets cover successful AST shapes plus malformed syntax that must fail during parsing.

use super::*;

#[test]
// Verifies that `<?php function foo($a, $b) { return $a; }` parses to a `FunctionDecl` with
// name "foo", two parameters ["a", "b"], and a body containing one return statement.
/// Verifies that function declaration parses.
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
// Verifies that `<?php FUNCTION Foo() { RETURN TRUE; } IF (FALSE) { ECHO 1; }` parses with
// case-insensitive keywords: function name "Foo", `RETURN` producing a `Return(Some(...))`,
// and `IF` producing an `If` statement.
/// Verifies that parse mixed case PHP keywords.
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
// Verifies that `<?php function noop() { return; }` parses to a `FunctionDecl` with no
// parameters and an empty body containing a bare `return;`.
/// Verifies that function no params.
fn test_function_no_params() {
    let stmts = parse_source("<?php function noop() { return; }");
    if let StmtKind::FunctionDecl { params, .. } = &stmts[0].kind {
        assert!(params.is_empty());
    }
}

#[test]
// Verifies that `<?php $fn = function($x) { return $x; };` parses to an `Assign` statement
// with an `ExprKind::Closure` that has one parameter "x" and `is_arrow` false.
/// Verifies that parse closure.
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
// Verifies that `<?php $fn = function(int &$x) { return $x; };` parses a closure with a
// typed by-reference parameter: type `Int`, name "x", and `by_ref` flag set.
/// Verifies that parse typed closure param.
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
// Verifies that `<?php $x = 1; $fn = function(int $n) use ($x): string { return "ok"; };`
// parses a closure with: typed param `int $n`, capture `$x`, return type `string`, and
// `is_arrow` false.
/// Verifies that parse closure return type after use.
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
// Verifies that `<?php $g = function(int $n) use (&$g): int { return $g($n - 1); };`
// parses a closure with by-reference capture `&$g`: name "g", `capture_refs` includes "g".
/// Verifies that parse closure use by reference capture.
fn test_parse_closure_use_by_reference_capture() {
    let stmts =
        parse_source("<?php $g = function(int $n) use (&$g): int { return $g($n - 1); };");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params,
            captures,
            capture_refs,
            return_type,
            ..
        } = &value.kind
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "n");
            assert_eq!(captures, &["g".to_string()]);
            assert_eq!(capture_refs, &["g".to_string()]);
            assert_eq!(return_type.as_ref(), Some(&TypeExpr::Int));
        } else {
            panic!("expected Closure");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
// Verifies that `<?php $fn = fn($x) => $x * 2;` parses to a closure with `is_arrow` true
// and one parameter "x".
/// Verifies that parse arrow function.
fn test_parse_arrow_function() {
    let stmts = parse_source("<?php $fn = fn($x) => $x * 2;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params,
            captures,
            is_arrow,
            ..
        } = &value.kind
        {
            let param_names: Vec<&str> = params.iter().map(|(n, _, _, _)| n.as_str()).collect();
            assert_eq!(param_names, &["x"]);
            assert!(captures.is_empty());
            assert!(is_arrow);
        } else {
            panic!("expected Closure (arrow)");
        }
    } else {
        panic!("expected Assign");
    }
}

/// Verifies arrow functions record outer variable reads as implicit by-value captures.
#[test]
fn test_parse_arrow_function_implicit_capture() {
    let stmts = parse_source("<?php $fn = fn($x) => $x + $y;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            captures,
            is_arrow,
            ..
        } = &value.kind
        {
            assert!(is_arrow);
            assert_eq!(captures, &["y".to_string()]);
        } else {
            panic!("expected Closure (arrow)");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
// Verifies that `<?php $fn = fn(int $x): int => $x + 1;` parses an arrow function with
// typed param `int $x`, return type `int`, and `is_arrow` true.
/// Verifies that parse arrow function return type.
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
// Verifies that `<?php $fn = fn(string $label) => $label;` parses an arrow function with
// typed param `string $label` and `is_arrow` true.
/// Verifies that parse typed arrow function param.
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
// Verifies that `<?php $fn(1, 2);` parses a closure call expression with var "fn" and
// two positional arguments.
/// Verifies that parse closure call.
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
// Verifies that `<?php greet(name: "Alice", age: 30);` parses a named function call with
// two `NamedArg` expressions for "name" and "age".
/// Verifies that parse named function call.
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
// Verifies that `<?php $user = new User(id: 42);` parses a named constructor call with
// class "User", one `NamedArg` for "id", and an integer literal value.
/// Verifies that parse named constructor call.
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

/// Verifies that parse keyword named constructor call.
#[test]
fn test_parse_keyword_named_constructor_call() {
    let stmts = parse_source("<?php $user = new User(class: 42);");
    if let StmtKind::Assign { value: expr, .. } = &stmts[0].kind {
        if let ExprKind::NewObject { class_name, args } = &expr.kind {
            assert_eq!(class_name.as_str(), "User");
            assert_eq!(args.len(), 1);
            assert!(matches!(
                args[0].kind,
                ExprKind::NamedArg { ref name, .. } if name == "class"
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
// Verifies that `<?php function foo($a, $b = 10) { return $a + $b; }` parses a function
// declaration where the second param "b" has a default value expression and the first
// param "a" does not.
/// Verifies that parse function default params.
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
// Verifies that `<?php function foo(&$x) { }` parses a function declaration with a single
// by-reference parameter "x" (pass-by-reference flag set).
/// Verifies that parse ref param.
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
// Verifies that `<?php function foo(&$a, $b, &$c) { }` parses a function declaration with
// mixed ref/non-ref parameters: first and third are by-reference, second is not.
/// Verifies that parse mixed ref params.
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
// Verifies that `<?php function foo($x) { }` parses a normal (non-ref) parameter and that
// the pass-by-reference flag is unset.
/// Verifies that parse non ref param.
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
// Verifies that `<?php function foo(int $x): string { return "ok"; }` parses a function
// declaration with a typed parameter `int $x` and return type `string`.
/// Verifies that parse typed function param and return type.
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
// Verifies that `<?php function describe(int|string $value): ?int { return null; }` parses
// a function with a union type param `int|string` and nullable return type `?int`.
/// Verifies that parse union and nullable function types.
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
// Verifies that `<?php function bump(int &$x) { }` parses a typed by-reference parameter:
// type `Int`, name "x", and pass-by-reference flag set.
/// Verifies that parse typed ref param.
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
// Verifies that `<?php function walk(iterable $items): iterable { return $items; }` parses
// a function with `iterable` type on both the parameter and the return type.
/// Verifies that parse iterable type.
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
// Verifies that `<?php function foo(...$args) { }` parses a variadic-only function with
// an empty param list and `variadic` set to `Some("args")`.
/// Verifies that parse variadic function.
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
// Verifies that `<?php function foo($a, $b, ...$rest) { }` parses a variadic function with
// two regular parameters "a" and "b", and `variadic` set to `Some("rest")`.
/// Verifies that parse variadic with regular params.
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
// Verifies that `<?php function foo($a) { }` parses a non-variadic function and that
// `variadic` is `None`.
/// Verifies that parse no variadic.
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
// Verifies that `<?php function foo(int ...$xs) { }` fails to parse because typed
// variadic parameters are not permitted.
/// Verifies that parse typed variadic param fails.
fn test_parse_typed_variadic_param_fails() {
    assert!(parse_fails("<?php function foo(int ...$xs) { }"));
}

#[test]
// Verifies that `<?php foo(...$arr);` parses a function call with a spread argument
// (single `Spread` expression inside args).
/// Verifies that parse spread in function call.
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
// Verifies that `<?php $x = [...$a, ...$b];` parses an array literal containing two
// spread elements in source order.
/// Verifies that parse spread in array literal.
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
// Verifies that `<?php echo getColor()[0];` parses correctly with an `Echo` statement
// containing an `ArrayAccess` whose array operand is a `FunctionCall` to `getColor`
// with no arguments and integer index `0`.
/// Verifies that parse array access on function call result.
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
// Verifies that `<?php $f = strlen(...);` parses a first-class callable expression
// wrapping the builtin function "strlen".
/// Verifies that parse first class callable function.
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
// Verifies that `<?php echo __FUNCTION__;` parses an echo statement whose expression is
// the magic constant `MagicConstant::Function`.
/// Verifies that parse dunder function magic constant.
fn test_parse_dunder_function_magic_constant() {
    let stmts = parse_source("<?php echo __FUNCTION__;");
    assert_eq!(
        echoed_expr(&stmts),
        &ExprKind::MagicConstant(MagicConstant::Function)
    );
}

#[test]
// Verifies that a trailing comma after the last call argument (PHP 7.3+) is accepted:
// `<?php greet(1, 2,);` parses to a `FunctionCall` with exactly two arguments.
/// Verifies that a trailing comma in a call argument list parses.
fn test_parse_trailing_comma_in_call_args() {
    let stmts = parse_source("<?php greet(1, 2,);");
    if let StmtKind::ExprStmt(expr) = &stmts[0].kind {
        if let ExprKind::FunctionCall { name, args } = &expr.kind {
            assert_eq!(name.as_str(), "greet");
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected FunctionCall, got {:?}", expr.kind);
        }
    } else {
        panic!("expected ExprStmt");
    }
}

#[test]
// Verifies that a trailing comma after the last parameter (PHP 8.0+) is accepted:
// `<?php function foo($a, $b,) {}` parses to a `FunctionDecl` with two parameters.
/// Verifies that a trailing comma in a parameter list parses.
fn test_parse_trailing_comma_in_param_list() {
    let stmts = parse_source("<?php function foo($a, $b,) { return $a; }");
    if let StmtKind::FunctionDecl { name, params, .. } = &stmts[0].kind {
        assert_eq!(name, "foo");
        let param_names: Vec<&str> = params.iter().map(|(n, _, _, _)| n.as_str()).collect();
        assert_eq!(param_names, &["a", "b"]);
    } else {
        panic!("expected FunctionDecl");
    }
}

#[test]
// Verifies that a single trailing comma after one argument is accepted:
// `<?php greet(1,);` parses to a `FunctionCall` with exactly one argument.
/// Verifies that a single-argument trailing comma in a call parses.
fn test_parse_single_arg_trailing_comma_in_call() {
    let stmts = parse_source("<?php greet(1,);");
    if let StmtKind::ExprStmt(expr) = &stmts[0].kind {
        if let ExprKind::FunctionCall { name, args } = &expr.kind {
            assert_eq!(name.as_str(), "greet");
            assert_eq!(args.len(), 1);
        } else {
            panic!("expected FunctionCall, got {:?}", expr.kind);
        }
    } else {
        panic!("expected ExprStmt");
    }
}
