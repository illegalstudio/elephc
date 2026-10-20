//! Purpose:
//! Regression tests for array_unshift through local references and shared packed arrays.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` through the native codegen harness.
//!
//! Key details:
//! - Consuming COW helpers and reference-cell publication must retire separate owners.
//! - Copies remain readable after splitting, growth, and multiple prepended values.

use crate::support::compile_and_run_with_heap_debug;

/// A local reference must not retire the old array still owned by a value copy twice.
#[test]
fn test_array_unshift_reference_cow_preserves_copy() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [1, 2]; $ref = &$items; $copy = $items;
array_unshift($ref, 3);
echo implode(",", $items), "|", implode(",", $copy), "\n";
unset($ref, $items);
echo implode(",", $copy), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3,1,2|1,2\n1,2\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Unique, shared, and value-less reference calls balance ownership even when growth relocates.
#[test]
fn test_array_unshift_reference_cow_growth_and_no_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [1, 2, 3, 4]; $ref = &$items;
echo array_unshift($ref), "|";
echo array_unshift($ref, 8, 9), "|", implode(",", $items), "\n";
$copy = $items;
array_unshift($ref, 6, 7);
echo implode(",", $items), "|", implode(",", $copy), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "4|6|8,9,1,2,3,4\n6,7,8,9,1,2,3,4|8,9,1,2,3,4\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}
