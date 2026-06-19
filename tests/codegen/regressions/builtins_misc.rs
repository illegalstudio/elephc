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
fn test_var_dump_array_int_elements() {
    // Regression: var_dump used to print "array(N) {\n}\n" with no
    // element bodies. Indexed int arrays now walk through
    // __rt_var_dump_array_int and emit per-element "  [N]=>\n  int(V)\n".
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
var_dump($a);
"#,
    );
    assert_eq!(out, "array(3) {\n  [0]=>\n  int(10)\n  [1]=>\n  int(20)\n  [2]=>\n  int(30)\n}\n");
}

/// Regression: `var_dump` on an associative array (hash) used to print just
/// `array(N) {\n}\n` with no entries. It now walks the hash via
/// `__rt_var_dump_hash`, formatting string keys as `["key"]=>` and the scalar
/// value beneath. Output matches PHP exactly (cross-checked with `php -r`).
#[test]
fn test_var_dump_hash_string_keys() {
    let out = compile_and_run(r#"<?php var_dump(["a" => 1, "b" => 2, "c" => 3]);"#);
    assert_eq!(
        out,
        "array(3) {\n  [\"a\"]=>\n  int(1)\n  [\"b\"]=>\n  int(2)\n  [\"c\"]=>\n  int(3)\n}\n"
    );
}

/// Regression: `var_dump` on an integer-keyed hash renders keys as `[N]=>`
/// (no quotes), matching PHP.
#[test]
fn test_var_dump_hash_int_keys() {
    let out = compile_and_run(r#"<?php var_dump([10 => "x", 20 => "y"]);"#);
    assert_eq!(
        out,
        "array(2) {\n  [10]=>\n  string(1) \"x\"\n  [20]=>\n  string(1) \"y\"\n}\n"
    );
}

/// Regression: `var_dump` on a heterogeneous hash formats each scalar value by
/// its runtime tag (int/string/float/bool) and renders null entries as `NULL`.
#[test]
fn test_var_dump_hash_heterogeneous_values() {
    let out = compile_and_run(
        r#"<?php var_dump(["name" => "Alice", "age" => 30, "score" => 4.5, "ok" => true, "nil" => null]);"#,
    );
    assert_eq!(
        out,
        "array(5) {\n  [\"name\"]=>\n  string(5) \"Alice\"\n  [\"age\"]=>\n  int(30)\n  [\"score\"]=>\n  float(4.5)\n  [\"ok\"]=>\n  bool(true)\n  [\"nil\"]=>\n  NULL\n}\n"
    );
}

/// Regression: a nested array value inside a hash falls back to `NULL` (the
/// same limitation as the indexed Mixed walker) instead of crashing or
/// emitting garbage. The surrounding scalar entries still format correctly.
#[test]
fn test_var_dump_hash_nested_value_falls_back_to_null() {
    let out = compile_and_run(r#"<?php var_dump(["x" => 5, "inner" => [1, 2], "y" => 7]);"#);
    assert_eq!(
        out,
        "array(3) {\n  [\"x\"]=>\n  int(5)\n  [\"inner\"]=>\n  NULL\n  [\"y\"]=>\n  int(7)\n}\n"
    );
}

/// Regression: a hash reached through a boxed `Mixed` value (e.g. an associative
/// array read out of a heterogeneous array, the shape that builtins like
/// `getdate()`/`DateTimeZone::getLocation()` return) is walked by the hash
/// formatter after the Mixed unbox, instead of printing the empty array shell.
#[test]
fn test_var_dump_mixed_boxed_hash() {
    let out = compile_and_run(
        r#"<?php
$outer = ["h" => ["a" => 1, "b" => 2], "n" => 9];
var_dump($outer["h"]);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [\"a\"]=>\n  int(1)\n  [\"b\"]=>\n  int(2)\n}\n"
    );
}

/// Verifies compiled PHP output for var dump array bool elements.
#[test]
fn test_var_dump_array_bool_elements() {
    // Phase 11 follow-up: var_dump([true, false, ...]) walks via
    // __rt_var_dump_array_bool, emitting per-element bool(true|false).
    let out = compile_and_run(
        r#"<?php
$a = [true, false, true];
var_dump($a);
"#,
    );
    assert_eq!(out, "array(3) {\n  [0]=>\n  bool(true)\n  [1]=>\n  bool(false)\n  [2]=>\n  bool(true)\n}\n");
}

/// Verifies compiled PHP output for var dump array float elements.
#[test]
fn test_var_dump_array_float_elements() {
    // Phase 11 follow-up: var_dump([1.5, 2.75, 3.0]) walks via
    // __rt_var_dump_array_float, formatting each f64 through __rt_ftoa.
    let out = compile_and_run(
        r#"<?php
$a = [1.5, 2.75, 3.0];
var_dump($a);
"#,
    );
    assert_eq!(out, "array(3) {\n  [0]=>\n  float(1.5)\n  [1]=>\n  float(2.75)\n  [2]=>\n  float(3)\n}\n");
}

/// Verifies compiled PHP output for var dump array str elements.
#[test]
fn test_var_dump_array_str_elements() {
    // Same regression as test_var_dump_array_int_elements but for
    // indexed string arrays — walks via __rt_var_dump_array_str and
    // emits `  [N]=>\n  string(LEN) "VAL"\n` per element.
    let out = compile_and_run(
        r#"<?php
$a = ["hello", "world"];
var_dump($a);
"#,
    );
    assert_eq!(out, "array(2) {\n  [0]=>\n  string(5) \"hello\"\n  [1]=>\n  string(5) \"world\"\n}\n");
}

/// Verifies compiled PHP output for function exists builtin.
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

/// Verifies `abs()` of a boxed Mixed value (a heterogeneous-array element) applies the
/// numeric absolute value per the runtime tag instead of operating on the boxed-cell
/// pointer. Before the fix this printed garbage pointers; the int and float results must
/// both be correct and keep PHP's int→int / float→float formatting.
#[test]
fn test_abs_mixed_array_element_preserves_int_and_float() {
    let out = compile_and_run(
        r#"<?php $a = [-5, -2.5, 7, -3.25]; foreach ($a as $v) { echo abs($v), ","; }"#,
    );
    assert_eq!(out, "5,2.5,7,3.25,");
}

/// Verifies `pow()` coerces boxed Mixed operands (heterogeneous-array elements) to their
/// numeric value before exponentiating, instead of converting the boxed-cell pointer.
/// Covers a Mixed base, a Mixed exponent, and both operands Mixed.
#[test]
fn test_pow_mixed_base_and_exponent() {
    let out = compile_and_run(
        r#"<?php $a = [-5, -2.5, 4, 3]; echo pow($a[0], 2), "|", pow(2, $a[1]), "|", pow($a[2], $a[3]);"#,
    );
    assert_eq!(out, "25|0.17677669529664|64");
}

/// Verifies `is_float()` of a boxed Mixed value (a heterogeneous-array element) tests the
/// runtime tag instead of constant-folding to false. Before the fix every element of a
/// mixed int/float array reported "i"; floats must report "f" while ints/strings/bools
/// report "i", matching the sibling predicates (is_int already did this).
#[test]
fn test_is_float_mixed_array_element_tests_runtime_tag() {
    let out = compile_and_run(
        r#"<?php $a = [-5, -2.5, 7, 3.25, "x", true]; foreach ($a as $v) { echo (is_float($v) ? "f" : "i"); }"#,
    );
    assert_eq!(out, "ififii");
}

/// Verifies `is_nan`/`is_finite`/`is_infinite` of a boxed Mixed value cast the payload to a
/// double (via `__rt_mixed_cast_float`) before the IEEE check, instead of treating the cell
/// pointer as a float. Iterates a heterogeneous float/int array so the elements are Mixed.
#[test]
fn test_is_nan_finite_infinite_mixed_array_element() {
    let out = compile_and_run(
        r#"<?php $a = [NAN, 7, INF, 2.5]; foreach ($a as $v) { echo (is_nan($v)?"N":"-"), (is_finite($v)?"F":"-"), (is_infinite($v)?"I":"-"), "|"; }"#,
    );
    assert_eq!(out, "N--|-F-|--I|-F-|");
}

/// Verifies `is_infinite(NAN)` is false. On x86_64 the `ucomisd`/`sete` infinity check set
/// the zero flag for the unordered NaN comparison and wrongly reported NaN as infinite; the
/// fix adds a NaN parity guard (the NaN comparison is unordered, never equal to ±Inf).
#[test]
fn test_is_infinite_nan_is_false() {
    let out = compile_and_run(
        r#"<?php echo (is_infinite(NAN)?"1":"0"), (is_infinite(INF)?"1":"0"), (is_infinite(-INF)?"1":"0"), (is_infinite(2.5)?"1":"0");"#,
    );
    assert_eq!(out, "0110");
}

/// Verifies `is_numeric()` of a boxed Mixed value unboxes the runtime tag and, for a string
/// payload, runs the numeric-string scan — instead of constant-folding to false. Covers
/// int/float (numeric), numeric and non-numeric strings, bool, and a bare `.`.
#[test]
fn test_is_numeric_mixed_array_element() {
    let out = compile_and_run(
        r#"<?php $a = [2.5, "3.14", 5, "x", true, "-7", "."]; foreach ($a as $v) { echo (is_numeric($v) ? "1" : "0"); }"#,
    );
    assert_eq!(out, "1110010");
}

/// Regression: a user-defined function in a namespace whose name collides with a procedural
/// date/time alias (e.g. `date_diff`) must NOT be hijacked into the OOP desugaring. The name
/// resolver only rewrites the alias when no user function of that name is declared.
#[test]
fn test_namespaced_user_function_shadows_date_alias() {
    let out = compile_and_run(
        r#"<?php
namespace App;
function date_diff($a, $b) { return "user:" . ($a + $b); }
function timezone_name_get($x) { return "tz:" . $x; }
echo date_diff(1, 2), "|", timezone_name_get(5);
"#,
    );
    assert_eq!(out, "user:3|tz:5");
}
