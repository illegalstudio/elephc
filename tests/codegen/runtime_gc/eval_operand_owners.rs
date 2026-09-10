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

/// Positional strlen releases extracted strings and agrees with named and callable argument owners.
#[test]
fn test_core_eval_strlen_releases_extracted_and_callable_string_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function evalStringLengthOwners(string $source): void { eval($source); }
$source = 'echo strlen(["key" => str_repeat("x", 48)]["key"]), ":";
echo strlen(string: str_repeat("x", 48)), ":";
echo call_user_func("strlen", str_repeat("x", 48)), ":";
$length = strlen(...);
echo $length(str_repeat("x", 48)), "|"; // ' . $argc;
for ($i = 0; $i < 3; $i++) { evalStringLengthOwners($source); }
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "48:48:48:48|".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested eval owns copied source bytes independently of temporaries or source-variable replacement.
#[test]
fn test_core_nested_eval_releases_source_operands_before_scope_mutation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function nestedEvalSourceOwners(string $source): void { eval($source); }
$source = 'echo eval("return 17;"), ":";
$nested = "return 23;";
echo eval($nested), ":";
$nested = "\$nested = \"\"; return 29;";
echo eval($nested), "|"; // ' . $argc;
for ($i = 0; $i < 3; $i++) { nestedEvalSourceOwners($source); }
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "17:23:29|".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Returning an eval-local cell across a native function boundary outlives eval scope teardown.
#[test]
fn test_core_eval_returned_local_survives_native_scope_teardown() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function returnEvalLocalOwner(string $source): mixed { return eval($source); }
$source = '$value = str_repeat("x", 48); return $value; // ' . $argc;
for ($i = 0; $i < 3; $i++) {
    $result = returnEvalLocalOwner($source);
    echo strlen($result), "|";
    unset($result);
}
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "48|48|48|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Immediate array indexing releases temporary containers while keeping extracted payloads alive.
#[test]
fn test_core_eval_array_read_releases_temporary_container_and_index() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function evalArrayReadOwners(string $source): void { eval($source); }
$source = 'echo strlen(["key" => str_repeat("x", 48)]["key"]), ":";
echo gc_status()["protected"] ? "protected" : "ready", "|";
$items = [10, 20];
echo $items[eval("\$items = []; return 1;")], "|"; // ' . $argc;
for ($i = 0; $i < 3; $i++) { evalArrayReadOwners($source); }
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "48:ready|20|".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A finally unset cannot invalidate the eval return consumed after native scope teardown.
#[test]
fn test_core_eval_returned_local_survives_finally_unset() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function returnEvalFinallyOwner(string $source): mixed { return eval($source); }
$source = '$value = str_repeat("x", 48);
try { try { return $value; } finally { unset($value); } } finally {} // ' . $argc;
for ($i = 0; $i < 3; $i++) {
    $result = returnEvalFinallyOwner($source);
    echo strlen($result), "|";
    unset($result);
}
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "48|48|48|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native Mixed reference stores adopt acquired strings instead of persisting a second copy.
#[test]
fn test_core_native_mixed_reference_stores_transfer_acquired_strings() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function replaceMixedReferenceOwner(mixed &$value): void {
    $value = "replaced";
    $value = $value;
}
function initialMixedReferenceOwner(): mixed { return null; }
$value = initialMixedReferenceOwner();
for ($i = 0; $i < 3; $i++) {
    replaceMixedReferenceOwner($value);
    echo $value, "|";
}
unset($value);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "replaced|".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval replacements, unchanged snapshots and missing entries balance native parameter owners.
#[test]
fn test_core_eval_local_reload_balances_replaced_and_missing_parameters() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function reloadLocalOwners(string $source, mixed $value, string $text): void {
    eval($source);
    echo strlen($value), ":", strlen($text), "|";
    eval($source);
    echo strlen($value), ":", strlen($text), "|";
}
$value = str_repeat("a", 8);
$text = str_repeat("b", 12);
$source = '$value = str_repeat("v", 48); $text = str_repeat("t", 64); // ' . $argc;
reloadLocalOwners($source, $value, $text);
$source = '$value = $value; $text = $text; // ' . $argc;
reloadLocalOwners($source, $value, $text);
$source = 'unset($value, $text); // ' . $argc;
reloadLocalOwners($source, $value, $text);
echo strlen($value), ":", strlen($text);
unset($source, $value, $text);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "48:64|48:64|8:12|8:12|0:0|0:0|8:12", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Literal and opaque eval share the active array parameter shadow without exposing its internal name.
#[test]
fn test_core_eval_array_parameter_shadow_is_the_visible_php_binding() {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
function inspectArrayShadowOwners(array $items, string $source): void {
    $items[0] = "native";
    eval($source);
    echo $items[0], "|";
    eval('$items[0] = "literal";');
    echo $items[0], "|";
}
$items = ["caller"];
$source = 'echo $items[0], "|";
$items[0] = "dynamic";
$locals = get_defined_vars();
echo array_key_exists("items#cow", $locals) ? "exposed|" : "hidden|";
unset($locals); // ' . $argc;
inspectArrayShadowOwners($items, $source);
echo $items[0];
unset($items, $source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "native|hidden|dynamic|literal|caller", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nuser assembly:\n{}", out.stderr, assembly);
}

/// Independent scope snapshots protect aliased native reference cells during sequential reload.
#[test]
fn test_core_eval_local_reload_keeps_aliased_reference_payloads_alive() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function reloadAliasedOwners(mixed &$first, mixed &$second, string $source): void {
    eval($source);
    echo strlen($first), ":", strlen($second), "|";
}
function initialAliasedOwner(): mixed { return str_repeat("a", 8); }
$value = initialAliasedOwner();
$source = '$first = str_repeat("b", 48); $second = $first; // ' . $argc;
reloadAliasedOwners($value, $value, $source);
reloadAliasedOwners($value, $value, $source);
echo strlen($value);
unset($source, $value);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "48:48|48:48|48", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// First compound writes use the OS count and retain it across later expression assignments.
#[test]
fn test_core_native_process_argument_first_writes_preserve_initial_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$argc += 7;
echo $argc === count($argv) + 7 ? "first|" : "bad|";
$updated = ($argc += 3);
echo $updated === count($argv) + 10 ? "expression|" : "bad|";
++$argc;
echo $argc === count($argv) + 11 ? "increment" : "bad";
unset($updated);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "first|expression|increment", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Global string stores transfer their acquired copy, including self-assignment and replacement.
#[test]
fn test_core_native_global_string_stores_transfer_persisted_payloads() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$payload = "";
function replaceOwnedGlobalString(): void {
    global $payload;
    $payload = str_repeat("a", 48);
    $payload = $payload;
    echo strlen($payload), "|";
    $payload = "done";
}
replaceOwnedGlobalString();
replaceOwnedGlobalString();
echo $payload;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "48|48|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Repeated native dispatch retires omitted defaults and newly coerced argument cells.
#[test]
fn test_core_eval_native_function_defaults_and_coercions_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function nativeBinderDefaults(int $number, string $text = "default-value"): int {
    return $number + strlen($text);
}
$source = 'for ($i = 0; $i < 3; $i++) { echo nativeBinderDefaults("2"), "|"; } // ' . $argc;
eval($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "15|15|15|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Raw and boxed reference arguments release their invoker markers after publishing changes.
#[test]
fn test_core_eval_native_function_reference_markers_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function nativeBinderReference(string &$text): void { $text .= "!"; }
function nativeBinderMixedReference(mixed &$value): void { $value = "replaced"; }
$source = '
for ($i = 0; $i < 3; $i++) {
    $text = str_repeat("x", 8);
    nativeBinderReference($text);
    echo strlen($text), ":";
    nativeBinderMixedReference($text);
    echo $text, "|";
    unset($text);
} // ' . $argc;
eval($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9:replaced|9:replaced|9:replaced|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Top-level eval replaces initial process-global boxes without leaking them or stealing local owners.
#[test]
fn test_core_eval_top_level_process_globals_retire_initial_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$argc += 7;
$source = 'echo count($argv) + 7 === $argc ? "ready|" : "bad|"; // ' . $argc;
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
$payload = "";
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
