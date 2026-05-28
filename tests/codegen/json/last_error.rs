//! Purpose:
//! Provides json_last_error tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Runtime error state must start clear, update after failures, and reset on success.

use super::*;

/// Verifies json_last_error returns 0 (JSON_ERROR_NONE) on a fresh runtime with no prior
/// JSON operations.
#[test]
fn test_json_last_error_initial_state_is_none() {
    let out = compile_and_run("<?php echo json_last_error();");
    assert_eq!(out, "0");
}

/// Verifies json_last_error stays 0 (JSON_ERROR_NONE) after a successful json_encode call.
#[test]
fn test_json_last_error_after_successful_encode() {
    let out = compile_and_run(
        "<?php json_encode(42); echo json_last_error();",
    );
    assert_eq!(out, "0");
}

/// Verifies json_last_error returns an integer type (not float, string, or null).
#[test]
fn test_json_last_error_returns_int_type() {
    let out = compile_and_run(
        "<?php $code = json_last_error(); echo gettype($code);",
    );
    assert_eq!(out, "integer");
}

/// Verifies json_last_error compares equal to JSON_ERROR_NONE via strict comparison.
/// This also exercises the constant resolution path.
#[test]
fn test_json_last_error_compares_with_constant() {
    let out = compile_and_run(
        "<?php echo (json_last_error() === JSON_ERROR_NONE ? 'ok' : 'no');",
    );
    assert_eq!(out, "ok");
}
