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

/// `array_diff()`, `array_intersect()`, `array_diff_key()` and `array_intersect_key()` keep each
/// survivor's ORIGINAL key, as PHP does: an indexed first operand yields an integer-keyed hash
/// rather than a renumbered list. Covers int, float, string and numeric-string elements,
/// associative operands on either side, an int operand against a string one (string-cast
/// equality), and `json_encode` (a sparse array encodes as an object). Regression for #1645.
#[test]
fn test_set_operations_keep_the_surviving_keys() {
    let out = compile_and_run(
        r#"<?php
echo json_encode(array_diff([1, 2, 3], [2])), "\n";
echo json_encode(array_diff([1.5, 2.5, 3.5], [2.5])), "\n";
echo json_encode(array_diff(["a", "b", "c", "d"], ["b", "d"])), "\n";
echo json_encode(array_diff(["1", "01", "2"], ["1"])), "\n";
echo json_encode(array_diff([1, 2, 3], ["2"])), "\n";
echo json_encode(array_diff(["x" => 1, "y" => 2, "z" => 3], [2])), "\n";
echo json_encode(array_diff([1, 2, 3], ["k" => 2])), "\n";
echo json_encode(array_intersect([1, 2, 3, 4], [2, 4])), "\n";
echo json_encode(array_intersect(["a", "b", "c"], ["c", "a"])), "\n";
echo json_encode(array_diff_key([10, 20, 30], [1 => 0])), "\n";
echo json_encode(array_intersect_key([10, 20, 30], [0 => 0, 2 => 0])), "\n";
echo json_encode(array_diff([1, 2], [1, 2])), json_encode(array_diff([1, 2], [9])), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "{\"0\":1,\"2\":3}\n",
            "{\"0\":1.5,\"2\":3.5}\n",
            "{\"0\":\"a\",\"2\":\"c\"}\n",
            "{\"1\":\"01\",\"2\":\"2\"}\n",
            "{\"0\":1,\"2\":3}\n",
            "{\"x\":1,\"z\":3}\n",
            "{\"0\":1,\"2\":3}\n",
            "{\"1\":2,\"3\":4}\n",
            "{\"0\":\"a\",\"2\":\"c\"}\n",
            "{\"0\":10,\"2\":30}\n",
            "{\"0\":10,\"2\":30}\n",
            "[][1,2]\n",
        )
    );
}

/// The kept keys are real keys: `$d[2]` reads the survivor, `isset($d[1])` sees the hole, a
/// `foreach` walks the original keys and `array_values()` renumbers them. Regression for #1645.
#[test]
fn test_array_diff_result_is_indexed_by_the_original_keys() {
    let out = compile_and_run(
        r#"<?php
$d = array_diff([1, 2, 3], [2]);
echo $d[2] ?? "missing", "|", isset($d[1]) ? "has1" : "no1", "|", count($d), "\n";
foreach (array_intersect(["p", "q", "r"], ["r", "p"]) as $k => $v) { echo $k, "=", $v, ","; }
echo "\n", json_encode(array_values(array_diff([5, 6, 7, 8], [6, 8]))), "\n";
"#,
    );
    assert_eq!(out, "3|no1|2\n0=p,2=r,\n[5,7]\n");
}

/// The key-preserving set operations leave the heap clean, including string survivors (persisted
/// into the result), string-cast comparisons between an int and a string operand (the casts are
/// freed and the concat scratch rewound), and the key operations whose literal operands used to
/// stay alive (they were not marked as returning fresh storage). Regression for #1645.
#[test]
fn test_key_preserving_set_operations_heap_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$t = 0;
for ($i = 0; $i < 40 + ($argc > 5 ? 1 : 0); $i++) {
    $t += count(array_diff(["a", "b" . $i, "c"], ["c", "x"]));
    $t += count(array_diff([1, 2, 3], ["2", "x" . $i]));
    $t += count(array_intersect(["a", "b" . $i, "c"], ["c", "a"]));
    $t += count(array_diff(["x" => "p" . $i, "y" => "q"], ["q"]));
    $t += count(array_diff_key(["a" . $i, "b", "c"], [1 => 0]));
    $t += count(array_intersect_key([10, 20, 30], [0 => 0, 2 => 0]));
    $t += count(array_diff_key([5 => 1, 6 => 2], [5 => 0]));
}
echo $t, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "480\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies the key and pair set operations and `array_merge_recursive` release what they are
/// handed, run in a loop under `--heap-debug`.
///
/// Two leaks: the five builtins sat in the default `MayAliasArguments` ownership bucket, so the
/// call-argument pin on a named first operand was never released and `$a` leaked its whole table
/// on every call; and `array_diff_assoc` / `array_intersect_assoc` never released the persisted
/// string each compared string value renders to. The loop leaked 420 blocks.
#[test]
fn test_key_and_pair_set_operations_release_their_operands() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function run(): string {
    $out = [];
    for ($i = 0; $i < 30; $i++) {
        $a = ["x" => "k" . $i, "y" => "v", "z" => 3, "n" => 12.5];
        $b = ["y" => "v", "z" => "3", "w" => "k" . $i, "n" => 12];
        $out = [
            json_encode(array_diff_key($a, $b)),
            json_encode(array_intersect_key($a, $b)),
            json_encode(array_diff_assoc($a, $b)),
            json_encode(array_intersect_assoc($a, $b)),
            json_encode(array_merge_recursive($a, $b)),
        ];
    }
    return implode("|", $out);
}
echo run();
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"{"x":"k29"}|{"y":"v","z":3,"n":12.5}|{"x":"k29","n":12.5}|{"y":"v","z":3}|{"x":"k29","y":["v","v"],"z":[3,"3"],"n":[12.5,12],"w":"k29"}"#,
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
