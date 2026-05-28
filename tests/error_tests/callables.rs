//! Purpose:
//! Integration or regression tests for diagnostic coverage of callables, including call user func wrong args, function exists wrong args, and call non callable variable.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies that error call user func wrong args.
#[test]
fn test_error_call_user_func_wrong_args() {
    // Verifies `call_user_func()` with no arguments produces a diagnostic about
    // requiring at least 1 argument.
    expect_error(
        r#"<?php call_user_func();"#,
        "call_user_func() takes at least 1 argument",
    );
}

/// Verifies that error function exists wrong args.
#[test]
fn test_error_function_exists_wrong_args() {
    // Verifies `function_exists()` with no arguments produces a diagnostic about
    // requiring exactly 1 argument.
    expect_error(
        r#"<?php function_exists();"#,
        "function_exists() takes exactly 1 argument",
    );
}

/// Verifies that error class exists requires literal name.
#[test]
fn test_error_class_exists_requires_literal_name() {
    // Verifies `class_exists()` with a runtime variable as the first argument
    // produces a diagnostic because AOT mode requires a string literal.
    expect_error(
        r#"<?php $name = "DateTime"; class_exists($name);"#,
        "class_exists() first argument must be a string literal in AOT mode",
    );
}

/// Verifies that error class exists requires literal autoload flag.
#[test]
fn test_error_class_exists_requires_literal_autoload_flag() {
    // Verifies `class_exists()` with a runtime variable as the autoload flag
    // produces a diagnostic because AOT mode requires a literal bool or int.
    expect_error(
        r#"<?php $autoload = false; class_exists("DateTime", $autoload);"#,
        "class_exists() autoload argument must be a literal bool or int in AOT mode",
    );
}

/// Verifies that error interface exists wrong args.
#[test]
fn test_error_interface_exists_wrong_args() {
    // Verifies `interface_exists()` with no arguments produces a diagnostic about
    // requiring 1 or 2 arguments.
    expect_error(
        r#"<?php interface_exists();"#,
        "interface_exists() takes 1 or 2 arguments",
    );
}

/// Verifies that error trait exists wrong args.
#[test]
fn test_error_trait_exists_wrong_args() {
    // Verifies `trait_exists()` with no arguments produces a diagnostic about
    // requiring 1 or 2 arguments.
    expect_error(
        r#"<?php trait_exists();"#,
        "trait_exists() takes 1 or 2 arguments",
    );
}

/// Verifies that error enum exists wrong args.
#[test]
fn test_error_enum_exists_wrong_args() {
    // Verifies `enum_exists()` with no arguments produces a diagnostic about
    // requiring 1 or 2 arguments.
    expect_error(
        r#"<?php enum_exists();"#,
        "enum_exists() takes 1 or 2 arguments",
    );
}

/// Verifies that error class implements wrong args.
#[test]
fn test_error_class_implements_wrong_args() {
    expect_error(
        r#"<?php class_implements();"#,
        "class_implements() takes 1 or 2 arguments",
    );
}

/// Verifies that error class implements requires literal or object.
#[test]
fn test_error_class_implements_requires_literal_or_object() {
    expect_error(
        r#"<?php $name = "DateTime"; class_implements($name);"#,
        "class_implements() first argument must be an object or string literal in AOT mode",
    );
}

/// Verifies that error class parents requires literal autoload flag.
#[test]
fn test_error_class_parents_requires_literal_autoload_flag() {
    expect_error(
        r#"<?php $autoload = true; class_parents("DateTime", $autoload);"#,
        "class_parents() autoload argument must be a literal bool or int in AOT mode",
    );
}

/// Verifies that error class uses wrong args.
#[test]
fn test_error_class_uses_wrong_args() {
    expect_error(
        r#"<?php class_uses("DateTime", true, false);"#,
        "class_uses() takes 1 or 2 arguments",
    );
}

/// Verifies that error get class wrong args.
#[test]
fn test_error_get_class_wrong_args() {
    // Verifies `get_class()` with a second argument produces a diagnostic about
    // accepting at most 1 argument.
    expect_error(
        r#"<?php class Box {} $box = new Box(); get_class($box, $box);"#,
        "get_class() takes at most 1 argument",
    );
}

/// Verifies that error get parent class wrong args.
#[test]
fn test_error_get_parent_class_wrong_args() {
    // Verifies `get_parent_class()` with a second argument produces a diagnostic
    // about accepting at most 1 argument.
    expect_error(
        r#"<?php class Box {} $box = new Box(); get_parent_class($box, $box);"#,
        "get_parent_class() takes at most 1 argument",
    );
}

/// Verifies that error is subclass of wrong args.
#[test]
fn test_error_is_subclass_of_wrong_args() {
    // Verifies `is_subclass_of()` with only 1 argument produces a diagnostic
    // about requiring 2 or 3 arguments.
    expect_error(
        r#"<?php is_subclass_of("Child");"#,
        "is_subclass_of() takes 2 or 3 arguments",
    );
}

/// Verifies that error is a wrong args.
#[test]
fn test_error_is_a_wrong_args() {
    // Verifies `is_a()` with only 1 argument produces a diagnostic about
    // requiring 2 or 3 arguments.
    expect_error(
        r#"<?php is_a("Child");"#,
        "is_a() takes 2 or 3 arguments",
    );
}

/// Verifies that error get declared classes wrong args.
#[test]
fn test_error_get_declared_classes_wrong_args() {
    // Verifies `get_declared_classes()` with an extra argument produces a
    // diagnostic about accepting no arguments.
    expect_error(
        r#"<?php get_declared_classes("extra");"#,
        "get_declared_classes() takes no arguments",
    );
}

/// Verifies that error get declared interfaces wrong args.
#[test]
fn test_error_get_declared_interfaces_wrong_args() {
    // Verifies `get_declared_interfaces()` with an extra argument produces a
    // diagnostic about accepting no arguments.
    expect_error(
        r#"<?php get_declared_interfaces("extra");"#,
        "get_declared_interfaces() takes no arguments",
    );
}

/// Verifies that error get declared traits wrong args.
#[test]
fn test_error_get_declared_traits_wrong_args() {
    // Verifies `get_declared_traits()` with an extra argument produces a
    // diagnostic about accepting no arguments.
    expect_error(
        r#"<?php get_declared_traits("extra");"#,
        "get_declared_traits() takes no arguments",
    );
}

/// Verifies that error class alias rejects runtime call shape.
#[test]
fn test_error_class_alias_rejects_runtime_call_shape() {
    // Verifies `class_alias()` with a runtime variable as the second argument
    // produces a diagnostic because only top-level statements with literal
    // class names are supported in AOT mode.
    expect_error(
        r#"<?php class Original {} $alias = "Alias"; class_alias("Original", $alias);"#,
        "class_alias() is only supported as a top-level statement with literal class names",
    );
}

// --- Closure / arrow function errors ---

/// Verifies that error call non callable variable.
#[test]
fn test_error_call_non_callable_variable() {
    // Verifies invoking a non-callable variable (integer) produces a "not a callable"
    // diagnostic at runtime.
    expect_error(r#"<?php $x = 5; $x(1);"#, "not a callable");
}

/// Verifies that error call user func ref param requires variable.
#[test]
fn test_error_call_user_func_ref_param_requires_variable() {
    // Verifies `call_user_func()` with a closure that has a by-reference
    // parameter and a non-variable argument produces a diagnostic requiring
    // a variable to be passed.
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); call_user_func($f, 1);",
        "parameter $n must be passed a variable",
    );
}

/// Verifies that error call user func string literal ref param requires variable.
#[test]
fn test_error_call_user_func_string_literal_ref_param_requires_variable() {
    // Verifies `call_user_func()` with a named function string and a by-reference
    // parameter passed a non-variable argument produces a diagnostic requiring
    // a variable to be passed.
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } call_user_func(\"bump\", 1);",
        "parameter $n must be passed a variable",
    );
}

/// Verifies that error case insensitive function string introspection keeps callback checks.
#[test]
fn test_error_case_insensitive_function_string_introspection_keeps_callback_checks() {
    // Verifies that case-insensitive function string introspection via
    // `function_exists("BUMP")` and `is_callable("BUMP")` still enforces
    // by-reference parameter semantics when `call_user_func("BUMP", ...)` is
    // subsequently invoked.
    expect_error(
        "<?php function Bump(&$n) { $n = $n + 1; } if (function_exists(\"BUMP\") && is_callable(\"BUMP\")) { call_user_func(\"BUMP\", 1); }",
        "parameter $n must be passed a variable",
    );
}

/// Verifies that error closure return type rejects mismatch.
#[test]
fn test_error_closure_return_type_rejects_mismatch() {
    // Verifies a closure with an explicit return type that returns a mismatched
    // type produces a diagnostic showing the expected and actual types.
    expect_error(
        "<?php $f = function(): string { return 1; };",
        "Closure return type expects Str, got Int",
    );
}

/// Verifies that error arrow return type rejects mismatch.
#[test]
fn test_error_arrow_return_type_rejects_mismatch() {
    // Verifies an arrow function with an explicit return type that returns a
    // mismatched type produces a diagnostic showing the expected and actual types.
    expect_error(
        "<?php $f = fn(): int => \"nope\";",
        "Closure return type expects Int, got Str",
    );
}

/// Verifies that error closure return type requires return value.
#[test]
fn test_error_closure_return_type_requires_return_value() {
    // Verifies a closure with an explicit return type and an empty body (no return)
    // produces a diagnostic about every path needing to return a value.
    expect_error(
        "<?php $f = function(): int { };",
        "Closure must return a value on every path",
    );
}

/// Verifies that error closure return type rejects partial fallthrough.
#[test]
fn test_error_closure_return_type_rejects_partial_fallthrough() {
    // Verifies a closure with an explicit return type where only some branches
    // return a value (missing return in else branch) produces a diagnostic
    // about every path needing to return a value.
    expect_error(
        "<?php $f = function(bool $ok): int { if ($ok) { return 1; } };",
        "Closure must return a value on every path",
    );
}

/// Verifies that error closure return type rejects bare return.
#[test]
fn test_error_closure_return_type_rejects_bare_return() {
    // Verifies a closure with `mixed` return type and a bare `return;` (no value)
    // produces a diagnostic about needing to return a value of the specified type.
    expect_error(
        "<?php $f = function(): mixed { return; };",
        "Closure return type must return a value of type",
    );
}

/// Verifies that error closure void return type rejects value.
#[test]
fn test_error_closure_void_return_type_rejects_value() {
    // Verifies a closure with `void` return type that returns a value produces
    // a diagnostic about not returning a value.
    expect_error(
        "<?php $f = function(): void { return 1; };",
        "Closure return type must not return a value",
    );
}

/// Verifies that error fiber callback rejects too many start args.
#[test]
fn test_error_fiber_callback_rejects_too_many_start_args() {
    // Verifies a `Fiber` with a callback accepting 8 start arguments produces
    // a diagnostic because Fibers support at most 7 start arguments.
    expect_error(
        "<?php $fiber = new Fiber(function($a, $b, $c, $d, $e, $f, $g, $h): void {});",
        "Fiber callbacks support at most 7 start arguments, got 8",
    );
}

/// Verifies that error fiber callback rejects by ref start arg.
#[test]
fn test_error_fiber_callback_rejects_by_ref_start_arg() {
    // Verifies a `Fiber` with a callback that receives a start argument
    // by reference produces a diagnostic because by-reference start args
    // are not supported.
    expect_error(
        "<?php $fiber = new Fiber(function(&$value): void {});",
        "Fiber callbacks cannot receive start arguments by reference",
    );
}

// --- PHP 8.5 pipe operator ---

/// Verifies that error pipe rhs integer not callable.
#[test]
fn test_error_pipe_rhs_int_not_callable() {
    // Verifies the pipe operator (`|>`) with a plain integer on the right-hand
    // side produces a "must be a callable" diagnostic.
    expect_error(
        "<?php $r = 5 |> 42;",
        "must be a callable",
    );
}

/// Verifies that error pipe rhs string literal not callable.
#[test]
fn test_error_pipe_rhs_string_literal_not_callable() {
    // Verifies the pipe operator (`|>`) with a bare string literal on the RHS
    // produces a "must be a callable" diagnostic because string literals are
    // treated as `Str`, not `Callable`, at compile time.
    expect_error(
        "<?php $r = 5 |> \"strlen\";",
        "must be a callable",
    );
}

/// Verifies that error pipe rejects by ref parameter.
#[test]
fn test_error_pipe_rejects_by_ref_parameter() {
    // Verifies the pipe operator (`|>`) with a function that has by-reference
    // parameters produces a diagnostic because by-reference parameters are not
    // supported with the pipe operator.
    expect_error(
        "<?php function bump(int &$n): int { return ++$n; } $r = 1 |> bump(...);",
        "by-reference parameters",
    );
}

/// Verifies that error pipe target requires more than one required arg.
#[test]
fn test_error_pipe_target_requires_more_than_one_required_arg() {
    // Verifies the pipe operator (`|>`) with a callable that requires more than
    // one argument and is called without sufficient arguments produces a diagnostic
    // showing the expected vs received argument count.
    expect_error(
        "<?php function pair(int $a, int $b): int { return $a + $b; } $r = 1 |> pair(...);",
        "expects 2 arguments, got 1",
    );
}

/// Verifies that error pipe closure literal requires two args.
#[test]
fn test_error_pipe_closure_literal_requires_two_args() {
    // Verifies the pipe operator (`|>`) with a closure literal that expects two
    // arguments but receives only one (via the pipe's left-hand side) produces
    // a diagnostic showing the expected vs received argument count.
    expect_error(
        "<?php $r = 1 |> (function(int $a, int $b): int { return $a + $b; });",
        "pipe target expects 2 arguments, got 1",
    );
}

/// Verifies that error pipe closure literal rejects by ref parameter.
#[test]
fn test_error_pipe_closure_literal_rejects_by_ref_parameter() {
    // Verifies the pipe operator (`|>`) with a closure literal containing a
    // by-reference parameter produces a diagnostic because by-reference
    // parameters are not supported with the pipe operator.
    expect_error(
        "<?php $r = 1 |> (function(&$n): int { return $n; });",
        "Pipe operator does not support by-reference parameters",
    );
}

/// Verifies that error pipe closure literal typed parameter mismatch.
#[test]
fn test_error_pipe_closure_literal_typed_parameter_mismatch() {
    // Verifies the pipe operator (`|>`) with a closure literal that has a typed
    // parameter where the piped value's type does not match produces a diagnostic
    // showing the expected vs actual parameter type.
    expect_error(
        r#"<?php $r = "nope" |> (function(int $n): int { $copy = $n; return $copy; });"#,
        "pipe target parameter $n expects Int, got Str",
    );
}
