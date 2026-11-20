//! Purpose:
//! Regression tests for array_pop, array_shift and array_splice through local references
//! while a value copy still shares the array.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` through the native codegen harness.
//!
//! Key details:
//! - Consuming COW helpers and reference-cell publication must retire separate owners, as
//!   `unshift_reference_cow` pins for array_unshift.
//! - Copies stay readable after the split, through two-hop aliases, and when taken before the
//!   reference.

use crate::support::compile_and_run_with_heap_debug;

/// A copy taken after the reference keeps the original array.
#[test]
fn test_pop_shift_splice_reference_cow_preserves_copy() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$a = [1, 2, 3]; $ra = &$a; $ca = $a;
echo array_pop($ra), "|", implode(",", $a), "|", implode(",", $ca), "\n";
$b = [1, 2, 3]; $rb = &$b; $cb = $b;
echo array_shift($rb), "|", implode(",", $b), "|", implode(",", $cb), "\n";
$c = [1, 2, 3]; $rc = &$c; $cc = $c;
array_splice($rc, 1, 1, [7, 8]);
echo implode(",", $c), "|", implode(",", $cc), "\n";
unset($ra, $a, $rb, $b, $rc, $c);
echo implode(",", $ca), "|", implode(",", $cb), "|", implode(",", $cc), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3|1,2|1,2,3\n1|2,3|1,2,3\n1,7,8,3|1,2,3\n1,2,3|1,2,3|1,2,3\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Two-hop aliases and a copy taken before the reference balance ownership too.
#[test]
fn test_pop_shift_splice_reference_cow_aliases_and_early_copy() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$a = [1, 2, 3]; $r1 = &$a; $r2 = &$r1; $ca = $a;
echo array_pop($r2), "|", implode(",", $a), "|", implode(",", $ca), "\n";
$b = [1, 2, 3]; $cb = $b; $rb = &$b;
echo array_shift($rb), "|", implode(",", $b), "|", implode(",", $cb), "\n";
$c = [1, 2, 3]; $cc = $c; $rc = &$c;
array_splice($rc, 0, 2);
echo implode(",", $c), "|", implode(",", $cc), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3|1,2|1,2,3\n1|2,3|1,2,3\n3|1,2,3\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Popping and shifting an empty referenced array leaves a clean heap.
#[test]
fn test_pop_shift_reference_cow_empty_arrays() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$a = [1]; array_pop($a); $ra = &$a;
var_dump(array_pop($ra));
$b = [1]; array_shift($b); $rb = &$b; $cb = $b;
var_dump(array_shift($rb));
echo count($b), "|", count($cb), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "NULL\nNULL\n0|0\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}
