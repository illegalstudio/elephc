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

/// Compiles `array_sum($a)`/`array_product($a)` over a float-element array and verifies the
/// floating-point runtime helpers accumulate and return in the float result register.
/// Input: `[1.5, 2.5, 3.5]` → sum = 7.5; `[2.5, 3.0]` → product = 7.5 (matches PHP).
#[test]
fn test_array_sum_and_product_over_float_array() {
    let out = compile_and_run(
        r#"<?php
$sum = [1.5, 2.5, 3.5];
$prod = [2.5, 3.0];
echo array_sum($sum);
echo "|";
echo array_product($prod);
"#,
    );
    assert_eq!(out, "7.5|7.5");
}
