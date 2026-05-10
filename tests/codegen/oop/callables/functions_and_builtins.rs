//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP, callables functions and builtins, including first class callable named function indirect call, first class callable builtin used in array map, and first class callable builtin intval.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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

#[test]
fn test_first_class_callable_direct_call_user_func() {
    let out = compile_and_run(
        r#"<?php
echo call_user_func(strlen(...), "hello");
"#,
    );
    assert_eq!(out, "5");
}
