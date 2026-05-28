//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP, callables functions and builtins, including first class callable named function indirect call, first class callable builtin used in array map, and first class callable builtin intval.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests first-class callable syntax on a named function used via indirect call.
///
/// PHP `triple(...)(7)` via variable indirect call.
#[test]
fn test_first_class_callable_named_function_indirect_call() {
    let out = compile_and_run(
        r#"<?php
function triple($n) {
    return $n * 3;
}

$fn = triple(...);
echo $fn(7);
"#,
    );
    assert_eq!(out, "21");
}

/// Tests first-class callable builtin `strlen` passed to `array_map`.
#[test]
fn test_first_class_callable_builtin_used_in_array_map() {
    let out = compile_and_run(
        r#"<?php
$len = strlen(...);
echo $len("tool");
"#,
    );
    assert_eq!(out, "4");
}

/// Tests first-class callable builtin `intval` used in arithmetic expression.
#[test]
fn test_first_class_callable_builtin_intval() {
    let out = compile_and_run(
        r#"<?php
$to_int = intval(...);
echo $to_int("123") + 7;
"#,
    );
    assert_eq!(out, "130");
}

/// Tests first-class callable builtin `strtolower` used in direct call.
#[test]
fn test_first_class_callable_builtin_string_transform() {
    let out = compile_and_run(
        r#"<?php
$lower = strtolower(...);
echo $lower("TOOLS");
"#,
    );
    assert_eq!(out, "tools");
}

/// Tests first-class callable builtin `array_sum` with a literal array argument.
#[test]
fn test_first_class_callable_builtin_array_sum() {
    let out = compile_and_run(
        r#"<?php
$sum = array_sum(...);
echo $sum([2, 3, 5]);
"#,
    );
    assert_eq!(out, "10");
}

/// Tests first-class callable builtin `trim` stripping leading/trailing whitespace.
#[test]
fn test_first_class_callable_builtin_trim() {
    let out = compile_and_run(
        r#"<?php
$trim = trim(...);
echo $trim("  ready  ");
"#,
    );
    assert_eq!(out, "ready");
}

/// Tests first-class callable builtin `substr` with start index and length arguments.
#[test]
fn test_first_class_callable_builtin_substr() {
    let out = compile_and_run(
        r#"<?php
$substr = substr(...);
echo $substr("abcdef", 2, 3);
"#,
    );
    assert_eq!(out, "cde");
}

/// Tests first-class callable builtin `str_contains` used in a ternary for boolean output.
#[test]
fn test_first_class_callable_builtin_str_contains() {
    let out = compile_and_run(
        r#"<?php
$contains = str_contains(...);
echo $contains("compiler", "pile") ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// Tests that a first-class callable builtin that mutates a by-ref parameter preserves the array after call.
#[test]
fn test_first_class_callable_builtin_sort_preserves_by_ref_param() {
    let out = compile_and_run(
        r#"<?php
$sort = sort(...);
$values = [3, 1, 2];
$sort($values);
foreach ($values as $value) {
    echo $value;
}
"#,
    );
    assert_eq!(out, "123");
}

/// Tests that a user-defined function with by-ref parameter is correctly mutated via first-class callable.
#[test]
fn test_first_class_callable_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$fn = bump(...);
$value = 7;
$fn($value);
echo $value;
"#,
    );
    assert_eq!(out, "8");
}

/// Tests that an alias of a first-class callable still mutates the caller's by-ref argument.
#[test]
fn test_first_class_callable_alias_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$f = bump(...);
$g = $f;
$value = 7;
$g($value);
echo $value;
"#,
    );
    assert_eq!(out, "8");
}

/// Tests that an alias of a closure with by-ref parameter correctly mutates the caller's argument.
#[test]
fn test_closure_alias_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
$f = function (&$x) {
    $x = $x + 1;
};

$g = $f;
$value = 7;
$g($value);
echo $value;
"#,
    );
    assert_eq!(out, "8");
}

/// Tests a first-class callable named function passed to `array_map` with index-based array access.
#[test]
fn test_first_class_callable_variable_used_in_array_map() {
    let out = compile_and_run(
        r#"<?php
function double($n) {
    return $n * 2;
}

$fn = double(...);
$values = array_map($fn, [1, 2, 3]);
echo $values[0];
echo ":";
echo $values[2];
"#,
    );
    assert_eq!(out, "2:6");
}

/// Tests a first-class callable on an untyped user function accepting a string argument.
#[test]
fn test_first_class_callable_untyped_function_accepts_string_args() {
    let out = compile_and_run(
        r#"<?php
function greet($name) {
    return "Hello " . $name;
}

$f = greet(...);
echo $f("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Tests `call_user_func` with a first-class callable builtin (`strlen`).
#[test]
fn test_first_class_callable_direct_call_user_func() {
    let out = compile_and_run(
        r#"<?php
echo call_user_func(strlen(...), "hello");
"#,
    );
    assert_eq!(out, "5");
}
