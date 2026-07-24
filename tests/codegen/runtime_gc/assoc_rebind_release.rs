//! Purpose:
//! Regression tests for issue #595: rebinding an associative-array local to a
//! fresh literal inside a loop must release the previous hash (and every boxed
//! Mixed value it owns) so the heap stays clean across iterations.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`.
//! - Values are `$i`-dependent to defeat constant folding, so the associative
//!   literal is materialized with a genuine owned boxed-Mixed value every
//!   iteration (the shape that leaked: `hash_set` steals the value, but the
//!   value materializer wrongly retained producers such as `ichecked_add`).
//! - The double-free guards (escaped/passed/returned hashes) confirm the fix is
//!   ownership-gated: the retained boxed cell must not be freed twice.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Issue #595 repro: an associative literal whose value is a boxed `ichecked_add`
/// result, rebound every loop iteration. The previous hash (and its boxed Mixed
/// value) must be released on rebind — otherwise one block leaks per iteration.
#[test]
fn test_issue_595_assoc_rebind_int_add_releases_previous_hash() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $m = ["k" => $i + 1, "j" => "x"];
    $count = $count + $m["k"];
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "210\n");
}

/// Same shape with a boxed `ichecked_mul` value, covering the sibling checked
/// integer op that also produces a fresh owned Mixed box.
#[test]
fn test_assoc_rebind_int_mul_value_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $m = ["k" => $i * 2, "j" => "x"];
    $count = $count + $m["k"];
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "380\n");
}

/// Same shape with a boxed `ichecked_sub` value.
#[test]
fn test_assoc_rebind_int_sub_value_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $m = ["k" => $i - 1, "j" => "x"];
    $count = $count + $m["k"];
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "170\n");
}

/// Value produced by `mixed_numeric_binop` (a Mixed operand plus an int): another
/// op that returns a fresh owned boxed Mixed cell that `hash_set` steals.
#[test]
fn test_assoc_rebind_mixed_numeric_value_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$count = 0;
$acc = 0;
for ($i = 0; $i < 20; $i++) {
    $acc = $acc + 1;
    $m = ["k" => $acc + 0, "j" => "x"];
    $count = $count + $m["k"];
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "210\n");
}

/// Rebinding the associative local to an empty literal (`$m = []`) must also
/// release the previous hash and its owned boxed value.
#[test]
fn test_assoc_rebind_to_empty_literal_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $m = ["k" => $i + 1, "j" => "x"];
    $count = $count + $m["k"];
    $m = [];
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "210\n");
}

/// Control: the indexed sibling was already heap-clean (its `array_push` retains
/// and the owning temp is released). This guards against a regression in the
/// indexed path while fixing the associative one.
#[test]
fn test_assoc_rebind_indexed_control_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $m = [$i + 1, 5];
    $count = $count + $m[0];
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "210\n");
}

/// A dynamic string value inside a heterogeneous (Mixed-valued) hash: this path
/// persists+boxes the string and was already clean. Guards that the fix does not
/// disturb the string boxing path.
#[test]
fn test_assoc_rebind_string_value_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $m = ["k" => "v" . $i, "j" => 5];
    $count = $count + $m["j"];
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "100\n");
}

/// Double-free guard: the hash (with its owned boxed value) escapes into an outer
/// array before the local is rebound. The boxed cell is now owned by the outer
/// array; it must be released exactly once when the outer array is torn down.
#[test]
fn test_assoc_hash_escaped_into_outer_array_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$out = [];
for ($i = 0; $i < 5; $i++) {
    $m = ["k" => $i + 1, "j" => "x"];
    $out[] = $m;
}
echo count($out), "\n";
"#,
    );
    assert_clean(out, "5\n");
}

/// Double-free guard: the hash is passed to a function that reads a key, then the
/// local is rebound. The boxed value must survive the call and be released once.
#[test]
fn test_assoc_hash_passed_to_function_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function firstk($h) { return $h["k"]; }
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $m = ["k" => $i + 1, "j" => "x"];
    $count = $count + firstk($m);
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "210\n");
}

/// Double-free guard: the associative literal is built and returned by a callee,
/// then bound (and rebound) in the caller loop. Exercises the same lowering from
/// a function body while confirming a single release per rebind.
#[test]
fn test_assoc_hash_returned_from_function_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function mk($i) { return ["k" => $i + 1, "j" => "x"]; }
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $m = mk($i);
    $count = $count + $m["k"];
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "210\n");
}

/// A boxed-Mixed arithmetic value stored into a `Mixed` object property inside a
/// loop hits the same `value_can_own_mixed_box_source` classification. The
/// property store must steal the fresh box rather than re-retain it, so the
/// previous property value is released cleanly on each iteration.
#[test]
fn test_object_mixed_property_int_add_value_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public $v; }
$count = 0;
for ($i = 0; $i < 20; $i++) {
    $o = new C();
    $o->v = $i + 1;
    $count = $count + $o->v;
}
echo $count, "\n";
"#,
    );
    assert_clean(out, "210\n");
}

/// Issue #528 shape (indexed local grown then pushed into an outer array before a
/// `[]` rebind). Already clean on current main; this guards the shape against
/// regressions and documents that it shares the family but not the defect.
#[test]
fn test_issue_528_indexed_rebind_after_push_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [];
for ($i = 0; $i < 6; $i++) {
    $mid = [];
    for ($j = 0; $j < 3; $j++) { $mid[] = ['x', 'y']; }
    $a[] = $mid;
}
echo count($a) . "\n";
"#,
    );
    assert_clean(out, "6\n");
}
