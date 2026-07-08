//! Purpose:
//! Integration tests for memory-model-aware constant propagation: array-literal
//! facts, targeted call invalidation, and every reference-exposure hazard
//! (by-ref params, by-ref captures, by-ref foreach, `global`) compiled and run
//! end-to-end so an unsound fold or missed invalidation shows as a wrong output.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs are PHP-cross-checked (`php -r`); fixtures pair a
//!   fold-eligible read with a hazard that must block or survive it.

use super::*;

/// Array assignment is a COW snapshot: writes through the copy leave the
/// original untouched, and the original's folded reads stay correct.
#[test]
fn test_array_fact_cow_copy_divergence() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$b = $a;
$b[0] = 9;
echo $a[0], ",", $b[0];
"#,
    );
    assert_eq!(out, "1,9");
}

/// An element write after a folded read must not fold the later read.
#[test]
fn test_array_fact_element_write_ordering() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
echo $a[1];
$a[1] = 9;
echo $a[1];
"#,
    );
    assert_eq!(out, "29");
}

/// `sort($a)` mutates by reference: reads after the call see the sorted array.
#[test]
fn test_sort_invalidates_array_fact() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
sort($a);
echo $a[0], $a[1], $a[2];
"#,
    );
    assert_eq!(out, "123");
}

/// A user function's by-ref parameter mutates the caller local.
#[test]
fn test_user_by_ref_param_mutation_observed() {
    let out = compile_and_run(
        r#"<?php
function bump(int &$n): void {
    $n = $n + 7;
}
$v = 5;
bump($v);
echo $v + 1;
"#,
    );
    assert_eq!(out, "13");
}

/// A `global`-writing callee invoked from top level rewrites the top-level
/// local between the assignment and the echo.
#[test]
fn test_global_writing_callee_observed_at_top_level() {
    let out = compile_and_run(
        r#"<?php
function rewrite(): void {
    global $x;
    $x = 9;
}
$x = 5;
rewrite();
echo $x + 1;
"#,
    );
    assert_eq!(out, "10");
}

/// The by-ref foreach value var writes through to the array — including after
/// the loop, where it still aliases the last element.
#[test]
fn test_foreach_by_ref_post_loop_alias_observed() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
    $v = $v * 2;
}
$v = 99;
echo $a[0], ",", $a[1], ",", $a[2];
"#,
    );
    assert_eq!(out, "2,4,99");
}

/// A by-ref closure capture rewrites the outer local when invoked.
#[test]
fn test_closure_by_ref_capture_mutation_observed() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
$bump = function () use (&$x): void {
    $x = $x + 1;
};
$bump();
echo $x;
"#,
    );
    assert_eq!(out, "6");
}

/// A pure user call between an assignment and its read changes nothing — the
/// read may fold, and the output must be identical either way.
#[test]
fn test_pure_user_call_between_assign_and_read() {
    let out = compile_and_run(
        r#"<?php
function pf(int $a): int {
    return $a + 1;
}
$x = 5;
pf(1);
echo $x + 1;
"#,
    );
    assert_eq!(out, "6");
}

/// `call_user_func` forwards the caller's variable to a by-ref parameter; the
/// mutation must be visible in the read after the call. (elephc preserves
/// by-ref through `call_user_func` — locked by the descriptor-invoker suite —
/// where PHP 8 would coerce to by-value with a warning; the propagation must
/// match elephc's own runtime.)
#[test]
fn test_callback_builtin_by_ref_forwarding_observed() {
    let out = compile_and_run(
        r#"<?php
function add3(int &$n): void {
    $n = $n + 3;
}
$v = 5;
call_user_func('add3', $v);
echo $v;
"#,
    );
    assert_eq!(out, "8");
}

/// `unset($a[0])` removes one element; unrelated scalars keep folding and the
/// removed offset reads as absent afterwards.
#[test]
fn test_unset_array_element_targeted() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
$a = ["k" => 1, "j" => 2];
unset($a["k"]);
echo $x + 1, ",", isset($a["k"]) ? "y" : "n", ",", $a["j"];
"#,
    );
    assert_eq!(out, "6,n,2");
}
