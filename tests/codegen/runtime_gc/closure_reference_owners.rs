//! Purpose:
//! Verifies managed ownership of local cells captured by reference in closures.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Local bindings and escaping descriptors own independent leases on the same cell.

use crate::support::*;

/// Escaping closures preserve shared writable state and release it after the final descriptor.
#[test]
fn test_core_closure_reference_cells_survive_creator_and_shared_descriptors() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function counterPair(): array {
    $value = 0;
    $text = str_repeat("r", 3);
    return [
        function() use (&$value, &$text): void { $value++; echo $text, $value, "|"; },
        function() use (&$value): void { echo $value, "|"; },
    ];
}
$callbacks = counterPair();
$first = $callbacks[0];
$second = $callbacks[1];
unset($callbacks);
$first();
$second();
unset($first);
$second();
unset($second);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "rrr1|1|1|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Removing a captured binding does not destroy its object before the remaining closure retires.
#[test]
fn test_core_closure_reference_cells_release_objects_and_repeated_handlers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CapturedReferenceOwner {
    public function __destruct() { echo "drop|"; }
}
function captureReferenceObject(): callable {
    $value = new CapturedReferenceOwner();
    $callback = function() use (&$value): void { echo get_class($value), "|"; };
    unset($value);
    return $callback;
}
$callback = captureReferenceObject();
$callback();
unset($callback);
for ($i = 0; $i < 3; $i++) {
    $warnings = 0;
    set_error_handler(function(int $level, string $message) use (&$warnings): bool { $warnings++; return true; });
    trigger_error("message", E_USER_WARNING);
    restore_error_handler();
    echo $warnings, "|";
    unset($warnings);
}
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "CapturedReferenceOwner|drop|1|1|1|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Loop reinitialization replaces the payload of an already captured cell without leaking its old box.
#[test]
fn test_core_reference_capture_loop_reinitialization_retires_boxed_payloads() {
    let source = r#"<?php
function repeatCapturedCounter(): void {
    for ($i = 0; $i < 5; $i++) {
        $count = 0;
        $increment = function() use (&$count): void { $count++; };
        $increment();
        echo $count, "|";
        unset($increment);
    }
}
repeatCapturedCounter();
for ($i = 0; $i < 5; $i++) {
    $count = 0;
    $increment = function() use (&$count): void { $count++; };
    $increment();
    echo $count, "|";
    unset($increment);
}
unset($count);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1|".repeat(10), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "1|".repeat(10));
}

/// Existing descriptors observe the replacement string through their shared cell on later iterations.
#[test]
fn test_core_reference_capture_loop_reinitialization_keeps_live_aliases() {
    let source = r#"<?php
$saved = static fn(): string => "";
for ($i = 0; $i < 4; $i++) {
    $text = str_repeat("x", $i + 1);
    $read = function() use (&$text): string { return $text; };
    if ($i === 0) { $saved = $read; }
    echo $read(), ":", $saved(), "|";
    unset($read);
}
unset($text, $saved);
"#;
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "{}\n{assembly}", out.stderr);
    assert_eq!(out.stdout, "x:x|xx:xx|xxx:xxx|xxxx:xxxx|", "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), out.stdout);
}

/// Unsetting and probing a captured string keeps old aliases independent of a newly captured binding.
#[test]
fn test_core_reference_capture_unset_rebind_preserves_old_and_new_strings() {
    let source = r#"<?php
function detachedStringCaptures(int $seed): void {
    $text = str_repeat("x", $seed);
    $old = function() use (&$text): string { return $text; };
    unset($text);
    echo isset($text) ? "bad|" : "unset|";
    echo $old(), "|";
    $text = "new" . $seed;
    $fresh = function() use (&$text): string { return $text; };
    echo $old(), ":", $fresh(), "|";
    unset($text);
    echo isset($text) ? "bad|" : "unset|";
    echo $old(), ":", $fresh(), "|";
    unset($old, $fresh);
}
detachedStringCaptures($argc);
detachedStringCaptures($argc + 1);
"#;
    let expected = "unset|x|x:new1|unset|x:new1|unset|xx|xx:new2|unset|xx:new2|";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "{}\n{assembly}", out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}
