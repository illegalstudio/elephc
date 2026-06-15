//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc functions, including variadic missing variable, variadic not last, and first class callable method requires object receiver.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies that a variadic parameter `...` without a following variable name produces the
/// expected error message.
#[test]
fn test_error_variadic_missing_variable() {
    expect_error(
        "<?php function foo(... ) {}",
        "Expected variable after '...'",
    );
}

/// Verifies that a variadic parameter cannot be followed by another regular parameter.
#[test]
fn test_error_variadic_not_last() {
    expect_error(
        "<?php function foo(...$a, $b) {}",
        "Variadic parameter must be the last parameter",
    );
}

/// Verifies that a first-class callable using an object property (non-object receiver)
/// for a method call produces the expected error.
#[test]
fn test_error_first_class_callable_method_requires_object_receiver() {
    expect_error(
        "<?php $u = 1; $f = $u->greet(...);",
        "First-class method callable requires an object receiver",
    );
}

/// Verifies that first-class callable syntax is rejected for builtins that are not yet
/// supported (e.g., `isset`).
#[test]
fn test_error_first_class_callable_rejects_unsupported_builtin() {
    expect_error(
        "<?php $f = isset(...);",
        "does not support builtin 'isset' yet",
    );
}

/// Verifies that a by-reference parameter on a first-class callable must be passed a
/// variable at the call site; passing a literal produces an error.
#[test]
fn test_error_first_class_callable_ref_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); $f(1);",
        "parameter $n must be passed a variable",
    );
}

/// Verifies that a by-reference parameter on a closure must be passed a variable at
/// the call site; passing a literal produces an error.
#[test]
fn test_error_closure_ref_param_requires_variable() {
    expect_error(
        "<?php $f = function (&$x) { $x = $x + 1; }; $f(1);",
        "parameter $x must be passed a variable",
    );
}

/// Verifies that a typed parameter rejects a mismatched argument type at the call site.
#[test]
fn test_error_function_typed_param_rejects_wrong_argument() {
    expect_error(
        "<?php function foo(int $x) { echo $x; } foo(\"hello\");",
        "Function 'foo' parameter $x expects Int, got Str",
    );
}

/// Verifies that declaring two functions with names differing only by case produces a
/// duplicate declaration error (functions are case-sensitive in PHP).
#[test]
fn test_error_duplicate_functions_differing_only_by_case() {
    expect_error(
        "<?php function Foo() { return 1; } function foo() { return 2; }",
        "Duplicate function declaration: foo",
    );
}

/// Verifies that a user-defined function cannot shadow a builtin function even when
/// the case differs (e.g., `STRLEN` vs `strlen`).
#[test]
fn test_error_cannot_redeclare_builtin_function_differing_only_by_case() {
    expect_error(
        "<?php function STRLEN(string $value): int { return 0; }",
        "Cannot redeclare built-in function: strlen",
    );
}

/// Verifies that a user-defined function cannot shadow a filesystem builtin function
/// (e.g., `touch`) even when the signature differs.
#[test]
fn test_error_cannot_redeclare_filesystem_builtin_function() {
    expect_error(
        "<?php function touch(string $path): bool { return true; }",
        "Cannot redeclare built-in function: touch",
    );
}

/// Verifies that accessing a user-defined constant with incorrect casing produces
/// an undefined constant error (constants are case-sensitive in PHP).
#[test]
fn test_error_user_constants_are_case_sensitive() {
    expect_error(
        "<?php const MyConst = 1; echo myconst;",
        "Undefined constant: myconst",
    );
}

/// Verifies that a typed parameter with a default value that does not match the
/// declared type produces a type mismatch error at declaration time.
#[test]
fn test_error_typed_default_parameter_rejects_mismatched_default() {
    expect_error(
        "<?php function foo(int $x = \"hello\") { echo $x; }",
        "Function 'foo' parameter $x expects Int, got Str",
    );
}

/// Verifies that passing a named argument that does not match any parameter name
/// produces an error.
#[test]
fn test_error_named_arguments_reject_unknown_parameter() {
    expect_error(
        "<?php function greet($name) { echo $name; } greet(age: 30);",
        "Function 'greet' has no parameter $age",
    );
}

/// Verifies an unknown string-literal callback passed to `call_user_func_array` with a
/// runtime-opaque (non-array-literal) second argument is rejected as an undefined function,
/// instead of being accepted and deferring to a mangle panic / dangling symbol at codegen.
#[test]
fn test_error_call_user_func_array_unknown_string_callback() {
    expect_error(
        "<?php $args = [1, 2]; call_user_func_array(\"does_not_exist\", $args);",
        "Undefined function: does_not_exist",
    );
}

/// Verifies that positional arguments cannot follow named arguments in a call.
#[test]
fn test_error_named_arguments_reject_positional_after_named() {
    expect_error(
        "<?php function greet($name, $age) { echo $name; } greet(name: \"Alice\", 30);",
        "Function 'greet' cannot use positional arguments after named arguments",
    );
}

/// Verifies that a parameter cannot be assigned twice via named arguments.
#[test]
fn test_error_named_arguments_reject_duplicate_assignment() {
    expect_error(
        "<?php function greet($name) { echo $name; } greet(\"Alice\", name: \"Bob\");",
        "Function 'greet' parameter $name is already assigned",
    );
}

/// Verifies that spread arguments from associative arrays are subject to the same
/// unknown-parameter checks as regular named arguments.
#[test]
fn test_error_named_arguments_reject_unknown_assoc_spread_literal_parameter() {
    expect_error(
        "<?php function greet($name) { echo $name; } greet(...[\"age\" => 30]);",
        "Function 'greet' has no parameter $age",
    );
}

/// Verifies that a parameter cannot be assigned twice even when one assignment comes
/// from a spread associative literal and another from a named argument.
#[test]
fn test_error_named_arguments_reject_duplicate_assoc_spread_literal_assignment() {
    expect_error(
        "<?php function greet($name) { echo $name; } greet(...[\"name\" => \"Alice\"], name: \"Bob\");",
        "Function 'greet' parameter $name is already assigned",
    );
}

/// Verifies that named arguments targeting a builtin function with an unknown
/// parameter name (e.g., `strlen(value: ...)` instead of `strlen(value:)`)
/// produce an error.
#[test]
fn test_error_named_arguments_reject_unknown_builtin_parameter() {
    expect_error(
        "<?php strlen(value: \"hello\");",
        "Builtin 'strlen' has no parameter $value",
    );
}

/// Verifies that builtin variadic parameters cannot be addressed by name when the
/// builtin does not declare that parameter (e.g., `printf(values: ...)` where
/// `printf` has a variadic `format` and no `values` parameter).
#[test]
fn test_error_named_arguments_reject_builtin_variadic_named_parameter() {
    expect_error(
        "<?php printf(format: \"%s\", values: \"hello\");",
        "Builtin 'printf' has no parameter $values",
    );
}

/// Verifies that positional arguments cannot follow spread arguments in a call.
#[test]
fn test_error_named_arguments_reject_positional_after_spread() {
    expect_error(
        "<?php function greet($name, $age) { echo $name; } $args = [\"Alice\"]; greet(...$args, 30);",
        "Function 'greet' cannot use positional arguments after spread arguments",
    );
}

/// Verifies that spread arguments cannot follow named arguments in a call.
#[test]
fn test_error_named_arguments_reject_spread_after_named() {
    expect_error(
        "<?php function greet($name, $age) { echo $name; } $args = [30]; greet(name: \"Alice\", ...$args);",
        "Function 'greet' cannot use argument unpacking after named arguments",
    );
}

/// Verifies that even when a spread provides positional arguments, named arguments
/// are still processed, and a missing required parameter is still reported.
#[test]
fn test_error_named_arguments_after_positional_spread_still_rejects_missing_required_param() {
    expect_error(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$args = [10];
echo sum3(...$args, a: 1, b: 20);
"#,
        "Function 'sum3' missing required parameter $c",
    );
}

/// Verifies that extern functions (FFI) reject named arguments with unknown
/// parameter names.
#[test]
fn test_error_named_arguments_reject_unknown_extern_parameter() {
    expect_error(
        "<?php extern function abs(int $n): int; abs(value: -1);",
        "Extern function 'abs' has no parameter $value",
    );
}

/// Verifies that a function with a declared return type that returns a mismatched
/// type (without being called) produces a return type mismatch error.
#[test]
fn test_error_function_declared_return_type_rejects_mismatch_without_call() {
    expect_error(
        "<?php function foo(): string { return 1; }",
        "Function 'foo' return type expects Str, got Int",
    );
}

/// Verifies that a function with a declared return type that is violated when
/// invoked via first-class callable syntax produces the expected error.
#[test]
fn test_error_function_declared_return_type_rejects_mismatch_via_first_class_callable() {
    expect_error(
        "<?php function foo(): string { return 1; } $f = foo(...);",
        "Function 'foo' return type expects Str, got Int",
    );
}

/// Verifies that a function with a declared return type that does not return a
/// value on all paths (bare function body) produces an error.
#[test]
fn test_error_function_declared_return_type_requires_return_value() {
    expect_error(
        "<?php function foo(): int { }",
        "Function 'foo' must return a value on every path",
    );
}

/// Verifies that a function with a declared return type that returns a value on
/// some paths but not all (e.g., inside an `if` without an `else`) produces an error.
#[test]
fn test_error_function_declared_return_type_rejects_partial_fallthrough() {
    expect_error(
        "<?php function foo(bool $ok): int { if ($ok) { return 1; } }",
        "Function 'foo' must return a value on every path",
    );
}

/// Verifies that a function with a declared return type that can exit via a switch
/// `break` without returning a value produces an error.
#[test]
fn test_error_function_declared_return_type_rejects_switch_break_path() {
    expect_error(
        "<?php function foo(int $x): int { switch ($x) { case 1: if ($x > 0) { break; } return 1; default: return 2; } }",
        "Function 'foo' must return a value on every path",
    );
}

/// Verifies that a bare `return;` inside a function with a non-void return type
/// produces an error.
#[test]
fn test_error_function_declared_return_type_rejects_bare_return() {
    expect_error(
        "<?php function foo(): ?int { return; }",
        "Function 'foo' return type must return a value of type",
    );
}

/// Verifies that a method with a declared return type that does not return a
/// value on all paths produces an error.
#[test]
fn test_error_method_declared_return_type_requires_return_value() {
    expect_error(
        "<?php class Box { public function value(): int { } }",
        "Method 'Box::value' must return a value on every path",
    );
}

/// Verifies that a typed closure parameter rejects a mismatched argument type
/// at the call site.
#[test]
fn test_error_typed_closure_param_rejects_wrong_argument() {
    expect_error(
        "<?php $f = function (int $x) { echo $x; }; $f(\"hello\");",
        "callable $f parameter $x expects Int, got Str",
    );
}

/// Verifies that a first-class callable capturing a typed function rejects a
/// mismatched argument type at the call site.
#[test]
fn test_error_typed_first_class_callable_rejects_wrong_argument() {
    expect_error(
        "<?php function foo(int $x) { echo $x; } $f = foo(...); $f(\"hello\");",
        "callable $f parameter $x expects Int, got Str",
    );
}

/// Verifies that `void` cannot be used as a parameter type in a user-defined function.
#[test]
fn test_error_void_parameter_type_is_rejected() {
    expect_error(
        "<?php function foo(void $x) { }",
        "Function 'foo' parameter $x cannot use type void",
    );
}

/// Verifies that a variadic parameter with an explicit type annotation (e.g., `int ...$xs`)
/// is accepted and type-checks without error.
#[test]
fn test_typed_variadic_parameter_is_accepted() {
    assert!(check_source("<?php function foo(int ...$xs): int { return array_sum($xs); } foo(1, 2, 3);").is_ok());
}

/// Verifies that the declared element type of a typed variadic is enforced against each argument:
/// passing a string to `int ...$xs` is rejected. Regression test — the hint was previously dropped.
#[test]
fn test_error_typed_variadic_rejects_wrong_argument_type() {
    expect_error(
        "<?php function sum(int ...$nums): int { return array_sum($nums); } sum(\"a\", \"b\");",
        "variadic parameter $nums expects Int, got Str",
    );
}

/// Verifies that the declared element type of a typed variadic method parameter is enforced.
#[test]
fn test_error_typed_variadic_method_rejects_wrong_argument_type() {
    expect_error(
        "<?php class C { public function add(int ...$n): int { return count($n); } } $c = new C(); $c->add(\"x\");",
        "variadic parameter $n expects Int, got Str",
    );
}

/// Verifies that an *untyped* variadic still accepts heterogeneous arguments (no enforcement).
#[test]
fn test_untyped_variadic_accepts_mixed_arguments() {
    assert!(check_source(
        "<?php function f(...$xs): int { return count($xs); } f(1, \"a\", 2.5);"
    )
    .is_ok());
}

/// Verifies that a dynamic method call with named arguments is rejected with a clear diagnostic
/// (the target method name is a runtime value, so parameter names are unknown at compile time).
/// The internal `call_user_func` desugaring must not leak into the message.
#[test]
fn test_error_dynamic_method_call_rejects_named_arguments() {
    expect_error(
        "<?php class C { public function op(int $a, int $b): int { return $a - $b; } } $c = new C(); $m = \"op\"; $c->$m(b: 3, a: 10);",
        "Named arguments are not supported in dynamic calls",
    );
}

/// Verifies that a nullable by-reference parameter (e.g., `?int &$x`) requires
/// boxed storage (mixed/union/nullable) when passed by reference.
#[test]
fn test_error_nullable_by_ref_parameter_requires_boxed_storage() {
    expect_error(
        "<?php function bump(?int &$x) { $x = null; } $value = 1; bump($value);",
        "requires a variable with mixed/union/nullable storage when passed by reference",
    );
}

// -- Include/require path expression errors --

/// Verifies that a static closure cannot capture `$this` from the enclosing scope.
#[test]
fn test_error_static_closure_uses_this() {
    expect_error(
        "<?php class C { public int $count = 5; public function bad() { $f = static function() { return $this->count; }; return $f; } }",
        "Cannot use $this inside a static closure",
    );
}

/// Verifies that a static arrow function (`static fn()`) cannot capture `$this`
/// from the enclosing scope.
#[test]
fn test_error_static_arrow_closure_uses_this() {
    expect_error(
        "<?php class C { public int $count = 5; public function bad() { $f = static fn() => $this->count; return $f; } }",
        "Cannot use $this inside a static closure",
    );
}
