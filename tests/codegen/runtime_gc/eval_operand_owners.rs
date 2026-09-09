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

/// Top-level eval replaces initial process-global boxes without leaking them or stealing local owners.
#[test]
fn test_core_eval_top_level_process_globals_retire_initial_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = 'echo $argc > 0 && count($argv) === $argc ? "ready|" : "bad|"; // ' . $argc;
eval($source);
eval($source);
unset($source);
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ready|ready|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Changed globals retire old owners while unchanged reloads preserve the existing value.
#[test]
fn test_core_eval_global_reload_balances_replacements_and_identical_cells() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function seedReloadGlobal(): void { global $payload; $payload = str_repeat("a", 48); }
function runReloadGlobal(string $source): void { eval($source); }
seedReloadGlobal();
$source = 'global $payload; echo strlen($payload), "|"; $payload = str_repeat("b", 64); // ' . $argc;
runReloadGlobal($source);
runReloadGlobal($source);
$source = 'global $payload; echo strlen($payload), "|"; // ' . $argc;
runReloadGlobal($source);
runReloadGlobal($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "48|64|64|64|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native and opaque-eval globals share boxed process arguments and release their implicit owners.
#[test]
fn test_core_eval_process_argument_globals_have_balanced_ownership() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function inspectNativeProcessGlobals(): void {
    global $argc, $argv;
    echo $argc > 0 && count($argv) === $argc ? "native|" : "bad|";
}
function inspectEvalProcessGlobals(string $source): void { eval($source); }
inspectNativeProcessGlobals();
$source = 'global $argc, $argv; echo $argc > 0 && count($argv) === $argc ? "eval|" : "bad|"; // ' . $argc;
inspectEvalProcessGlobals($source);
unset($source);
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "native|eval|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A native argv snapshot uses managed copy-on-write storage and retires at process exit.
#[test]
fn test_core_native_argv_array_snapshot_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$before = $argv[0];
$copy = $argv;
$copy[0] = "changed";
echo $argv[0] === $before && $copy[0] === "changed" ? "isolated" : "bad";
unset($before, $copy);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "isolated", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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
