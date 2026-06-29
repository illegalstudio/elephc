//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array aggregates, including reverse, sum, and product.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Compiles `array_reverse($a)` and verifies the returned array has elements in reverse order.
/// Input: `[3, 1, 2]` → reversed to `[2, 1, 3]` → access via indices 0,1,2 yields `"213"`.
#[test]
fn test_array_reverse() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
$b = array_reverse($a);
echo $b[0] . $b[1] . $b[2];
"#,
    );
    assert_eq!(out, "213");
}

/// Compiles `array_sum($a)` and verifies integer summation of all elements.
/// Input: `[10, 20, 30]` → sum = 60.
#[test]
fn test_array_sum() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_sum($a);
"#,
    );
    assert_eq!(out, "60");
}

/// Compiles `array_product($a)` and verifies integer multiplication of all elements.
/// Input: `[2, 3, 4]` → product = 24.
#[test]
fn test_array_product() {
    let out = compile_and_run(
        r#"<?php
$a = [2, 3, 4];
echo array_product($a);
"#,
    );
    assert_eq!(out, "24");
}

/// Verifies array_sum() of a float[] accumulates as IEEE doubles (not raw 64-bit
/// integers). Uses a value comparison so the result is checked numerically rather
/// than via float-to-string formatting.
#[test]
fn test_array_sum_float_is_numeric() {
    let out = compile_and_run(
        r#"<?php
$a = [1.5, 2.25];
$s = array_sum($a);
echo ($s == 3.75) ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies array_product() of a float[] multiplies as IEEE doubles.
#[test]
fn test_array_product_float_is_numeric() {
    let out = compile_and_run(
        r#"<?php
$a = [1.5, 4.0];
$p = array_product($a);
echo ($p == 6.0) ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies the float sum prints the expected value (exact-representable so the
/// formatting is unambiguous), exercising the float result register path through echo.
#[test]
fn test_array_sum_float_display() {
    let out = compile_and_run(r#"<?php echo array_sum([1.5, 2.25]);"#);
    assert_eq!(out, "3.75");
}

/// Verifies array_sum() of an int[] is unaffected by the float routing (regression guard).
#[test]
fn test_array_sum_int_unaffected() {
    let out = compile_and_run(r#"<?php $a = [1, 2, 3]; echo array_sum($a);"#);
    assert_eq!(out, "6");
}

/// Verifies summing a float[] does not leak: the array is borrowed by the runtime
/// and released by the caller's epilogue.
#[test]
fn test_array_sum_float_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1.5, 2.5, 3.0];
$s = array_sum($a);
echo ($s == 7.0) ? "ok" : "bad";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
