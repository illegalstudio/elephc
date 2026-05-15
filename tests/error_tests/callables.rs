//! Purpose:
//! Integration or regression tests for diagnostic coverage of callables, including call user func wrong args, function exists wrong args, and call non callable variable.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_call_user_func_wrong_args() {
    expect_error(
        r#"<?php call_user_func();"#,
        "call_user_func() takes at least 1 argument",
    );
}

#[test]
fn test_error_function_exists_wrong_args() {
    expect_error(
        r#"<?php function_exists();"#,
        "function_exists() takes exactly 1 argument",
    );
}

#[test]
fn test_error_class_exists_requires_literal_name() {
    expect_error(
        r#"<?php $name = "DateTime"; class_exists($name);"#,
        "class_exists() first argument must be a string literal in AOT mode",
    );
}

#[test]
fn test_error_class_exists_requires_literal_autoload_flag() {
    expect_error(
        r#"<?php $autoload = false; class_exists("DateTime", $autoload);"#,
        "class_exists() autoload argument must be a literal bool or int in AOT mode",
    );
}

#[test]
fn test_error_class_alias_rejects_runtime_call_shape() {
    expect_error(
        r#"<?php class Original {} $alias = "Alias"; class_alias("Original", $alias);"#,
        "class_alias() is only supported as a top-level statement with literal class names",
    );
}

// --- Closure / arrow function errors ---

#[test]
fn test_error_call_non_callable_variable() {
    expect_error(r#"<?php $x = 5; $x(1);"#, "not a callable");
}

#[test]
fn test_error_call_user_func_ref_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); call_user_func($f, 1);",
        "parameter $n must be passed a variable",
    );
}

#[test]
fn test_error_call_user_func_string_literal_ref_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } call_user_func(\"bump\", 1);",
        "parameter $n must be passed a variable",
    );
}

#[test]
fn test_error_closure_return_type_rejects_mismatch() {
    expect_error(
        "<?php $f = function(): string { return 1; };",
        "Closure return type expects Str, got Int",
    );
}

#[test]
fn test_error_arrow_return_type_rejects_mismatch() {
    expect_error(
        "<?php $f = fn(): int => \"nope\";",
        "Closure return type expects Int, got Str",
    );
}

#[test]
fn test_error_closure_return_type_requires_return_value() {
    expect_error(
        "<?php $f = function(): int { };",
        "Closure must return a value on every path",
    );
}

#[test]
fn test_error_closure_return_type_rejects_partial_fallthrough() {
    expect_error(
        "<?php $f = function(bool $ok): int { if ($ok) { return 1; } };",
        "Closure must return a value on every path",
    );
}

#[test]
fn test_error_closure_return_type_rejects_bare_return() {
    expect_error(
        "<?php $f = function(): mixed { return; };",
        "Closure return type must return a value of type",
    );
}

#[test]
fn test_error_closure_void_return_type_rejects_value() {
    expect_error(
        "<?php $f = function(): void { return 1; };",
        "Closure return type must not return a value",
    );
}

#[test]
fn test_error_fiber_callback_rejects_too_many_start_args() {
    expect_error(
        "<?php $fiber = new Fiber(function($a, $b, $c, $d, $e, $f, $g, $h): void {});",
        "Fiber callbacks support at most 7 start arguments, got 8",
    );
}

#[test]
fn test_error_fiber_callback_rejects_by_ref_start_arg() {
    expect_error(
        "<?php $fiber = new Fiber(function(&$value): void {});",
        "Fiber callbacks cannot receive start arguments by reference",
    );
}

#[test]
fn test_error_fiber_callback_rejects_variadic_arg() {
    expect_error(
        "<?php $fiber = new Fiber(function(...$args): void {});",
        "Fiber callbacks cannot be variadic",
    );
}

#[test]
fn test_error_fiber_variable_callback_rejects_variadic_arg() {
    expect_error(
        r#"<?php
$fn = function(...$args): void {};
$fiber = new Fiber($fn);
"#,
        "Fiber callbacks cannot be variadic",
    );
}

#[test]
fn test_error_fiber_direct_callback_rejects_capture_slot_overflow() {
    expect_error(
        r#"<?php
$a = "a"; $b = "b"; $c = "c"; $d = "d";
$fiber = new Fiber(function() use ($a, $b, $c, $d): void {});
"#,
        "Fiber capture $d exceeds the 7 integer-slot Fiber capture limit",
    );
}

#[test]
fn test_error_fiber_variable_callback_rejects_capture_slot_overflow() {
    expect_error(
        r#"<?php
$a = "a"; $b = "b"; $c = "c"; $d = "d";
$fn = function() use ($a, $b, $c, $d): void {};
$fiber = new Fiber($fn);
"#,
        "Fiber capture $d exceeds the 7 integer-slot Fiber capture limit",
    );
}

// --- PHP 8.5 pipe operator ---

#[test]
fn test_error_pipe_rhs_int_not_callable() {
    expect_error(
        "<?php $r = 5 |> 42;",
        "must be a callable",
    );
}

#[test]
fn test_error_pipe_rhs_string_literal_not_callable() {
    // elephc treats a bare string literal as Str, not Callable, so this rejects at compile time.
    expect_error(
        "<?php $r = 5 |> \"strlen\";",
        "must be a callable",
    );
}

#[test]
fn test_error_pipe_rejects_by_ref_parameter() {
    expect_error(
        "<?php function bump(int &$n): int { return ++$n; } $r = 1 |> bump(...);",
        "by-reference parameters",
    );
}

#[test]
fn test_error_pipe_target_requires_more_than_one_required_arg() {
    expect_error(
        "<?php function pair(int $a, int $b): int { return $a + $b; } $r = 1 |> pair(...);",
        "expects 2 arguments, got 1",
    );
}

#[test]
fn test_error_pipe_closure_literal_requires_two_args() {
    expect_error(
        "<?php $r = 1 |> (function(int $a, int $b): int { return $a + $b; });",
        "pipe target expects 2 arguments, got 1",
    );
}

#[test]
fn test_error_pipe_closure_literal_rejects_by_ref_parameter() {
    expect_error(
        "<?php $r = 1 |> (function(&$n): int { return $n; });",
        "Pipe operator does not support by-reference parameters",
    );
}

#[test]
fn test_error_pipe_closure_literal_typed_parameter_mismatch() {
    expect_error(
        r#"<?php $r = "nope" |> (function(int $n): int { $copy = $n; return $copy; });"#,
        "pipe target parameter $n expects Int, got Str",
    );
}
