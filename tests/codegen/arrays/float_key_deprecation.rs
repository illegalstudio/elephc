//! Purpose:
//! Integration tests for PHP deprecation warnings on float array keys.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Asserts both stdout and stderr for fractional, whole-number, and suppressed float keys.

use crate::support::*;

/// Verifies that a fractional float array key emits the implicit-conversion deprecation.
#[test]
fn test_float_array_key_emits_deprecation() {
    let out = compile_and_run_capture(r#"<?php echo [1 => 'x'][1.9];"#);
    assert_eq!(out.stdout, "x");
    assert!(
        out.stderr.contains("Implicit conversion from float 1.9 to int loses precision"),
        "stderr was: {}",
        out.stderr
    );
}

/// Verifies that a whole-number float array key does not emit the deprecation.
#[test]
fn test_float_array_key_whole_number_no_deprecation() {
    let out = compile_and_run_capture(r#"<?php echo [1 => 'x'][2.0];"#);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, "", "expected no stderr, got: {}", out.stderr);
}

/// Verifies that the `@` suppression operator silences the float-key deprecation.
#[test]
fn test_float_array_key_suppressed_by_at() {
    let out = compile_and_run_capture(r#"<?php $x = @[1 => 'x'][1.9]; echo $x;"#);
    assert_eq!(out.stdout, "x");
    assert_eq!(out.stderr, "");
}