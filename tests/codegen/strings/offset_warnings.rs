//! Purpose:
//! Regression tests for diagnostics on out-of-bounds string offset reads.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - Reads warn once; suppressed reads and in-bounds reads remain silent.

use crate::support::*;

/// Checks both signs retain the original offset in PHP's warning text.
#[test]
fn test_string_offset_out_of_bounds_warnings() {
    let out = compile_and_run_capture("<?php var_dump('abc'[99]); var_dump('abc'[-4]);");
    assert_eq!(out.stdout, "string(0) \"\"\nstring(0) \"\"\n");
    assert!(out.stderr.contains("Uninitialized string offset 99"), "{}", out.stderr);
    assert!(out.stderr.contains("Uninitialized string offset -4"), "{}", out.stderr);
}

/// Checks normal and suppressed string indexing keep their existing results.
#[test]
fn test_string_offset_warning_suppression_and_in_bounds() {
    let out = compile_and_run_capture("<?php echo 'abc'[1]; echo @'abc'[99];");
    assert_eq!(out.stdout, "b");
    assert_eq!(out.stderr, "");
}

/// Checks a warning handler receives one complete warning per failed read.
#[test]
fn test_string_offset_warning_handler_receives_complete_message() {
    let out = compile_and_run_capture(r#"<?php
set_error_handler(function($level, $message) { echo $level, ':', $message, '|'; });
$s = 'abc';
$unused = $s[99];
"#);
    assert_eq!(out.stdout, "2:Uninitialized string offset 99|");
    assert_eq!(out.stderr, "");
}

/// Checks an existence probe stays quiet for a missing string offset.
#[test]
fn test_string_offset_silent_probes() {
    let out = compile_and_run_capture("<?php $s = 'abc'; echo isset($s[99]) ? 'bad' : 'ok';");
    assert_eq!(out.stdout, "ok");
    assert_eq!(out.stderr, "");
}

/// Null coalescing an out-of-bounds string offset selects the fallback silently.
#[test]
fn test_string_offset_null_coalesce_missing_is_silent() {
    let out = compile_and_run_capture("<?php $s = 'ab'; echo $s[99] ?? 'coalesce';");
    assert_eq!(out.stdout, "coalesce");
    assert_eq!(out.stderr, "");
}

/// An in-bounds string offset keeps its byte under null coalescing.
#[test]
fn test_string_offset_null_coalesce_present() {
    let out = compile_and_run_capture("<?php $s = 'ab'; echo $s[1] ?? 'coalesce';");
    assert_eq!(out.stdout, "b");
    assert_eq!(out.stderr, "");
}

/// Empty probes a missing string offset without reporting an ordinary-read warning.
#[test]
fn test_string_offset_empty_missing_is_silent() {
    let out = compile_and_run_capture("<?php $s = 'ab'; echo empty($s[99]) ? 'empty' : 'present';");
    assert_eq!(out.stdout, "empty");
    assert_eq!(out.stderr, "");
}

/// A float string offset truncates to an integer and reports PHP's offset-cast warning.
#[test]
fn test_string_offset_float_read_warns_once() {
    let out = compile_and_run_capture("<?php $offset = 1.9; echo 'abc'[$offset];");
    assert_eq!(out.stdout, "b");
    assert_eq!(out.stderr.matches("Warning: String offset cast occurred").count(), 1, "{}", out.stderr);
    assert!(!out.stderr.contains("Implicit conversion from float"), "{}", out.stderr);
}

/// Even an integral-valued float triggers PHP's string offset cast warning.
#[test]
fn test_string_offset_integral_float_read_warns_once() {
    let out = compile_and_run_capture("<?php echo 'abc'[1.0];");
    assert_eq!(out.stdout, "b");
    assert_eq!(out.stderr.matches("Warning: String offset cast occurred").count(), 1, "{}", out.stderr);
}

/// A boxed float retains the string-offset warning when the other branch is a string.
#[test]
fn test_string_offset_boxed_float_read_warns_once() {
    let out = compile_and_run_capture("<?php $offset = $argc > 0 ? 1.9 : '1'; echo 'abc'[$offset];");
    assert_eq!(out.stdout, "b");
    assert_eq!(out.stderr.matches("Warning: String offset cast occurred").count(), 1, "{}", out.stderr);
}

/// A cast warning handler can update a boxed variable before the string fetch.
#[test]
fn test_string_offset_boxed_float_handler_updates_offset_once() {
    let out = compile_and_run_capture(r#"<?php
$s = 'abc';
$offset = 1.9;
set_error_handler(function($level, $message) use (&$offset) {
    echo $level, ':', $message, '|';
    $offset = 2.9;
});
echo $s[$offset];
"#);
    assert_eq!(out.stdout, "2:String offset cast occurred|c");
    assert_eq!(out.stderr, "");
}

/// A boxed integer-form string offset remains silent on the runtime string branch.
#[test]
fn test_string_offset_boxed_integer_string_read_is_silent() {
    let out = compile_and_run_capture("<?php $offset = $argc > 0 ? '01' : 1.9; echo 'abc'[$offset];");
    assert_eq!(out.stdout, "b");
    assert_eq!(out.stderr, "");
}

/// PHP accepts integer-form string offsets without reporting a cast warning.
#[test]
fn test_string_offset_integer_string_read_is_silent() {
    let out = compile_and_run_capture("<?php echo 'abc'['01'];");
    assert_eq!(out.stdout, "b");
    assert_eq!(out.stderr, "");
}
