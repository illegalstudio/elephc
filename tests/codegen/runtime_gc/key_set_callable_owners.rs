//! Purpose:
//! Verifies key-set results own callable descriptors copied from indexed inputs.
//!
//! Called from:
//! - The runtime GC codegen suite on each executable supported target.
//!
//! Key details:
//! - Homogeneous closure literals use indexed storage before conversion to a temporary hash.
//! - Results are invoked after source retirement, then released under heap-debug checks.

use crate::support::*;

/// A difference result keeps its surviving closure after both the conversion and source retire.
#[test]
fn test_key_set_diff_retains_indexed_callable_result() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function diff_callables(int $n): int {
    $items = [
        function (int $value) use ($n): int { return $n + $value; },
        function (int $value) use ($n): int { return $n + $value + 100; }
    ];
    $result = array_diff_key($items, [0]);
    unset($items);
    $callback = $result[1];
    $value = $callback(7);
    unset($callback, $result);
    return $value;
}
$total = 0;
for ($n = 0; $n < 20; $n++) { $total += diff_callables($n); }
echo $total;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2330");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An intersection result owns its callable after the temporary hash and original array retire.
#[test]
fn test_key_set_intersect_retains_indexed_callable_result() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function intersect_callables(int $n): int {
    $items = [
        function (int $value) use ($n): int { return $n + $value; },
        function (int $value) use ($n): int { return $n + $value + 100; }
    ];
    $result = array_intersect_key($items, [1 => 0]);
    unset($items);
    $callback = $result[1];
    $value = $callback(7);
    unset($callback, $result);
    return $value;
}
$total = 0;
for ($n = 0; $n < 20; $n++) { $total += intersect_callables($n); }
echo $total;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2330");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Retaining an immutable first-class function descriptor remains safe for both key-set helpers.
#[test]
fn test_key_set_static_callable_descriptor_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function key_set_static(int $value): int { return $value + 7; }
$items = [key_set_static(...), key_set_static(...)];
$diff = array_diff_key($items, [0]);
$intersection = array_intersect_key($items, [1 => 0]);
unset($items);
$first = $diff[1];
$second = $intersection[1];
echo $first(7), ":", $second(7);
unset($first, $second, $diff, $intersection);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "14:14");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
