//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array set-operation builtins, including unique, diff, and intersect.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies the value-comparing array builtins refuse BOXED elements rather than compare their
/// addresses.
///
/// These helpers compare slots as raw 8-byte words — the value itself for an int or float, a
/// POINTER for anything heap-backed. Boxed elements therefore compared cell addresses, and two
/// separately boxed `3`s never matched:
///
/// - `array_diff([1,"b",3,4], [3,"z"])` answered `1,b,3,4`, PHP answers `1,b,4`
/// - `array_intersect` of the same pair answered NOTHING, PHP answers `3`
/// - `array_unique([1,"b",1,4])` answered `1,b,1,4`, PHP answers `1,b,4`
///
/// All three silent. PHP compares these elements by their STRING rendering, which needs a
/// by-value comparison in the runtime; until that exists the calls are refused, exactly as
/// `array<string>` already is — its 16-byte slots do not fit these helpers either.
#[test]
fn test_value_comparing_builtins_refuse_boxed_elements() {
    for (source, message) in [
        (
            r#"<?php $a = [1, "b", 3, 4]; $b = [3, "z"]; $r = array_diff($a, $b);"#,
            "array_diff compares boxed elements by identity",
        ),
        (
            r#"<?php $a = [1, "b", 3, 4]; $b = [3, "z"]; $r = array_intersect($a, $b);"#,
            "array_intersect compares boxed elements by identity",
        ),
        (
            r#"<?php $a = [1, "b", 1, 4]; $r = array_unique($a);"#,
            "array_unique compares boxed elements by identity",
        ),
    ] {
        let error = compile_source_expect_backend_error(source);
        assert!(
            error.contains(message),
            "expected `{message}` for this source, got: {error}"
        );
    }
}

/// Verifies the refusal of BOXED elements did not take the typed cases with it.
///
/// `array_diff`, `array_intersect` and `array_unique` refuse a boxed source because they would
/// compare cell addresses (see `test_error_value_comparing_builtins_refuse_boxed_elements`).
/// The refusal has to be narrow: an `array<int>` slot IS the value, so raw comparison is the
/// right one, and these three must keep working. `array_reverse` and `array_merge` share the
/// element gate but never compare, so they still accept a boxed array — that is why the
/// refusal sits at each comparing builtin rather than in the gate.
#[test]
fn test_value_comparing_builtins_still_accept_typed_elements() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", array_diff([1, 2, 3], [2])), "|";
echo implode(",", array_intersect([1, 2, 3], [2, 3])), "|";
echo implode(",", array_unique([1, 2, 2, 3])), "|";
$boxed = [1, "b", 3];
$more = [9, "z"];
echo implode(",", array_reverse($boxed)), "|";
echo implode(",", array_merge($boxed, $more));
"#,
    );
    assert_eq!(out, "1,3|2,3|1,2,3|3,b,1|1,b,3,9,z");
}

/// Verifies `array_unique()` removes duplicate values; count of `[1,2,2,3,3,3]` is 3.
#[test]
fn test_array_unique() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 2, 3, 3, 3];
$b = array_unique($a);
echo count($b);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies `array_diff()` returns values from `$a` not present in `$b`; count of `[1,2,3,4]` vs `[2,4]` is 2.
#[test]
fn test_array_diff() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
$b = [2, 4];
$c = array_diff($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies `array_intersect()` returns values present in both `$a` and `$b`; count of `[1,2,3,4]` vs `[2,4,6]` is 2.
#[test]
fn test_array_intersect() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
$b = [2, 4, 6];
$c = array_intersect($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies `array_rand()` returns a valid key/index within the array bounds `[0, 3)`.
#[test]
fn test_array_rand() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$i = array_rand($a);
if ($i >= 0 && $i < 3) { echo "ok"; }
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `shuffle()` permutes all elements without losing any; count stays 5, sum stays 15.
#[test]
fn test_shuffle() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
shuffle($a);
echo count($a);
echo array_sum($a);
"#,
    );
    assert_eq!(out, "515");
}

/// Verifies `array_diff_key()` removes entries by key; count of `["a"=>"1","b"=>"2"]` minus key "a" is 1.
#[test]
fn test_array_diff_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => "1", "b" => "2"];
$b = ["a" => "9"];
$c = array_diff_key($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "1");
}

/// Regression: verifies borrowed arrays inside `$src` are not freed when `$src` is unset after `array_diff_key()`.
#[test]
fn test_gc_array_diff_key_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$src = ["keep" => [1, 2], "drop" => [3, 4]];
$mask = ["drop" => 1];
$filtered = array_diff_key($src, $mask);
unset($src);
$saved = $filtered["keep"];
echo $saved[1];
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies `array_intersect_key()` keeps only entries with matching keys; count of `["a"=>"1","b"=>"2"]` intersecting key "a" is 1.
#[test]
fn test_array_intersect_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => "1", "b" => "2"];
$b = ["a" => "9"];
$c = array_intersect_key($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "1");
}

/// Regression: verifies borrowed arrays inside `$src` are not freed when `$src` is unset after `array_intersect_key()`.
#[test]
fn test_gc_array_intersect_key_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$src = ["keep" => [5, 6], "drop" => [7, 8]];
$mask = ["keep" => 1];
$filtered = array_intersect_key($src, $mask);
unset($src);
$saved = $filtered["keep"];
echo $saved[0] . "|" . $saved[1];
"#,
    );
    assert_eq!(out, "5|6");
}
