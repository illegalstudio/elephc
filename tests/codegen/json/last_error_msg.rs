//! Purpose:
//! Provides json_last_error_msg tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Messages must map from the current JSON error code to PHP-compatible strings.

use super::*;

/// Verifies json_last_error_msg returns "No error" on a fresh runtime with no prior JSON operations.
#[test]
fn test_json_last_error_msg_initial() {
    let out = compile_and_run("<?php echo json_last_error_msg();");
    assert_eq!(out, "No error");
}

/// Verifies json_last_error_msg stays "No error" after a successful json_encode call.
#[test]
fn test_json_last_error_msg_after_successful_call() {
    let out = compile_and_run(
        "<?php json_encode([1, 2, 3]); echo json_last_error_msg();",
    );
    assert_eq!(out, "No error");
}

/// Verifies json_last_error_msg returns a string type (not int, bool, or null).
#[test]
fn test_json_last_error_msg_returns_string_type() {
    let out = compile_and_run(
        "<?php $msg = json_last_error_msg(); echo gettype($msg);",
    );
    assert_eq!(out, "string");
}

/// Verifies json_last_error_msg value can be concatenated with strings.
#[test]
fn test_json_last_error_msg_concat() {
    let out = compile_and_run(
        r#"<?php echo "msg=" . json_last_error_msg() . ";";"#,
    );
    assert_eq!(out, "msg=No error;");
}

/// Verifies json_last_error_msg keeps the numeric location after the source string is replaced.
#[test]
fn test_json_last_error_msg_survives_replaced_source() {
    let out = compile_and_run(
        r#"<?php
            function bad() { return "[{"; }
            json_decode(bad());
            $gone = str_repeat("Z", 32);
            echo json_last_error_msg(), ":", strlen($gone);
        "#,
    );
    assert_eq!(out, "Syntax error near location 1:3:32");
}

/// Verifies a later successful decode clears a previous location-bearing message.
#[test]
fn test_json_last_error_msg_clears_location_after_success() {
    let out = compile_and_run(
        r#"<?php
            json_decode("[{");
            json_decode("[]");
            echo json_last_error_msg();
        "#,
    );
    assert_eq!(out, "No error");
}

/// Verifies public json_validate failures replace a previous decode location.
#[test]
fn test_json_last_error_msg_validate_has_own_location_suffix() {
    let out = compile_and_run(
        r#"<?php
            json_decode("[{");
            json_validate("{");
            echo json_last_error_msg();
        "#,
    );
    assert_eq!(out, "Syntax error near location 1:2");
}
