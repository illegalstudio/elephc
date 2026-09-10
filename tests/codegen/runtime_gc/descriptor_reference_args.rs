//! Purpose:
//! Verifies temporary reference arguments owned by callable descriptor invokers.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Default cells retire on return and throw, but escaping captures retain their own cell lease.

use crate::support::*;

/// Callable defaults balance their current boxed array even when the callee replaces its layout.
#[test]
fn test_core_descriptor_reference_defaults_release_replaced_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function descriptorDefault(array &$items = [], int $value = 0): int {
    $items = ["value" => str_repeat("v", $value)];
    return strlen($items["value"]);
}
$callback = descriptorDefault(...);
for ($i = 1; $i < 4; $i++) {
    echo $callback(value: $i), ":", call_user_func_array("descriptorDefault", ["value" => $i]), "|";
}
unset($callback);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:1|2:2|3:3|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A closure capturing an omitted reference keeps its cell alive after the invoker retires its lease.
#[test]
fn test_core_descriptor_reference_default_survives_escaping_capture() {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
function descriptorCapture(array &$items = [], int $value = 0): callable {
    $items[] = $value;
    return function() use (&$items): int { $items[] = 7; return count($items); };
}
$factory = descriptorCapture(...);
$callback = $factory(value: 3);
unset($factory);
echo $callback(), ":", $callback();
unset($callback);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:3", "{}\nuser assembly:\n{}", out.stderr, assembly);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nuser assembly:\n{}", out.stderr, assembly);
}

/// Direct explicit reference arguments separate closure-cell mutation from descriptor default staging.
#[test]
fn test_core_direct_reference_capture_preserves_array_mutations() {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
function directReferenceCapture(array &$items): callable {
    $items[] = 3;
    return function() use (&$items): int { $items[] = 7; return count($items); };
}
$items = [];
$callback = directReferenceCapture($items);
echo $callback(), ":", $callback(), ":", count($items);
unset($callback, $items);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}\n{}", out.stdout, out.stderr, assembly);
    assert_eq!(out.stdout, "2:3:3", "{}\nuser assembly:\n{}", out.stderr, assembly);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nuser assembly:\n{}", out.stderr, assembly);
}

/// Native throws still release default reference cells and every value written into them.
#[test]
fn test_core_descriptor_reference_defaults_release_on_throw() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function descriptorThrow(array &$items = [], int $value = 0): void {
    $items = [str_repeat("x", $value)];
    throw new Exception("default");
}
$callback = descriptorThrow(...);
for ($i = 1; $i < 4; $i++) {
    try { $callback(value: $i); } catch (Exception $error) { echo $error->getMessage(), "|"; }
}
unset($callback, $error);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "default|default|default|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
