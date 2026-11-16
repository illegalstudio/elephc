//! Purpose:
//! Verifies managed ownership of local cells captured by reference in closures.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Local bindings and escaping descriptors own independent leases on the same cell.

use crate::support::*;

/// Mutating captures in alternative branches each own their cell and retire both object versions.
#[test]
fn test_alternative_reference_captures_release_cell_and_object_owners() {
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(r#"<?php
class AlternativeReferenceOwner {
    public static int $destroyed = 0;
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
    public function __destruct() { self::$destroyed++; }
}
function alternativeReferenceOwners(int $mode): void {
    $item = new AlternativeReferenceOwner('old'); $copy = $item;
    if ($mode === 0) {
        $change = function() use (&$item): int { $item = new AlternativeReferenceOwner('left'); return 1; };
        echo $change();
    } elseif ($mode === 1) {
        $change = function() use (&$item): int { $item = new AlternativeReferenceOwner('right'); return 2; };
        echo $change();
    } else { echo '0'; }
    echo ':', $copy->name, ':', $item->name, '|';
    unset($copy, $item, $change);
}
alternativeReferenceOwners($argc - 1);
echo AlternativeReferenceOwner::$destroyed;
"#, &[&[], &["right"], &["raw", "raw"]]);
    for (mode, (out, expected)) in runs.iter().zip(["1:old:left|2", "2:old:right|2", "0:old:old|1"]).enumerate() {
        assert!(out.success && out.stdout == expected && out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "mode={mode} stdout={:?} expected={expected:?}\n{}\n{}", out.stdout, out.stderr,
            selected_function_assembly(&assembly, &[elephc::names::function_symbol("alternativeReferenceOwners")]));
    }
}

/// Alternative branch promotions preserve their owners without EIR optimization.
#[test]
fn test_alternative_reference_captures_release_cell_and_object_owners_no_ir_opt() {
    without_ir_opt(test_alternative_reference_captures_release_cell_and_object_owners);
}

/// A flow-proven object remains usable through raw and promoted Mixed reference storage.
#[test]
fn test_core_reference_capture_narrowed_object_reads_preserve_payload() {
    let source = r#"<?php
class NarrowedCaptureObject {
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
}
$value = new NarrowedCaptureObject('old');
if ($argc === 1) {
    $replace = function() use (&$value): void { $value = new NarrowedCaptureObject('new'); };
    $replace();
    unset($replace);
}
if ($value instanceof NarrowedCaptureObject) { echo $value->name; }
unset($value);
"#;
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(source, &[&[], &["raw"]]);
    let mut failures = Vec::new();
    for (mode, (out, expected)) in runs.iter().zip(["new", "old"]).enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} success={} stdout={:?} expected={expected:?}\n{}", out.success, out.stdout, out.stderr));
        }
    }
    let symbols = ["_main".into(), elephc::names::function_symbol("__eir_closure_main_0")];
    assert!(failures.is_empty(), "{}\n{}", failures.join("\n"), selected_function_assembly(&assembly, &symbols));
}

/// Cell payload extraction stays correct without EIR optimization.
#[test]
fn test_core_reference_capture_narrowed_object_reads_preserve_payload_no_ir_opt() {
    without_ir_opt(test_core_reference_capture_narrowed_object_reads_preserve_payload);
}

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
