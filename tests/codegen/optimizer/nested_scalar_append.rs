//! Purpose:
//! Verifies synthetic nested appends keep their writable root through constant propagation.
//!
//! Called from:
//! - The optimizer codegen integration suite.
//!
//! Key details:
//! - Literal scalar roots must still evaluate computed keys and the RHS before their Error.
//! - A right-hand side replacing the scalar by an array must select the updated storage.

use crate::support::compile_and_run_with_heap_debug;

/// Constant propagation cannot turn the parser's internal write-context read into a scalar read.
#[test]
fn test_nested_write_scalar_root_append_after_propagation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function append_key(): string { echo 'key|'; return 'outer'; }
function append_rhs(): int { echo 'rhs|'; return 9; }
function rejected_append(): void {
    $root = 42;
    try { $root[append_key()][] = append_rhs(); }
    catch (Error $e) { echo $e->getMessage(); unset($e); }
}
rejected_append();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "key|rhs|Cannot use a scalar value as an array");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// The append selects the live receiver after the RHS replaces a scalar by an array.
#[test]
fn test_nested_write_scalar_root_replaced_by_rhs_array() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function retype_key(): string { echo 'key|'; return 'outer'; }
function retype_rhs(): array { echo 'rhs|'; return ['outer' => []]; }
function retype_control(mixed $root): void {
    $root[retype_key()][] = ($root = retype_rhs()) ? 9 : 0;
    echo json_encode($root);
}
retype_control(42);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "key|rhs|{\"outer\":[9]}");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}
