//! Purpose:
//! Covers temporary source strings and discarded results at native-to-eval boundaries.
//!
//! Called from:
//! - The runtime GC codegen integration suite on all executable targets.
//!
//! Key details:
//! - Runtime-unknown source keeps execution in Magician instead of literal AOT lowering.
//! - Heap assertions include normal returns, string results and exception escape.

use crate::support::*;

/// Repeated eval calls retire their source conversions and both null and string results.
#[test]
fn test_core_opaque_eval_discarded_results_and_source_buffers_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function discardOwnedEvalResult(string $source): void { eval($source); }
$source = 'return null; // ' . $argc;
for ($i = 0; $i < 5; $i++) { discardOwnedEvalResult($source); }
$source = 'return str_repeat("x", 48); // ' . $argc;
for ($i = 0; $i < 5; $i++) { discardOwnedEvalResult($source); }
unset($source);
echo "ok";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ok", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Source-buffer roots survive a throwing eval and retire when its native caller unwinds.
#[test]
fn test_core_opaque_eval_source_buffers_are_released_on_exception_escape() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function throwOwnedEvalSource(string $source): void { eval($source); }
$source = 'throw new RuntimeException("stop"); // ' . $argc;
$caught = 0;
for ($i = 0; $i < 5; $i++) {
    try { throwOwnedEvalSource($source); }
    catch (RuntimeException $error) { $caught++; unset($error); }
}
unset($source);
echo $caught;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "5", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
