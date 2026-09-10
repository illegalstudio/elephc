//! Purpose:
//! Regression tests for the associative-array sorts that reorder a hash table's
//! insertion-order chain: `ksort()`, `krsort()`, `asort()` and `arsort()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is verbatim `LC_ALL=C php` (PHP 8.4.20) output for the same fixture.
//! - Before this suite, `ksort()`/`krsort()` on a hash were runtime no-ops that returned
//!   the receiver untouched with no diagnostic; the string-key case is the original repro.
//! - Sorting only relinks `prev`/`next`/`head`/`tail`, so the fixtures also assert that key
//!   association, later key lookups, later inserts and copy-on-write all still hold, and
//!   one fixture re-checks the heap under `--heap-debug`.
//! - A large reverse-order fixture guards the merge-sort path against accidentally
//!   reintroducing quadratic insertion behavior.
//! - PHP's key ordering is `zend_compare`, not a byte-wise order: `10` sorts before
//!   `'Banana'` and `'0.5'` before `2`, which the mixed-key fixture pins.

use crate::support::*;

/// Issue repro: `ksort()`/`krsort()` over a string-keyed associative array used to leave the
/// receiver in insertion order without any diagnostic. Both directions must now reorder it.
#[test]
fn test_ksort_krsort_string_keys() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
krsort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "a=3;b=2;c=1;|c=1;b=2;a=3;");
}

/// The original one-line repro: `implode(",", array_keys($a))` after `ksort()`.
#[test]
fn test_ksort_string_keys_through_array_keys() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1];
ksort($a);
echo implode(",", array_keys($a));
"#,
    );
    assert_eq!(out, "a,b,c");
}

/// Sparse integer keys must sort numerically (`-1 < 2 < 10 < 33`), not by insertion order
/// and not by the decimal text of the key.
#[test]
fn test_ksort_krsort_integer_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [10 => "x", 2 => "y", 33 => "z", -1 => "w"];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
krsort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "-1=w;2=y;10=x;33=z;|33=z;10=x;2=y;-1=w;");
}

/// Mixed integer and string keys follow PHP's standard comparison, so `''` sorts before the
/// integer `2` and the integer `10` sorts before `'Banana'` — not a lexicographic order.
#[test]
fn test_ksort_krsort_mixed_int_and_string_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [10 => "a", "9" => "b", "apple" => "c", "Banana" => "d", 2 => "e", "" => "f"];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$b = [10 => "a", "9" => "b", "apple" => "c", "Banana" => "d", 2 => "e", "" => "f"];
krsort($b);
foreach ($b as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(
        out,
        "=f;2=e;9=b;10=a;Banana=d;apple=c;|apple=c;Banana=d;10=a;9=b;2=e;=f;"
    );
}

/// An empty receiver — both a literal `[]` and a hash emptied with `unset()` — must sort to
/// itself without touching the header's head/tail sentinels.
#[test]
fn test_ksort_krsort_empty_array() {
    let out = compile_and_run(
        r#"<?php
$a = [];
ksort($a);
echo count($a), ";";
krsort($a);
echo count($a), ";";
$b = ["k" => 1];
unset($b["k"]);
ksort($b);
echo count($b), ";";
krsort($b);
echo count($b);
"#,
    );
    assert_eq!(out, "0;0;0;0");
}

/// A single-entry hash must survive both directions with its one key/value pair intact.
#[test]
fn test_ksort_krsort_single_element() {
    let out = compile_and_run(
        r#"<?php
$a = ["only" => 7];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
krsort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "only=7;only=7;");
}

/// `asort()`/`arsort()` over duplicate values must be stable in both directions: `b`, `d`
/// and `e` all hold `2` and keep their original relative order, exactly like PHP 8.
#[test]
fn test_asort_arsort_duplicate_values_are_stable() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1, "d" => 2, "e" => 2];
asort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$b = ["b" => 2, "a" => 3, "c" => 1, "d" => 2, "e" => 2];
arsort($b);
foreach ($b as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "c=1;b=2;d=2;e=2;a=3;|a=3;b=2;d=2;e=2;c=1;");
}

/// `asort()`/`arsort()` over string values compare with PHP's ordering, not by slot width.
#[test]
fn test_asort_arsort_string_values() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => "pear", "a" => "apple", "c" => "fig"];
asort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
arsort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "a=apple;c=fig;b=pear;|b=pear;c=fig;a=apple;");
}

/// Copy-on-write: a copy taken before the sort must keep the original iteration order, in
/// both directions. The sorters mutate the table in place, so the receiver has to be split
/// with `__rt_hash_ensure_unique` first.
#[test]
fn test_ksort_krsort_does_not_mutate_aliased_copy() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1];
$copy = $a;
ksort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
foreach ($copy as $k => $v) { echo $k; }
echo "|";
$other = $a;
krsort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
foreach ($other as $k => $v) { echo $k; }
"#,
    );
    assert_eq!(out, "abc|bac|cba|abc");
}

/// Sorting must not disturb the hash's probe layout: key lookups, a later insert, and the
/// live count all still work on the reordered table.
#[test]
fn test_ksort_preserves_lookup_and_later_inserts() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1];
ksort($a);
echo $a["a"], $a["b"], $a["c"], ";";
$a["d"] = 4;
foreach ($a as $k => $v) { echo $k, $v; }
echo ";", count($a);
"#,
    );
    assert_eq!(out, "321;a3b2c1d4;4");
}

/// Repeated sorts in both directions must keep converging on the same orders instead of
/// corrupting the insertion-order chain after the first relink.
#[test]
fn test_repeated_key_and_value_sorts_stay_consistent() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1, "d" => 4, "e" => 5];
krsort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
ksort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
asort($a);
foreach ($a as $k => $v) { echo $k; }
"#,
    );
    assert_eq!(out, "edcba|abcde|cbade");
}

/// An input that is already in the requested order must come back unchanged, which also
/// exercises the backward scan's immediate-stop path.
#[test]
fn test_key_sorts_on_already_ordered_input() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
ksort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
$b = ["c" => 3, "b" => 2, "a" => 1];
krsort($b);
foreach ($b as $k => $v) { echo $k; }
"#,
    );
    assert_eq!(out, "abc|cba");
}

/// A large packed array exercises several bottom-up merge passes and the packed-to-hash
/// promotion used by descending key order. The first, second, and last keys pin the full
/// relink without making the assertion depend on timing.
#[test]
fn test_krsort_scales_to_large_reverse_key_order() {
    let out = compile_and_run(
        r#"<?php
$a = range(0, 2047);
krsort($a);
$keys = array_keys($a);
echo count($keys), ":", $keys[0], ":", $keys[1], ":", $keys[2047];
"#,
    );
    assert_eq!(out, "2048:2047:2046:0");
}

/// `ksort()` on an indexed array stays a no-op: its keys are the slot positions `0..n-1`,
/// which are already in ascending key order, and the values keep their slots.
#[test]
fn test_ksort_on_indexed_array_is_a_noop() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "0=3;1=1;2=2;");
}

/// `krsort()` promotes non-empty indexed storage, returns true, and preserves direct lookup
/// while exposing descending integer keys through iteration.
#[test]
fn test_krsort_on_indexed_array_returns_true_and_preserves_lookup() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
echo krsort($a) ? "true:" : "false:";
echo $a[0], ":";
foreach ($a as $key => $value) { echo $key, "=", $value, ";"; }
"#,
    );
    assert_eq!(out, "true:1:2=3;1=2;0=1;");
}

/// Keeps the promoted local's boxed owner alive when EIR subsequently releases its argument.
#[test]
fn test_krsort_promoted_local_keeps_boxed_owner_after_argument_cleanup() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2, 3];
echo krsort($a) ? "true:" : "false:";
echo $a[0], ":";
$copy = $a;
$a[0] = 99;
echo $copy[0], ":", $a[0], ":";
foreach ($copy as $key => $value) { echo $key, "=", $value, ";"; }
"#,
    );
    assert_eq!(out.stdout, "true:1:1:99:2=3;1=2;0=1;", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `krsort()` on a statically empty indexed array stays accepted, because an empty receiver
/// is trivially representable in either direction.
#[test]
fn test_krsort_on_empty_indexed_array_is_accepted() {
    let out = compile_and_run(
        r#"<?php
$a = [];
krsort($a);
echo count($a);
"#,
    );
    assert_eq!(out, "0");
}

/// Sorting must not acquire, persist or release anything: it only rewrites slot indices in
/// the chain. Running the string-keyed and string-valued fixtures under `--heap-debug`
/// pins that, including the copy-on-write split the sorters ask for.
#[test]
fn test_hash_sorts_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["bb" => "two", "aa" => "three", "cc" => "one"];
$b = $a;
ksort($a);
krsort($b);
asort($a);
arsort($b);
foreach ($a as $k => $v) { echo $k, $v; }
foreach ($b as $k => $v) { echo $k, $v; }
"#,
    );
    assert_eq!(
        out.stdout, "cconeaathreebbtwobbtwoaathreeccone",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}
