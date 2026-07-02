//! Purpose:
//! Integration tests for PHP warnings on out-of-bounds string offsets.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Asserts both stdout and stderr for positive, negative, suppressed, and in-bounds offsets.

use crate::support::*;

/// Verifies that a positive out-of-bounds string offset warns with the offset value.
#[test]
fn test_string_offset_oob_positive_warns() {
    let out = compile_and_run_capture(r#"<?php var_dump('abc'[99]);"#);
    assert_eq!(out.stdout, "string(0) \"\"\n");
    assert!(
        out.stderr.contains("Uninitialized string offset 99"),
        "stderr was: {}",
        out.stderr
    );
}

/// Verifies that a negative out-of-bounds string offset warns with the original offset.
#[test]
fn test_string_offset_oob_negative_warns() {
    let out = compile_and_run_capture(r#"<?php var_dump('abc'[-4]);"#);
    assert_eq!(out.stdout, "string(0) \"\"\n");
    assert!(
        out.stderr.contains("Uninitialized string offset -4"),
        "stderr was: {}",
        out.stderr
    );
}

/// Verifies that the `@` suppression operator silences the out-of-bounds string offset warning.
#[test]
fn test_string_offset_oob_suppressed_by_at() {
    let out = compile_and_run_capture(r#"<?php @var_dump('abc'[99]);"#);
    assert_eq!(out.stdout, "string(0) \"\"\n");
    assert_eq!(out.stderr, "", "expected no stderr, got: {}", out.stderr);
}

/// Verifies that an in-bounds string offset produces no warning.
#[test]
fn test_string_offset_in_bounds_no_warning() {
    let out = compile_and_run_capture(r#"<?php var_dump('abc'[1]);"#);
    assert_eq!(out.stdout, "string(1) \"b\"\n");
    assert_eq!(out.stderr, "");
}