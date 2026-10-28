//! Purpose:
//! Preserves PHP comparisons when nested numeric regions acquire independent counter slots.
//!
//! Called from:
//! - The focused optimizer codegen integration suite.
//!
//! Key details:
//! - Both IR modes validate heaps; PHP-compatible cases also use the optional real oracle.

use super::*;
use super::counter_fast_paths::equivalent;

/// Separate object and ordering fixtures cover each storage-proof gate in nested and simple loops.
#[test]
fn test_counter_region_comparison_php_semantics() {
    equivalent(r#"<?php
function regionObject(int $n, mixed $other): void {
    $equal = 0; $different = 0;
    for ($outer = 0; $outer < $n; $outer++) {
        for ($i = $n - 1; $i >= 0; $i--) {}
        for ($i = 0; $i < $n; $i++) {
            if ($i == $other) { $equal++; }
            if ($i != $other) { $different++; }
        }
    }
    echo $equal, ':', $different, '|';
}
function simpleObject(int $n, mixed $other): void {
    $equal = 0; $different = 0;
    for ($i = 0; $i < $n; $i++) {
        if ($i == $other) { $equal++; }
        if ($i != $other) { $different++; }
    }
    echo $equal, ':', $different, '|';
}
function regionOrdering(int $n): void {
    $truth = 0; $nil = 0;
    for ($outer = 0; $outer < $n; $outer++) {
        for ($i = $n - 1; $i >= 0; $i--) {}
        for ($i = -2; $i < $n; $i++) { $truth += ($i <=> true); $nil += ($i <=> null); }
    }
    echo $truth, ':', $nil, '|';
}
function simpleOrdering(int $n): void {
    $truth = 0; $nil = 0;
    for ($i = -2; $i < $n; $i++) { $truth += ($i <=> true); $nil += ($i <=> null); }
    echo $truth, ':', $nil, '|';
}
$object = new stdClass;
regionObject($argc + 2, $object); regionObject(0, $object); simpleObject($argc + 2, $object);
regionOrdering($argc + 2); regionOrdering(0); simpleOrdering($argc + 2);
"#);
}

/// Pointer identity retains generic boxing in nested and simple counters, without a PHP oracle.
#[test]
fn test_counter_region_comparison_pointer_identity() {
    let source = r#"<?php
function regionPointer(int $n): void {
    $p = ptr_null(); $equal = 0; $different = 0;
    for ($outer = 0; $outer < $n; $outer++) {
        for ($i = $n - 1; $i >= 0; $i--) {}
        for ($i = 0; $i < $n; $i++) {
            if ($i === $p) { $equal++; }
            if ($i !== $p) { $different++; }
        }
    }
    echo $equal, ':', $different, '|';
}
function simplePointer(int $n): void {
    $p = ptr_null(); $equal = 0; $different = 0;
    for ($i = 0; $i < $n; $i++) {
        if ($i === $p) { $equal++; }
        if ($i !== $p) { $different++; }
    }
    echo $equal, ':', $different, '|';
}
regionPointer($argc + 2); regionPointer(0); simplePointer($argc + 2);
"#;
    for optimized in [false, true] {
        let output = if optimized { compile_and_run_with_heap_debug_tagged(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug_tagged(source)) };
        assert_eq!(output.stdout, "3:6|0:0|1:2|", "optimized={optimized}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "optimized={optimized}: {}", output.stderr);
    }
}
