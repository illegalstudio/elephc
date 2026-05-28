//! Purpose:
//! Provides JSON argument evaluation-order tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Builtins must evaluate arguments left-to-right before mutating runtime JSON state.

use super::*;

/// Verifies json_encode evaluates arguments left-to-right: value, then flags, then depth.
/// Each argument function echoes a unique marker so evaluation order is observable in output.
#[test]
fn test_json_encode_evaluates_value_before_flags_and_depth() {
    let out = compile_and_run(
        r#"<?php
function value_arg() { echo "V"; return "x"; }
function flags_arg() { echo "F"; return 0; }
function depth_arg() { echo "D"; return 512; }
echo json_encode(value_arg(), flags_arg(), depth_arg());
"#,
    );
    assert_eq!(out, "VFD\"x\"");
}

/// Verifies json_decode evaluates arguments left-to-right: JSON string, assoc, depth, flags.
/// Each argument function echoes a unique marker so evaluation order is observable in output.
/// Also confirms the return type is correctly determined (array vs object).
#[test]
fn test_json_decode_evaluates_arguments_left_to_right() {
    let out = compile_and_run(
        r#"<?php
function json_arg() { echo "J"; return "{\"a\":1}"; }
function assoc_arg() { echo "A"; return true; }
function depth_arg() { echo "D"; return 512; }
function flags_arg() { echo "F"; return 0; }
echo gettype(json_decode(json_arg(), assoc_arg(), depth_arg(), flags_arg()));
"#,
    );
    assert_eq!(out, "JADFarray");
}

/// Verifies json_validate evaluates arguments left-to-right: JSON string, depth, flags.
/// Each argument function echoes a unique marker so evaluation order is observable in output.
#[test]
fn test_json_validate_evaluates_arguments_left_to_right() {
    let out = compile_and_run(
        r#"<?php
function json_arg() { echo "J"; return "[1]"; }
function depth_arg() { echo "D"; return 512; }
function flags_arg() { echo "F"; return 0; }
echo json_validate(json_arg(), depth_arg(), flags_arg()) ? "ok" : "no";
"#,
    );
    assert_eq!(out, "JDFok");
}

/// Verifies json_decode uses PHP truthiness for string assoc arguments.
/// Non-numeric strings coerce to boolean per PHP semantics: "" and "0" are falsy (→ object),
/// "1" and other non-empty non-"0" strings are truthy (→ array).
#[test]
fn test_json_decode_string_associative_uses_php_truthiness() {
    let out = compile_and_run(
        r#"<?php
echo gettype(json_decode("{}", "")) . "\n";
echo gettype(json_decode("{}", "0")) . "\n";
echo gettype(json_decode("{}", "1"));
"#,
    );
    assert_eq!(out, "object\nobject\narray");
}

/// Verifies JSON_OBJECT_AS_ARRAY flag is applied only when assoc is null (not false).
/// When assoc is null, the flag controls return type (array). When assoc is explicitly false,
/// the flag is ignored and the return type is always object.
#[test]
fn test_json_decode_object_as_array_flag_applies_when_associative_is_null() {
    let out = compile_and_run(
        r#"<?php
echo gettype(json_decode("{}", null, 512, JSON_OBJECT_AS_ARRAY)) . "\n";
echo gettype(json_decode("{}", false, 512, JSON_OBJECT_AS_ARRAY));
"#,
    );
    assert_eq!(out, "array\nobject");
}

/// Verifies json_decode and json_validate accept integer JSON strings without error.
/// Numeric strings passed to these builtins are accepted as valid JSON input (scalar coercion).
#[test]
fn test_json_string_arguments_accept_scalar_coercion() {
    let out = compile_and_run(
        r#"<?php
echo json_validate(123) ? "valid" : "invalid";
echo ":";
echo json_decode(123);
"#,
    );
    assert_eq!(out, "valid:123");
}
