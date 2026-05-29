//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions builtins misc, including function exists builtin, spread mixed with regular args, and implode integer array.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `function_exists("strlen")` returns `"yes"` for built-in PHP functions.
#[test]
fn test_function_exists_builtin() {
    let out = compile_and_run(r#"<?php echo function_exists("strlen") ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

/// Verifies spread arguments (`...$rest`) work correctly when mixed with regular positional args in a user-defined function call.
#[test]
fn test_spread_mixed_with_regular_args() {
    let out = compile_and_run(
        r#"<?php
function add3($a, $b, $c) { return $a + $b + $c; }
$rest = [20, 30];
echo add3(10, ...$rest);
"#,
    );
    assert_eq!(out, "60");
}

// Issue #17: Braceless single-statement bodies — verifies `implode` works with integer arrays.

/// Regression test for Issue #17: `implode` must correctly join integer array elements into a comma-separated string.
#[test]
fn test_implode_int_array() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
echo implode(", ", $a);
"#,
    );
    assert_eq!(out, "1, 2, 3");
}

/// Verifies >32 local variables do not cause stur/ldur offset overflow (Issue #22 regression).
/// Generates 50 integer variables, initializes each with its index, then sums $v0 + $v49 = 0 + 49 = 49.
#[test]
fn test_many_local_vars() {
    let mut php = String::from("<?php\nfunction f() {\n");
    for i in 0..50 {
        php.push_str(&format!("$v{} = {};\n", i, i));
    }
    // Sum some vars to ensure they're stored/loaded correctly
    php.push_str("echo $v0 + $v49;\n");
    php.push_str("}\nf();\n");
    let out = compile_and_run(&php);
    assert_eq!(out, "49");
}

/// Verifies `round(1.55, 1)` returns `"1.6"` (banker's rounding at 0.005 boundary).
#[test]
fn test_round_precision_1() {
    let out = compile_and_run("<?php echo round(1.55, 1);");
    assert_eq!(out, "1.6");
}

/// Verifies `round(3.14159, 2)` returns `"3.14"` (truncation at second decimal).
#[test]
fn test_round_precision_2() {
    let out = compile_and_run("<?php echo round(3.14159, 2);");
    assert_eq!(out, "3.14");
}

/// Verifies `rtrim("hello...", ".")` strips the trailing mask `"..."` and returns `"hello"`.
#[test]
fn test_rtrim_mask() {
    let out = compile_and_run(r#"<?php echo rtrim("hello...", ".");"#);
    assert_eq!(out, "hello");
}

/// Verifies `ltrim("000123", "0")` strips the leading zeros and returns `"123"`.
#[test]
fn test_ltrim_mask() {
    let out = compile_and_run(r#"<?php echo ltrim("000123", "0");"#);
    assert_eq!(out, "123");
}

/// Verifies `trim("**hello**", "*")` strips both leading and trailing asterisks and returns `"hello"`.
#[test]
fn test_trim_mask() {
    let out = compile_and_run(r#"<?php echo trim("**hello**", "*");"#);
    assert_eq!(out, "hello");
}

/// Verifies default trim masks include form-feed bytes on both sides.
#[test]
fn test_trim_default_mask_includes_form_feed() {
    let out = compile_and_run(r#"<?php echo "[" . trim("\f value \f") . "]";"#);
    assert_eq!(out, "[value]");
}

/// Verifies default ltrim masks include leading form-feed bytes.
#[test]
fn test_ltrim_default_mask_includes_form_feed() {
    let out = compile_and_run(r#"<?php echo "[" . ltrim("\f value") . "]";"#);
    assert_eq!(out, "[value]");
}

/// Verifies default rtrim masks include trailing form-feed bytes.
#[test]
fn test_rtrim_default_mask_includes_form_feed() {
    let out = compile_and_run(r#"<?php echo "[" . rtrim("value \f") . "]";"#);
    assert_eq!(out, "[value]");
}

/// Verifies explicit trim masks remain exact and do not strip form-feed unless requested.
#[test]
fn test_trim_explicit_mask_keeps_form_feed_when_omitted() {
    let out = compile_and_run(r#"<?php echo "[" . trim("\f value \f", " ") . "]";"#);
    assert_eq!(out, "[\x0c value \x0c]");
}

/// Verifies `chop()` behaves as PHP's alias for `rtrim()` and strips form-feed by default.
#[test]
fn test_chop_alias_trims_default_form_feed() {
    let out = compile_and_run(r#"<?php echo "[" . chop("value\f") . "]";"#);
    assert_eq!(out, "[value]");
}

/// Verifies `chop()` participates in case-insensitive namespaced builtin fallback.
#[test]
fn test_chop_case_insensitive_namespaced_builtin() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo ChOp("value\f");
"#,
    );
    assert_eq!(out, "value");
}

/// Verifies `min(3, 1, 2)` returns `"1"` (smallest of three integers).
#[test]
fn test_min_three_args() {
    let out = compile_and_run("<?php echo min(3, 1, 2);");
    assert_eq!(out, "1");
}

/// Verifies `max(1, 3, 2)` returns `"3"` (largest of three integers).
#[test]
fn test_max_three_args() {
    let out = compile_and_run("<?php echo max(1, 3, 2);");
    assert_eq!(out, "3");
}

/// Verifies `min(5, 4, 3, 2, 1)` returns `"1"` (smallest of five integers in descending order).
#[test]
fn test_min_five_args() {
    let out = compile_and_run("<?php echo min(5, 4, 3, 2, 1);");
    assert_eq!(out, "1");
}
