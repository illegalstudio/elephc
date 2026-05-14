//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of casts, constants, and introspection predicates, including boolval true, boolval false, and is boolean true.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_boolval_true() {
    let out = compile_and_run("<?php echo boolval(42);");
    assert_eq!(out, "1");
}

#[test]
fn test_boolval_false() {
    let out = compile_and_run("<?php echo boolval(0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_bool_true() {
    let out = compile_and_run("<?php echo is_bool(true);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_bool_false_for_int() {
    let out = compile_and_run("<?php echo is_bool(1);");
    assert_eq!(out, "");
}

#[test]
fn test_is_string_true() {
    let out = compile_and_run("<?php echo is_string(\"hello\");");
    assert_eq!(out, "1");
}

#[test]
fn test_is_string_false() {
    let out = compile_and_run("<?php echo is_string(42);");
    assert_eq!(out, "");
}

#[test]
fn test_is_numeric_int() {
    let out = compile_and_run("<?php echo is_numeric(42);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_numeric_float() {
    let out = compile_and_run("<?php echo is_numeric(3.14);");
    assert_eq!(out, "1");
}

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
