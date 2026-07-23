//! Purpose:
//! Regression coverage for issue #485: `strlen()` on a boxed `?string` value must not
//! leak a persisted copy of the string payload once per call.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A `?string` return is boxed into a Mixed cell whose lifecycle is balanced; the
//!   historical leak was `strlen()`'s Mixed-argument lowering calling
//!   `__rt_mixed_cast_string`, which persists an owned copy that was never released.
//! - Fixtures assert both correct output and `HEAP DEBUG: leak summary: clean`.

use crate::support::*;

/// Verifies a single `strlen()` call on a runtime-built `?string` return is heap-clean
/// and the boxed value remains printable after the length read.
#[test]
fn test_nullable_string_return_strlen_single_call_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function s(int $n): ?string { return str_repeat("x", 3 + ($n % 2)); }
$v = s(1);
echo strlen($v) . ":" . $v;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4:xxxx");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap after one strlen() on a boxed ?string, got: {}",
        out.stderr
    );
}

/// Verifies the exact issue #485 repro: looped `strlen()` calls on a `?string`
/// return leak nothing per call.
#[test]
fn test_nullable_string_return_strlen_loop_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function s(int $n): ?string { return str_repeat("x", 3 + ($n % 2)); }
$len = 0;
for ($n = 0; $n < 50; $n++) { $v = s($n); $len += strlen($v); }
echo $len;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "175");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected no per-call leak across 50 strlen() calls, got: {}",
        out.stderr
    );
}

/// Verifies the boxed string stays valid after `strlen()` consumed it: the length
/// read must not free or corrupt the live payload used by later reads.
#[test]
fn test_nullable_string_return_value_survives_strlen() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function s(int $n): ?string { return str_repeat("ab", $n); }
$v = s(2);
$n = strlen($v);
echo $n . ":" . $v . ":" . strtoupper($v);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4:abab:ABAB");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap with the boxed string still live after strlen(), got: {}",
        out.stderr
    );
}

/// Verifies the null branch of the `?string` return stays heap-clean and observable.
#[test]
fn test_nullable_string_return_null_branch_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function s(int $n): ?string {
    if ($n % 2 == 0) { return null; }
    return str_repeat("x", $n);
}
$len = 0;
for ($n = 0; $n < 10; $n++) {
    $v = s($n);
    if ($v === null) { $len += 1; } else { $len += strlen($v); }
}
echo $len;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "30");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap across null and string branches, got: {}",
        out.stderr
    );
}

/// Verifies reading the `?string` result across another function-return boundary
/// (callee parameter consumes the boxed value) stays heap-clean per call.
#[test]
fn test_nullable_string_return_boundary_read_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function s(int $n): ?string { return str_repeat("x", $n); }
function measure(?string $v): int {
    if ($v === null) { return 0; }
    return strlen($v);
}
$total = 0;
for ($n = 0; $n < 20; $n++) { $total += measure(s($n)); }
echo $total;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "190");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected no per-call leak across the ?string parameter boundary, got: {}",
        out.stderr
    );
}
