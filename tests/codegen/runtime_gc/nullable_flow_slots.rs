//! Purpose:
//! Checks ownership when flow-sensitive bindings change type or gain boxed storage.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Loop-entry conversion, arithmetic stores and catch binding preserve every owner.

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

/// A replacement catch value survives the displaced value's throwing destructor.
#[test]
fn test_nullable_flow_catch_replacement_keeps_owners_on_throw() {
    let source = r#"<?php
class CatchSlotD { public function __destruct() { throw new Exception('d'); } }
function catchSlot(int $flag): void {
    $e = new CatchSlotD();
    try {
        try { throw new Exception('a'); } catch (Exception $e) {}
        if ($flag < 0) { throw new LogicException('keep'); }
    } catch (Exception $c) { echo $e->getMessage(), ':', $c->getMessage(), "\n"; }
}
catchSlot($argc);
"#;
    for optimized in [true, false] {
        let run = || {
            let out = compile_and_run_with_heap_debug(source);
            assert!(out.success, "{}", out.stderr);
            assert_eq!(out.stdout, "a:d\n");
            assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        };
        if optimized { run(); } else { without_ir_opt(run); }
    }
}
