//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc, including bitwise compound assignment requires ints, duplicate use alias is rejected, and has line number.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_bitwise_compound_assignment_requires_ints() {
    expect_error(
        "<?php $x = \"flags\"; $x &= 1;",
        "Bitwise operators require integer operands",
    );
}

#[test]
fn test_error_duplicate_use_alias_is_rejected() {
    expect_error(
        "<?php namespace App; use Lib\\One as Tool; use Lib\\Two as Tool; echo 1;",
        "Duplicate import alias: Tool",
    );
}

#[test]
fn test_error_has_line_number() {
    let result = tokenize("<?php\n\n\"unterminated");
    let err = result.unwrap_err();
    assert_eq!(err.span.line, 3, "Error should be on line 3");
}

#[test]
fn test_error_has_column() {
    let result = tokenize("<?php `");
    let err = result.unwrap_err();
    assert!(err.span.col > 0, "Error should have a column number");
}

#[test]
fn test_error_gettype_wrong_args() {
    expect_error("<?php gettype();", "gettype() takes exactly 1 argument");
}

#[test]
fn test_error_empty_wrong_args() {
    expect_error("<?php empty();", "empty() takes exactly 1 argument");
}

#[test]
fn test_error_unset_wrong_args() {
    expect_error("<?php unset();", "unset() takes exactly 1 argument");
}

#[test]
fn test_error_settype_wrong_args() {
    expect_error("<?php settype(42);", "settype() takes exactly 2 arguments");
}

#[test]
fn test_error_bitwise_and_string() {
    expect_error(
        r#"<?php echo "hello" & 1;"#,
        "Bitwise operators require integer operands",
    );
}

#[test]
fn test_error_bitwise_not_string() {
    expect_error(
        r#"<?php echo ~"hello";"#,
        "Bitwise NOT requires integer operand",
    );
}

#[test]
fn test_error_spaceship_string() {
    expect_error(
        r#"<?php echo "a" <=> "b";"#,
        "Spaceship operator requires numeric operands",
    );
}

#[test]
fn test_error_static_this() {
    expect_error(
        "<?php class Demo { public static function bad() { return $this; } } Demo::bad();",
        "Cannot use $this inside a static method",
    );
}

#[test]
fn test_error_override_cannot_change_parameter_count() {
    expect_error(
        "<?php class Base { public function ping($x) { return $x; } } class Child extends Base { public function ping() { return 1; } }",
        "Cannot change parameter count when overriding method: Child::ping",
    );
}

#[test]
fn test_error_hex_no_digits() {
    expect_error("<?php echo 0x;", "Expected hex digits after '0x'");
}

// --- Mixed return type errors ---

// Note: mixed return types are now widened (Str > Float > Int) instead of
// producing an error. The test_return_type_mixed_branches codegen test
// covers the widening behavior.

// --- Math trig/log error tests ---

#[test]
fn test_error_is_null_wrong_args() {
    expect_error("<?php is_null();", "is_null() takes exactly 1 argument");
}

#[test]
fn test_error_nullable_typed_local_rejects_invalid_reassignment() {
    expect_error(
        "<?php ?int $value = null; $value = \"x\";",
        "cannot reassign $value",
    );
}

#[test]
fn test_include_path_with_variable_errors() {
    let err = resolver_error("<?php $path = 'x'; require $path;");
    assert!(
        err.message.contains("compile-time-constant string"),
        "message did not mention compile-time-constant: {}",
        err.message
    );
}

#[test]
fn test_include_path_with_function_call_errors() {
    let err = resolver_error("<?php require getenv('PATH');");
    assert!(
        err.message.contains("compile-time-constant string"),
        "message did not mention compile-time-constant: {}",
        err.message
    );
}

// --- Static closures ---
