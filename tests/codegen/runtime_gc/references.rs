//! Purpose:
//! Integration/regression tests for the garbage-collection behaviour of reference assignment into
//! storage (`$arr[$k] =& $v`): the heap-kind-6 reference cell must be reclaimed exactly once when
//! its owning container is released, with no leak, double-free, or use-after-free.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The reference cell is owned by the container (the array's hash) and freed once through
//!   `__rt_hash_free_deep`'s tag-11 dispatch when the container is released; the aliased source
//!   variable borrows the cell. High-iteration loops would crash — heap exhaustion on a leak,
//!   corruption on a double-free — if that discipline were wrong, so they double as leak/UAF probes.
//! - Fixtures use a non-empty heterogeneous assoc base (no empty-array→hash promotion, no `stdClass`)
//!   so the measured behaviour isolates the reference cell from unrelated pre-existing container
//!   teardown gaps.

use crate::support::*;

/// Verifies a reference into an associative-array element survives a tight 100k-iteration loop with
/// no heap exhaustion (leak) or corruption (double-free): each call promotes the source to a
/// reference cell, writes through it, and the cell is reclaimed when the function-local array is
/// released on return. The final returned value confirms the write-through still works after churn.
#[test]
fn test_gc_reference_into_assoc_element_survives_tight_loop() {
    let out = compile_and_run(
        r#"<?php
function f() {
    $a = ['x' => 0, 'n' => 's'];
    $v = 1;
    $a['x'] =& $v;
    $v = 5;
    $a['x'] = 7;
    return $a['x'];
}
for ($i = 0; $i < 100000; $i++) { $r = f(); }
echo $r;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies two distinct references into one array (aliasing two distinct source variables) are each
/// reclaimed exactly once across a 100k-iteration loop, with the two cells staying independent. A
/// double-free or cross-aliasing of the cells would corrupt the heap and crash within the loop.
#[test]
fn test_gc_two_references_in_one_array_survive_loop() {
    let out = compile_and_run(
        r#"<?php
function f() {
    $a = ['p' => 0, 'q' => 0, 'n' => 's'];
    $x = 1;
    $y = 2;
    $a['p'] =& $x;
    $a['q'] =& $y;
    $x = 10;
    $y = 20;
    return $a['p'] + $a['q'];
}
for ($i = 0; $i < 100000; $i++) { $r = f(); }
echo $r;
"#,
    );
    assert_eq!(out, "30");
}

/// Verifies the source variable still reads the shared value after its container is `unset`, across
/// a 100k-iteration loop: releasing the container frees its owner of the cell, and the aliased
/// source must keep observing the value with no use-after-free or crash. Matches `php -r` `1`.
#[test]
fn test_gc_reference_source_survives_container_unset_loop() {
    let out = compile_and_run(
        r#"<?php
function f() {
    $a = ['x' => 0, 'n' => 's'];
    $v = 1;
    $a['x'] =& $v;
    unset($a);
    return $v;
}
for ($i = 0; $i < 100000; $i++) { $r = f(); }
echo $r;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies that copying an array holding a reference element and mutating the copy writes through
/// the shared reference cell (PHP copy-on-write preserves reference entries) without freeing the
/// cell prematurely. Matches `php -r` output `9|9`.
#[test]
fn test_gc_reference_cow_copy_shares_value() {
    let out = compile_and_run(
        r#"<?php
$a = ['x' => 0, 'n' => 's'];
$v = 5;
$a['x'] =& $v;
$b = $a;
$b['x'] = 9;
echo $v, '|', $a['x'];
"#,
    );
    assert_eq!(out, "9|9");
}
