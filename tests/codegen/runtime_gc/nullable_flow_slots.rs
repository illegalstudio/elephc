//! Purpose:
//! Checks ownership when nullable value bindings gain boxed storage after coalescing.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Loop-entry conversion and arithmetic stores must retire the previous boxed cell.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

/// Widened nullable counters and accumulators release every owner in both IR modes.
#[test]
fn test_nullable_flow_parameter_widening_releases_initial_cells() {
    let source = r#"<?php
function counter(?int $value): string {
    $out = '';
    for ($value = $value ?? 0; $value < 3; $value++) { $out .= $value; }
    return $out;
}
function accumulator(?int $value): int {
    $value ??= 0;
    for ($i = 0; $i < 3; $i++) { $value = $value + $i; }
    return $value;
}
function localCounter(bool $empty): string {
    $value = $empty ? null : 1;
    $out = '';
    for ($value = $value ?? 0; $value < 3; $value++) { $out .= $value; }
    return $out;
}
function localAccumulator(bool $empty): int {
    $value = $empty ? null : 10;
    $value ??= 0;
    for ($i = 0; $i < 3; $i++) { $value = $value + $i; }
    return $value;
}
echo counter(null), '|', counter(1), '|', accumulator(null), '|', accumulator(10), "\n";
echo localCounter(true), '|', localCounter(false), '|', localAccumulator(true), '|', localAccumulator(false), "\n";
"#;
    for optimized in [true, false] {
        let run = || {
            let out = compile_and_run_with_heap_debug(source);
            assert!(out.success, "{}", out.stderr);
            assert_eq!(out.stdout, "012|12|3|13\n012|12|3|13\n");
            assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        };
        if optimized { run(); } else { without_ir_opt(run); }
    }
}
