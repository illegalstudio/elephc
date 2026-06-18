//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of casts, constants, and introspection predicates, including boolval true, boolval false, and is boolean true.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Compiles `boolval(42)` and verifies it outputs "1" (non-zero truthy value).
#[test]
fn test_boolval_true() {
    let out = compile_and_run("<?php echo boolval(42);");
    assert_eq!(out, "1");
}

/// Compiles `boolval(0)` and verifies it outputs "" (zero is falsy).
#[test]
fn test_boolval_false() {
    let out = compile_and_run("<?php echo boolval(0);");
    assert_eq!(out, "");
}

/// Compiles `is_bool(true)` and verifies it outputs "1" (true is boolean).
#[test]
fn test_is_bool_true() {
    let out = compile_and_run("<?php echo is_bool(true);");
    assert_eq!(out, "1");
}

/// Compiles `is_bool(1)` and verifies it outputs "" (int 1 is not a boolean).
#[test]
fn test_is_bool_false_for_int() {
    let out = compile_and_run("<?php echo is_bool(1);");
    assert_eq!(out, "");
}

/// Compiles `is_string("hello")` and verifies it outputs "1" (string literal).
#[test]
fn test_is_string_true() {
    let out = compile_and_run("<?php echo is_string(\"hello\");");
    assert_eq!(out, "1");
}

/// Compiles `is_string(42)` and verifies it outputs "" (int is not a string).
#[test]
fn test_is_string_false() {
    let out = compile_and_run("<?php echo is_string(42);");
    assert_eq!(out, "");
}

/// Compiles `is_numeric(42)` and verifies it outputs "1" (integer is numeric).
#[test]
fn test_is_numeric_int() {
    let out = compile_and_run("<?php echo is_numeric(42);");
    assert_eq!(out, "1");
}

/// Compiles `is_numeric(3.14)` and verifies it outputs "1" (float is numeric).
#[test]
fn test_is_numeric_float() {
    let out = compile_and_run("<?php echo is_numeric(3.14);");
    assert_eq!(out, "1");
}

/// Compiles `is_numeric("hello")` and verifies it outputs "" (non-numeric string).
#[test]
fn test_is_numeric_string() {
    let out = compile_and_run("<?php echo is_numeric(\"hello\");");
    assert_eq!(out, "");
}

/// Verifies PHP scalar predicate aliases and container/object predicates compile as builtin calls.
#[test]
fn test_type_predicate_aliases_array_and_object() {
    let out = compile_and_run(
        r#"<?php
class Box {}
$object = new Box();
echo is_integer(1) ? "i" : "_";
echo is_long(1) ? "l" : "_";
echo is_double(1.5) ? "d" : "_";
echo is_real(1.5) ? "r" : "_";
echo is_array([1]) ? "a" : "_";
echo is_array(["x" => 1]) ? "h" : "_";
echo is_object($object) ? "o" : "_";
echo is_object([1]) ? "bad" : "_";
"#,
    );
    assert_eq!(out, "ildraho_");
}

/// Verifies `is_array()` inspects boxed Mixed JSON payload tags for array values.
#[test]
fn test_is_array_recognizes_arrays_inside_mixed_array() {
    let out = compile_and_run(
        r#"<?php
$values = [json_decode("[1]"), json_decode("{\"a\":2}", true), 3];
foreach ($values as $value) {
    echo is_array($value) ? "a" : "_";
}
"#,
    );
    assert_eq!(out, "aa_");
}

/// Verifies `strval()` works directly, as a first-class callable, and through string callable dispatch.
#[test]
fn test_strval_direct_first_class_and_callable_dispatch() {
    let out = compile_and_run(
        r#"<?php
echo strval(12);
echo ":";
$strval = strval(...);
echo $strval(true);
echo ":";
echo call_user_func("strval", 7);
"#,
    );
    assert_eq!(out, "12:1:7");
}

/// Verifies `function_exists()` recognizes PHP predicate aliases and `strval()` case-insensitively.
#[test]
fn test_function_exists_recognizes_scalar_alias_builtins() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("is_integer") ? "1" : "0";
echo function_exists("IS_LONG") ? "1" : "0";
echo function_exists("is_double") ? "1" : "0";
echo function_exists("IS_REAL") ? "1" : "0";
echo function_exists("is_object") ? "1" : "0";
echo function_exists("strval") ? "1" : "0";
"#,
    );
    assert_eq!(out, "111111");
}

// --- Mixed-cell-aware predicates ---
//
// `is_string()` / `is_int()` / `is_bool()` peek at the runtime tag of a
// boxed Mixed value via `__rt_mixed_unbox`. Driven by the
// `class_attribute_args()` use case where attribute literals are stored
// as boxed mixed cells, but applies to any Mixed/Union runtime value.

/// Verifies `is_string()` correctly identifies a string inside a boxed Mixed cell
/// from a class attribute with heterogeneous arguments: "hello", 42, true, null.
/// Expects "s___" (first arg is string, rest are not).
#[test]
fn test_is_string_recognizes_string_inside_mixed_array() {
    let out = compile_and_run(
        r#"<?php
#[Tagged("hello", 42, true, null)]
class C {}
$args = class_attribute_args('C', 'Tagged');
foreach ($args as $arg) {
    echo is_string($arg) ? "s" : "_";
}
"#,
    );
    assert_eq!(out, "s___");
}

/// Verifies `is_int()` correctly identifies an int inside a boxed Mixed cell
/// from a class attribute with heterogeneous arguments: "hello", 42, true, null.
/// Expects "_i__" (second arg is int, rest are not).
#[test]
fn test_is_int_recognizes_int_inside_mixed_array() {
    let out = compile_and_run(
        r#"<?php
#[Tagged("hello", 42, true, null)]
class C {}
$args = class_attribute_args('C', 'Tagged');
foreach ($args as $arg) {
    echo is_int($arg) ? "i" : "_";
}
"#,
    );
    assert_eq!(out, "_i__");
}

/// Verifies `is_bool()` correctly identifies a bool inside a boxed Mixed cell
/// from a class attribute with heterogeneous arguments: "hello", 42, true, null.
/// Expects "__b_" (third arg is bool, rest are not).
#[test]
fn test_is_bool_recognizes_bool_inside_mixed_array() {
    let out = compile_and_run(
        r#"<?php
#[Tagged("hello", 42, true, null)]
class C {}
$args = class_attribute_args('C', 'Tagged');
foreach ($args as $arg) {
    echo is_bool($arg) ? "b" : "_";
}
"#,
    );
    assert_eq!(out, "__b_");
}

// --- Exponentiation operator ** ---
