//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array set-operation builtins, including unique, diff, and intersect.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies the two-operand value-comparing array builtins compare BOXED elements by value.
///
/// These helpers once compared slots as raw 8-byte words — the value itself for an int or float,
/// a POINTER for anything heap-backed — so boxed elements compared cell addresses and two
/// separately boxed `3`s never matched (`array_diff([1,"b",3,4], [3,"z"])` answered `1,b,3,4`,
/// `array_intersect` of the same pair answered nothing). The calls were then refused. They now
/// compare by PHP's string rendering and keep the source keys, which is php 8.5's answer:
/// `1,b,4|3|0,1,3|2`. This test used to pin the refusal.
#[test]
fn test_value_comparing_builtins_compare_boxed_elements_by_value() {
    let out = compile_and_run(
        r#"<?php $a = [1, "b", 3, 4]; $b = [3, "z"]; echo implode(",", array_diff($a, $b)), "|", implode(",", array_intersect($a, $b)), "|", implode(",", array_keys(array_diff($a, $b))), "|", implode(",", array_keys(array_intersect($a, $b)));"#,
    );
    assert_eq!(out, "1,b,4|3|0,1,3|2");
}

/// Verifies `array_unique` compares BOXED elements by their PHP string rendering.
///
/// A boxed slot holds a CELL POINTER, so the raw-word scan compared cell addresses:
/// `array_unique([1,"b",1,4])` answered `1,b,1,4` where PHP answers `1,b,4`, and the call was
/// refused rather than answered wrongly — which is how `Command::__construct` (`array_unique([
/// ...$this->aliases, ...$aliases])`) stopped the Symfony compile. `SORT_STRING` is what makes
/// `[0, false, null, "0", ""]` keep `0` AND `false`: their renderings are `"0"` and `""`, then
/// `null` and `""` both render `""` and `"0"` renders `"0"`. PHP's `==` would have collapsed all
/// five to one, so this fixture is what distinguishes a string-rendering comparison from a loose
/// one.
#[test]
fn test_array_unique_compares_boxed_elements_by_string_rendering() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
var_dump(array_unique([1, "b", 1, 4]));
var_dump(array_unique([1, "1", true, "b"]));
var_dump(array_unique([0, false, null, "0", ""]));
var_dump(array_unique([1.5, "1.5", 2, "x", 2.0]));
$a = ["k", 3];
$b = [3, "k", "k"];
var_dump(array_unique(array_merge($a, $b)));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"b\"\n  [3]=>\n  int(4)\n}\n\
         array(2) {\n  [0]=>\n  int(1)\n  [3]=>\n  string(1) \"b\"\n}\n\
         array(2) {\n  [0]=>\n  int(0)\n  [1]=>\n  bool(false)\n}\n\
         array(3) {\n  [0]=>\n  float(1.5)\n  [2]=>\n  int(2)\n  [3]=>\n  string(1) \"x\"\n}\n\
         array(2) {\n  [0]=>\n  string(1) \"k\"\n  [1]=>\n  int(3)\n}\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the vendor shape: `array_unique` over a spread of two declared string arrays.
///
/// `Symfony\Component\Console\Command\Command::__construct` builds its alias list this way, and
/// the spread widens the element type to the boxed one the scan used to refuse. PHP keeps the
/// FIRST occurrence at its ORIGINAL key, so the surviving `three` sits at key 3 with no key 2.
/// No heap assertion: the same program with the `array_unique` call removed leaves the identical
/// one-block residual behind.
#[test]
fn test_array_unique_over_a_spread_of_declared_string_arrays() {
    let out = compile_and_run(
        r#"<?php
/**
 * @param string[] $existing
 */
function combine(array $existing, string $name): array {
    $parts = explode('|', $name);
    $first = array_shift($parts);
    $aliases = array_unique([
        ...$existing,
        ...$parts,
    ]);
    return [$first, $aliases];
}
var_dump(combine(['one', 'two'], 'go|two|three|one'));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  string(2) \"go\"\n  [1]=>\n  array(3) {\n    [0]=>\n    \
         string(3) \"one\"\n    [1]=>\n    string(3) \"two\"\n    [3]=>\n    string(5) \"three\"\n  \
         }\n}\n"
    );
}

/// Verifies the refusal of BOXED elements did not take the typed cases with it.
///
/// `array_diff` and `array_intersect` refuse a boxed source because they would compare cell
/// addresses (see `test_value_comparing_builtins_refuse_boxed_elements`). The refusal has to be
/// narrow: an `array<int>` slot IS the value, so raw comparison is the right one, and these must
/// keep working. `array_reverse` and `array_merge` share the element gate but never compare, so
/// they still accept a boxed array — that is why the refusal sits at each comparing builtin
/// rather than in the gate.
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

/// Verifies `array_diff()` accepts an array flowing through gradual storage and preserves values.
#[test]
fn test_array_diff_gradual_first_operand() {
    let out = compile_and_run(
        r#"<?php
function without_sent_values(mixed $previous, array $current): array {
    if ($previous !== null) {
        return array_diff($previous, $current);
    }
    return $current;
}
$result = without_sent_values(["first", "second", "third"], ["second"]);
echo count($result), ":", $result[0], ":", $result[2];
"#,
    );
    assert_eq!(out, "2:first:third");
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

/// Verifies `shuffle()` mutates and writes back an array kept in boxed `mixed` storage.
#[test]
fn test_shuffle_guarded_mixed_array() {
    let out = compile_and_run(
        r#"<?php
function shuffle_guarded(mixed $value): mixed {
    echo is_array($value) ? (shuffle($value) ? "ok:" : "bad:") : "not-array:";
    return $value;
}
$result = shuffle_guarded([42]);
echo count($result), ":", $result[0];
"#,
    );
    assert_eq!(out, "ok:1:42");
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
