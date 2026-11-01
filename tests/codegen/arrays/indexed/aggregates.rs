//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array aggregates, including reverse, sum, and product.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies one-argument `array_reverse()` supports packed string payloads and renumbers keys.
#[test]
fn test_array_reverse_string_compatibility_helper() {
    let out = compile_and_run(
        r#"<?php
$result = array_reverse(["alpha", "bravo", "charlie"]);
echo implode(",", $result);
"#,
    );
    assert_eq!(out, "charlie,bravo,alpha");
}

/// Verifies gradual array reversal accepts an array-or-false source, renumbers integer keys,
/// preserves string keys, and handles gaps without assuming dense source offsets.
#[test]
fn test_array_reverse_gradual_mixed_keys() {
    let out = compile_and_run(
        r#"<?php
function gradual_reverse_source(): array|false {
    return [2 => "first", "named" => "second", 5 => "third"];
}
foreach (array_reverse(gradual_reverse_source()) as $key => $value) {
    echo $key, "=>", $value, "|";
}
"#,
    );
    assert_eq!(out, "0=>third|named=>second|1=>first|");
}

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
