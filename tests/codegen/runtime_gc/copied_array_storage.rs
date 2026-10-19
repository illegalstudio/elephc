//! Purpose:
//! Ownership and payload-lifetime regressions for copied indexed arrays promoted to hashes.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - A converted assignment consumes only its acquired source owner.
//! - Empty sibling copies stay independent while inserted object payloads survive allocation pressure.

/// Copied array storage preserves PHP keys, values and independent ownership.
#[test]
fn test_copied_array_storage_owned_payloads() {
    let out = crate::support::compile_and_run_with_heap_debug(r#"<?php
class Payload { public function __construct(public int $id) {} }
function run_case(): void {
    $sum = 0;
    for ($i = 0; $i < 40; $i++) {
        $source = [];
        $copy = $source;
        $sibling = $source;
        $copy[-5] = new Payload($i);
        $copy[] = new Payload($i + 1);
        $source[] = new Payload(100);
        $sibling[] = new Payload(200);
        $pressure = str_repeat("x", 200);
        $sum += $copy[-5]->id + $copy[-4]->id + $source[0]->id + $sibling[0]->id;
        unset($copy, $source, $sibling, $pressure);
    }
    echo $sum, "\n";
}
run_case();
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "13600\n");
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
