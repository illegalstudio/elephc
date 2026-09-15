//! Purpose:
//! Integration or regression tests for assigning to a local inside a region a flow guard narrowed.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.
//! - Every expectation here is the host PHP 8.5.10 output for the same fixture.

use super::*;

/// Issue #509: the `$x = f(); if ($x === false) { $x = []; }` fallback idiom.
///
/// A guard publishes its view by OVERWRITING the environment entry, so the store inside the
/// branch used to measure itself against `false` and fail with
/// `cannot reassign $g from false to array<never>`. The binding is `array<string>|false` and an
/// empty array is one of the things it holds, so there is nothing to reject.
///
/// The branch is dead here — `glob()` finds the file — which is exactly how the idiom is
/// normally written: the fallback exists for the failure the happy path never takes.
#[test]
fn test_glob_false_fallback_to_empty_array_compiles() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("a.meta", "hello-a");
$g = glob("*.meta");
if ($g === false) {
    $g = [];
}
echo count($g), "\n";
foreach ($g as $f) {
    echo "[", $f, "] ", gettype($f), "\n";
}
"#,
    );
    assert_eq!(out, "1\n[a.meta] string\n");
    let _ = fs::remove_dir_all(&dir);
}

/// The same idiom written the other way round, so the store lands in the region the COMPLEMENT
/// narrows rather than in the guard's own branch.
///
/// The complement is published separately from the then-branch's view
/// (`Checker::republish_flow_narrowing`), and is a guard fact in its own right however the
/// then-branch ended: the chain restores the pre-`if` type before checking the `else`.
#[test]
fn test_glob_false_fallback_on_the_else_side_compiles() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$g = glob("*.nothing-matches-this");
if ($g !== false) {
    echo "matched ", count($g), "\n";
} else {
    $g = [];
}
var_dump($g);
"#,
    );
    assert_eq!(out, "matched 0\narray(0) {\n}\n");
    let _ = fs::remove_dir_all(&dir);
}

/// The fallback branch actually RUNS, so the store goes through the union slot rather than
/// being compiled and never reached. The array is then grown and iterated to show the slot
/// really holds an array afterwards.
#[test]
fn test_taken_false_fallback_stores_through_the_union_slot() {
    let out = compile_and_run(
        r#"<?php
function maybe_list(bool $ok): array|false { return $ok ? ["a", "b"] : false; }
$m = maybe_list(false);
if ($m === false) {
    $m = [];
}
$m[] = "appended";
var_dump($m);
foreach ($m as $e) {
    echo "e=", $e, "\n";
}
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  string(8) \"appended\"\n}\ne=appended\n"
    );
}

/// Not specific to `false`: any guard narrows the same way, and the other member of the union
/// is as assignable inside the branch as it is outside one.
#[test]
fn test_narrowed_union_local_may_take_another_member_of_its_binding() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): int|string|float { return $n === 0 ? 1 : ($n === 1 ? "s" : 2.5); }
$p = pick(1);
if (is_int($p)) {
    $p = "was int";
} elseif (is_string($p)) {
    $p = 7;
}
var_dump($p);
"#,
    );
    assert_eq!(out, "int(7)\n");
}

/// Nested guards keep the OUTERMOST origin, which is the binding's own type. Measuring the
/// store against the enclosing guard's view instead would reject `"was float"` here, since the
/// inner branch narrowed `$q` to `float`.
#[test]
fn test_nested_guards_measure_the_store_against_the_binding() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): int|string|float { return $n === 0 ? 1 : ($n === 1 ? "s" : 2.5); }
$q = pick(2);
if (is_scalar($q)) {
    if (is_float($q)) {
        $q = "was float";
    }
}
var_dump($q);
"#,
    );
    assert_eq!(out, "string(9) \"was float\"\n");
}

/// A `while` condition is re-evaluated before every iteration, so it narrows its body the same
/// way an `if` does — and a store inside that body gets the same treatment.
#[test]
fn test_while_guard_region_accepts_a_store_the_binding_holds() {
    let out = compile_and_run(
        r#"<?php
function lines(): array { return ["a", "b"]; }
$pending = lines();
$w = array_shift($pending);
while ($w !== null) {
    if ($w === "b") {
        $w = "B";
    }
    echo "[", $w, "]\n";
    $w = array_shift($pending);
}
var_dump($w);
"#,
    );
    assert_eq!(out, "[a]\n[B]\nNULL\n");
}

/// A second store in the same region is measured against what the FIRST one left behind, not
/// against the binding: the guard stopped speaking for the name once the region bound it.
///
/// Both stores here fit the binding, so the region type-checks either way — what the test pins
/// is that the narrowing is not reapplied, which is what keeps
/// `$a = 1; if (is_string($a)) { $a = "x"; $a = 2; }` reaching the branch-divergent
/// `Mixed`-storage rule instead of being accepted outright
/// (`error_tests::type_system::test_guarded_region_shapes_still_error_under_strict`).
#[test]
fn test_a_second_store_in_the_region_follows_the_first() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): int|string { return $n === 0 ? 1 : "s"; }
$v = pick(1);
if (is_string($v)) {
    $v = "first";
    $v = "second";
}
var_dump($v);
"#,
    );
    assert_eq!(out, "string(6) \"second\"\n");
}
