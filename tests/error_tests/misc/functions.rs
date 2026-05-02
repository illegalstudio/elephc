use super::*;

#[test]
fn test_error_variadic_missing_variable() {
    expect_error(
        "<?php function foo(... ) {}",
        "Expected variable after '...'",
    );
}

#[test]
fn test_error_variadic_not_last() {
    expect_error(
        "<?php function foo(...$a, $b) {}",
        "Variadic parameter must be the last parameter",
    );
}

#[test]
fn test_error_first_class_callable_rejects_instance_methods() {
    expect_error(
        "<?php class User { public function greet() { return 1; } } $u = new User(); $f = $u->greet(...);",
        "First-class instance method callables are not supported yet",
    );
}

#[test]
fn test_error_first_class_callable_rejects_static_receiver_static() {
    expect_error(
        "<?php class User { public static function make() { return 1; } public function run() { $f = static::make(...); } }",
        "does not support static:: targets yet",
    );
}

#[test]
fn test_error_first_class_callable_rejects_unsupported_builtin() {
    expect_error(
        "<?php $f = trim(...);",
        "does not support builtin 'trim' yet",
    );
}

#[test]
fn test_error_first_class_callable_ref_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); $f(1);",
        "parameter $n must be passed a variable",
    );
}

#[test]
fn test_error_closure_ref_param_requires_variable() {
    expect_error(
        "<?php $f = function (&$x) { $x = $x + 1; }; $f(1);",
        "parameter $x must be passed a variable",
    );
}

#[test]
fn test_error_function_typed_param_rejects_wrong_argument() {
    expect_error(
        "<?php function foo(int $x) { echo $x; } foo(\"hello\");",
        "Function 'foo' parameter $x expects Int, got Str",
    );
}

#[test]
fn test_error_duplicate_functions_differing_only_by_case() {
    expect_error(
        "<?php function Foo() { return 1; } function foo() { return 2; }",
        "Duplicate function declaration: foo",
    );
}

#[test]
fn test_error_cannot_redeclare_builtin_function_differing_only_by_case() {
    expect_error(
        "<?php function STRLEN(string $value): int { return 0; }",
        "Cannot redeclare built-in function: strlen",
    );
}

#[test]
fn test_error_user_constants_are_case_sensitive() {
    expect_error(
        "<?php const MyConst = 1; echo myconst;",
        "Undefined constant: myconst",
    );
}

#[test]
fn test_error_typed_default_parameter_rejects_mismatched_default() {
    expect_error(
        "<?php function foo(int $x = \"hello\") { echo $x; }",
        "Function 'foo' parameter $x expects Int, got Str",
    );
}

#[test]
fn test_error_named_arguments_reject_unknown_parameter() {
    expect_error(
        "<?php function greet($name) { echo $name; } greet(age: 30);",
        "Function 'greet' has no parameter $age",
    );
}

#[test]
fn test_error_named_arguments_reject_positional_after_named() {
    expect_error(
        "<?php function greet($name, $age) { echo $name; } greet(name: \"Alice\", 30);",
        "Function 'greet' cannot use positional arguments after named arguments",
    );
}

#[test]
fn test_error_named_arguments_reject_duplicate_assignment() {
    expect_error(
        "<?php function greet($name) { echo $name; } greet(\"Alice\", name: \"Bob\");",
        "Function 'greet' parameter $name is already assigned",
    );
}

#[test]
fn test_error_named_arguments_reject_builtin_calls() {
    expect_error(
        "<?php strlen(string: \"hello\");",
        "Builtin 'strlen' does not support named arguments yet",
    );
}

#[test]
fn test_error_named_arguments_reject_spread_mix() {
    expect_error(
        "<?php function greet($name, $age) { echo $name; } $args = [\"Alice\"]; greet(...$args, age: 30);",
        "Function 'greet' does not support mixing named arguments with spread arguments yet",
    );
}

#[test]
fn test_error_function_declared_return_type_rejects_mismatch_without_call() {
    expect_error(
        "<?php function foo(): string { return 1; }",
        "Function 'foo' return type expects Str, got Int",
    );
}

#[test]
fn test_error_function_declared_return_type_rejects_mismatch_via_first_class_callable() {
    expect_error(
        "<?php function foo(): string { return 1; } $f = foo(...);",
        "Function 'foo' return type expects Str, got Int",
    );
}

#[test]
fn test_error_function_declared_return_type_requires_return_value() {
    expect_error(
        "<?php function foo(): int { }",
        "Function 'foo' must return a value on every path",
    );
}

#[test]
fn test_error_function_declared_return_type_rejects_partial_fallthrough() {
    expect_error(
        "<?php function foo(bool $ok): int { if ($ok) { return 1; } }",
        "Function 'foo' must return a value on every path",
    );
}

#[test]
fn test_error_function_declared_return_type_rejects_switch_break_path() {
    expect_error(
        "<?php function foo(int $x): int { switch ($x) { case 1: if ($x > 0) { break; } return 1; default: return 2; } }",
        "Function 'foo' must return a value on every path",
    );
}

#[test]
fn test_error_function_declared_return_type_rejects_bare_return() {
    expect_error(
        "<?php function foo(): ?int { return; }",
        "Function 'foo' return type must return a value of type",
    );
}

#[test]
fn test_error_method_declared_return_type_requires_return_value() {
    expect_error(
        "<?php class Box { public function value(): int { } }",
        "Method 'Box::value' must return a value on every path",
    );
}

#[test]
fn test_error_typed_closure_param_rejects_wrong_argument() {
    expect_error(
        "<?php $f = function (int $x) { echo $x; }; $f(\"hello\");",
        "callable $f parameter $x expects Int, got Str",
    );
}

#[test]
fn test_error_typed_first_class_callable_rejects_wrong_argument() {
    expect_error(
        "<?php function foo(int $x) { echo $x; } $f = foo(...); $f(\"hello\");",
        "callable $f parameter $x expects Int, got Str",
    );
}

#[test]
fn test_error_void_parameter_type_is_rejected() {
    expect_error(
        "<?php function foo(void $x) { }",
        "Function 'foo' parameter $x cannot use type void",
    );
}

#[test]
fn test_error_typed_variadic_parameter_is_not_supported_yet() {
    expect_error(
        "<?php function foo(int ...$xs) { }",
        "Typed variadic parameters are not supported yet",
    );
}

#[test]
fn test_error_nullable_by_ref_parameter_requires_boxed_storage() {
    expect_error(
        "<?php function bump(?int &$x) { $x = null; } $value = 1; bump($value);",
        "requires a variable with mixed/union/nullable storage when passed by reference",
    );
}

// --- Include/require path expression errors ---

#[test]
fn test_error_static_closure_uses_this() {
    expect_error(
        "<?php class C { public int $count = 5; public function bad() { $f = static function() { return $this->count; }; return $f; } }",
        "Cannot use $this inside a static closure",
    );
}

#[test]
fn test_error_static_arrow_closure_uses_this() {
    expect_error(
        "<?php class C { public int $count = 5; public function bad() { $f = static fn() => $this->count; return $f; } }",
        "Cannot use $this inside a static closure",
    );
}
