//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP, callables variadics, including first class callable variadic function call, closure variadic call, and first class callable variadic with regular param.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

// Tests variadic function called via first-class callable syntax `func(...)]`.
// Verifies that positional arguments are collected into the variadic parameter
// and count() returns the correct number.
#[test]
fn test_first_class_callable_variadic_function_call() {
    let out = compile_and_run(
        r#"<?php
function count_args(...$xs) {
    echo count($xs);
}

$f = count_args(...);
$f(1, 2, 3);
"#,
    );
    assert_eq!(out, "3");
}

// Tests variadic closure called directly as a callable expression.
// Verifies that positional arguments are collected into the variadic closure
// parameter and count() returns the correct number.
#[test]
fn test_closure_variadic_call() {
    let out = compile_and_run(
        r#"<?php
$f = function (...$xs) {
    echo count($xs);
};

$f(1, 2, 3);
"#,
    );
    assert_eq!(out, "3");
}

// Tests variadic function with a regular parameter before the variadic,
// called via first-class callable syntax.
// Verifies the regular parameter receives the first positional argument,
// remaining arguments fill the variadic, and both are handled correctly.
#[test]
fn test_first_class_callable_variadic_with_regular_param() {
    let out = compile_and_run(
        r#"<?php
function head_and_count($a, ...$rest) {
    echo $a;
    echo ":";
    echo count($rest);
}

$f = head_and_count(...);
$f(7, 8, 9);
"#,
    );
    assert_eq!(out, "7:2");
}

// Tests first-class callable syntax on builtin count() with a sequentially-keyed array.
// Verifies builtin callables work with variadic-compatible signatures.
#[test]
fn test_first_class_callable_builtin_count_accepts_string_arrays() {
    let out = compile_and_run(
        r#"<?php
$f = count(...);
$xs = ["a", "b"];
echo $f($xs);
"#,
    );
    assert_eq!(out, "2");
}

// Tests first-class callable syntax on builtin count() with an associative array.
// Verifies builtin callables work with associative array inputs.
#[test]
fn test_first_class_callable_builtin_count_accepts_assoc_arrays() {
    let out = compile_and_run(
        r#"<?php
$f = count(...);
$xs = ["a" => 1, "b" => 2];
echo $f($xs);
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies static-method callable arrays route associative variadic tails through the descriptor invoker.
#[test]
fn test_static_method_callable_array_call_user_func_array_assoc_variadic_tail() {
    let out = compile_and_run(
        r#"<?php
class Formatter {
    public static function wrap($value = 7, ...$rest) {
        echo $value . ":";
        foreach ($rest as $key => $item) {
            echo $key . "=" . $item . ";";
        }
    }
}

$callback = [Formatter::class, "wrap"];
$args = ["value" => 3, "extra" => 9, "more" => 10];
call_user_func_array($callback, $args);
"#,
    );
    assert_eq!(out, "3:extra=9;more=10;");
}
