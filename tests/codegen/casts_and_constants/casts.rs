//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of casts, constants, and introspection casts, including cast integer from float, cast integer from string, and cast integer from boolean.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_cast_int_from_float() {
    let out = compile_and_run("<?php echo (int)3.7;");
    assert_eq!(out, "3");
}

#[test]
fn test_cast_int_from_string() {
    let out = compile_and_run("<?php echo (int)\"42\";");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_int_from_bool() {
    let out = compile_and_run("<?php echo (int)true;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_float_from_int() {
    let out = compile_and_run("<?php echo (float)42;");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_float_from_string() {
    let out = compile_and_run("<?php echo (float)'3.14';");
    assert_eq!(out, "3.14");
}

#[test]
fn test_cast_float_from_string_integer() {
    let out = compile_and_run("<?php echo (float)'42';");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_float_from_string_non_numeric() {
    let out = compile_and_run("<?php echo (float)'abc';");
    assert_eq!(out, "0");
}

#[test]
fn test_cast_string_from_int() {
    let out = compile_and_run("<?php echo (string)42;");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_string_from_float() {
    let out = compile_and_run("<?php echo (string)3.14;");
    assert_eq!(out, "3.14");
}

#[test]
fn test_cast_string_from_bool_true() {
    let out = compile_and_run("<?php echo (string)true;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_string_from_bool_false() {
    let out = compile_and_run("<?php echo (string)false;");
    assert_eq!(out, "");
}

#[test]
fn test_cast_bool_from_int_zero() {
    let out = compile_and_run("<?php echo (bool)0;");
    assert_eq!(out, "");
}

#[test]
fn test_cast_bool_from_int_nonzero() {
    let out = compile_and_run("<?php echo (bool)42;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_bool_from_string_empty() {
    let out = compile_and_run("<?php echo (bool)\"\";");
    assert_eq!(out, "");
}

#[test]
fn test_cast_bool_from_string_nonempty() {
    let out = compile_and_run("<?php echo (bool)\"hello\";");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_mixed_unboxes_payload() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "int" => 42,
    "float" => 3.75,
    "true" => true,
    "false" => false,
    "null" => null,
    "text" => "27",
];
echo (int)$map["float"];
echo "|";
echo (int)$map["text"];
echo "|";
echo (bool)$map["int"] ? "1" : "0";
echo (bool)$map["false"] ? "1" : "0";
echo "|";
echo (string)$map["true"];
echo "|";
echo (string)$map["null"];
echo "|";
echo (string)$map["int"];
"#,
    );
    assert_eq!(out, "3|27|10|1||42");
}

#[test]
fn test_cast_integer_alias() {
    let out = compile_and_run("<?php echo (integer)3.7;");
    assert_eq!(out, "3");
}

#[test]
fn test_cast_double_alias() {
    let out = compile_and_run("<?php echo (double)42;");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_boolean_alias() {
    let out = compile_and_run("<?php echo (boolean)1;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_keywords_are_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
echo (INTEGER)3.7;
echo ":";
echo (DOUBLE)"2.5";
echo ":";
echo (STRING)42;
echo ":";
echo (BOOLEAN)0 ? "true" : "false";
"#,
    );
    assert_eq!(out, "3:2.5:42:false");
}

// --- gettype ---
