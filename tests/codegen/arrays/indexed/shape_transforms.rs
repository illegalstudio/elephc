//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array shape-transform builtins, including fill, pad, and splice.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests `array_fill(start_index, num, value)` — creates a 3-element array indexed from 0,
/// all initialized to 42, then accesses elements via integer index.
#[test]
fn test_array_fill() {
    let out = compile_and_run(
        r#"<?php
$a = array_fill(0, 3, 42);
echo $a[0] . " " . $a[1] . " " . $a[2];
"#,
    );
    assert_eq!(out, "42 42 42");
}

/// Tests `array_pad($array, length, value)` — pads `[1, 2]` to length 5 with trailing `0`
/// entries, then verifies the resulting array has exactly 5 elements via `count()`.
#[test]
fn test_array_pad() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = array_pad($a, 5, 0);
echo count($b);
"#,
    );
    assert_eq!(out, "5");
}

/// Tests `array_splice(&$array, offset, length)` — removes 2 elements starting at index 1
/// from `[1, 2, 3, 4, 5]`, captures the removed portion, and verifies both counts.
#[test]
fn test_array_splice() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$removed = array_splice($a, 1, 2);
echo count($removed) . " " . count($a);
"#,
    );
    assert_eq!(out, "2 3");
}

/// Tests `array_combine($keys, $values)` — combines `["a", "b"]` keys with `[1, 2]` values
/// into an associative array, then verifies the resulting array has exactly 2 elements.
#[test]
fn test_array_combine() {
    let out = compile_and_run(
        r#"<?php
$keys = ["a", "b"];
$vals = [1, 2];
$m = array_combine($keys, $vals);
echo count($m);
"#,
    );
    assert_eq!(out, "2");
}

/// Tests `array_flip($array)` — inverts values-to-keys on `[10, 20, 30]`, producing a map
/// with 3 entries. Verifies count only.
#[test]
fn test_array_flip() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$f = array_flip($a);
echo count($f);
"#,
    );
    assert_eq!(out, "3");
}

/// Tests `array_flip` integer-value key normalization — flips `[10, 20]`, then accesses
/// flipped keys using both integer (`$f[10]`) and string (`$f["20"]`) index forms, verifying
/// PHP's loose-key comparison for integer-like string keys.
#[test]
fn test_array_flip_integer_values_are_integer_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20];
$f = array_flip($a);
echo $f[10] . "|" . $f["20"];
"#,
    );
    assert_eq!(out, "0|1");
}

/// Tests `array_flip` with string values that normalize to the same integer key — flips
/// `["1", "02", "2"]` where "02" and "2" collide under PHP integer-string key normalization,
/// then verifies the resulting count is 3 and each flipped entry is accessible by its
/// canonical integer key.
#[test]
fn test_array_flip_string_values_normalize_numeric_keys() {
    let out = compile_and_run(
        r#"<?php
$a = ["1", "02", "2"];
$f = array_flip($a);
echo count($f) . "|" . $f[1] . "|" . $f["02"] . "|" . $f["2"];
"#,
    );
    assert_eq!(out, "3|0|1|2");
}

/// Tests `array_chunk($array, size)` — splits `[1, 2, 3, 4, 5]` into chunks of size 2,
/// producing 3 chunks. Verifies chunk count via `count()`.
#[test]
fn test_array_chunk() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$c = array_chunk($a, 2);
echo count($c);
"#,
    );
    assert_eq!(out, "3");
}

/// Regression (M8): array_chunk() returns an array of sub-arrays, but the codegen local-type
/// table inferred Array(Int) (dropping a nesting level), so indexing/iterating a chunk treated the
/// sub-array pointer as an int. The infer now nests as Array(Array(Int)), matching the emitter, so
/// `$c[i][j]` and `foreach` over the chunks read real sub-arrays.
#[test]
fn test_array_chunk_nested_element_indexing() {
    let out = compile_and_run(
        r#"<?php
$c = array_chunk([1, 2, 3, 4, 5, 6], 2);
echo $c[0][0] . $c[0][1] . "|" . $c[2][0] . $c[2][1];
echo "/";
$sum = 0;
foreach ($c as $pair) {
    $sum = $sum + $pair[0] + $pair[1];
}
echo $sum;
"#,
    );
    assert_eq!(out, "12|56/21");
}

/// Regression (M8): array_column() returns the column's value type, but the infer table reported
/// the row element type. With string columns the result-array element type must be Str so the
/// foreach value var is sized for a string; the infer now mirrors the emitter's column value type.
#[test]
fn test_array_column_string_values_foreach() {
    let out = compile_and_run(
        r#"<?php
$rows = [
    ["id" => 1, "name" => "alice"],
    ["id" => 2, "name" => "bob"],
];
$names = array_column($rows, "name");
$out = "";
foreach ($names as $n) {
    $out = $out . $n . ",";
}
echo $out;
"#,
    );
    assert_eq!(out, "alice,bob,");
}

/// Regression (M8): array_rand() returns a single key, but the infer table reported an Array, so a
/// local holding the result was mis-typed. The infer now reports Int, matching the emitter; the
/// returned key indexes back into the source array.
#[test]
fn test_array_rand_single_key_is_scalar() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$k = array_rand($a);
$ok = ($k === 0 || $k === 1 || $k === 2) && ($a[$k] === 10 || $a[$k] === 20 || $a[$k] === 30);
echo $ok ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Tests `array_fill_keys($keys, value)` — creates an array from `["x", "y"]` as keys,
/// both initialized to `0`, then verifies the resulting associative array has exactly 2 entries.
#[test]
fn test_array_fill_keys() {
    let out = compile_and_run(
        r#"<?php
$keys = ["x", "y"];
$m = array_fill_keys($keys, 0);
echo count($m);
"#,
    );
    assert_eq!(out, "2");
}
