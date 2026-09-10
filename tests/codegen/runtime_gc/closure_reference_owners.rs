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
