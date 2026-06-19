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

/// Verifies `is_array` on statically-known arrays/hashes and non-arrays, matching PHP's
/// `bool` result type (not int). Indexed and associative literals are both arrays.
#[test]
fn test_is_array_static() {
    let out = compile_and_run(
        r#"<?php
var_dump(is_array([1, 2, 3]));
var_dump(is_array(["a" => 1]));
var_dump(is_array("nope"));
var_dump(is_array(5));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(false)\nbool(false)\n");
}

/// Verifies `is_object` is true only for object values and false for scalars, returning `bool`.
#[test]
fn test_is_object_static() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $v = 1; }
var_dump(is_object(new Box()));
var_dump(is_object("nope"));
var_dump(is_object(42));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(false)\n");
}

/// Verifies `is_scalar` is true for int/float/string/bool and false for null/array/object,
/// matching PHP's classification (resources and null are not scalars).
#[test]
fn test_is_scalar_static() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $v = 1; }
var_dump(is_scalar(5));
var_dump(is_scalar(3.5));
var_dump(is_scalar("hi"));
var_dump(is_scalar(true));
var_dump(is_scalar(null));
var_dump(is_scalar([1]));
var_dump(is_scalar(new Box()));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies `is_array`/`is_object`/`is_scalar` dispatch on the runtime tag of a boxed `Mixed`
/// value (read from a heterogeneous associative array), not the static union member.
#[test]
fn test_is_kind_predicates_on_mixed() {
    let out = compile_and_run(
        r#"<?php
$het = ["arr" => [1, 2], "num" => 7, "str" => "x", "flo" => 2.5];
var_dump(is_array($het["arr"]));
var_dump(is_array($het["num"]));
var_dump(is_scalar($het["num"]));
var_dump(is_scalar($het["str"]));
var_dump(is_scalar($het["flo"]));
var_dump(is_scalar($het["arr"]));
var_dump(is_object($het["num"]));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies the new kind predicates honor PHP case-insensitive and namespace-qualified
/// call forms, and that `function_exists` recognizes them through the catalog.
#[test]
fn test_is_kind_predicates_case_and_namespace() {
    let out = compile_and_run(
        r#"<?php
echo IS_ARRAY([1]) ? "A" : "_";
echo \is_object(new \stdClass()) ? "O" : "_";
echo function_exists("is_scalar") ? "S" : "_";
"#,
    );
    assert_eq!(out, "AOS");
}

// --- Exponentiation operator ** ---
