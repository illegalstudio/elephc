//! Purpose:
//! Verifies boxed `array_walk` traversal, reference writes, and unwind ownership.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Callback mutations of source aliases must not invalidate active hash entry references.
//! - Throwing callbacks must retire argument boxes, source roots, and descriptor captures.

use crate::support::*;

/// A callback cannot publish an unmanaged element reference through a closure capture.
#[test]
fn test_core_boxed_array_walk_rejects_escaped_reference_capture() {
    let source = r#"<?php
function boxedWalkCaptureInput(): array { return ["value" => str_repeat("x", 24)]; }
$saved = static fn(): string => "unset";
$items = boxedWalkCaptureInput();
try {
    array_walk($items, function(mixed &$value) use (&$saved): void {
        $value = "changed";
        $saved = function() use (&$value): string { return strval($value); };
    });
    echo "missed|";
} catch (Error $error) {
    echo $error->getMessage(), "|", $items["value"], "|";
    unset($error);
}
unset($saved, $items);
echo "done";
"#;
    let expected = "Escaping a borrowed boxed array_walk() element reference is not supported|changed|done";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Constructor-promoted reference properties cannot retain a borrowed walk entry.
#[test]
fn test_core_boxed_array_walk_rejects_promoted_reference_property_escape() {
    let source = r#"<?php
class BoxedWalkReferenceHolder {
    public function __construct(public mixed &$value) {}
}
function boxedWalkPropertyInput(): array { return ["value" => "start"]; }

$items = boxedWalkPropertyInput();
try {
    array_walk($items, static function(mixed &$value): void {
        $value = "property";
        $holder = new BoxedWalkReferenceHolder($value);
        echo $holder->value;
    });
    echo "missed|";
} catch (Error $error) {
    echo $error->getMessage(), "|", $items["value"], "|";
    unset($error);
}
unset($items);
echo "done";
"#;
    let expected = "Escaping a borrowed boxed array_walk() element reference is not supported|property|done";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// An active element borrow relayed through a nested by-reference return is still copied out.
///
/// The relay's own frame sees only a by-reference parameter, so its acquisition boundary asks the
/// runtime: the address owns no managed cell, but it IS an exact node of the active borrow chain,
/// so the return is accepted with no lease instead of raising the owner-zero `Error`. The invoker
/// then copies the pointee into an owned `Mixed` before anything can free the entry.
#[test]
fn test_core_boxed_array_walk_relays_an_active_element_borrow_through_a_nested_return() {
    let source = r#"<?php
function &boxedWalkInnerRelay(mixed &$value): mixed {
    return $value;
}
function &boxedWalkOuterRelay(mixed &$value): mixed {
    $value = "relayed";
    $inner = &boxedWalkInnerRelay($value);
    return $inner;
}
function boxedWalkRelayInput(): array { return ["value" => "start"]; }

$items = boxedWalkRelayInput();
$callback = "boxedWalkOuterRelay";
array_walk($items, $callback);
echo $items["value"], "|";
unset($callback, $items);
echo "done";
"#;
    let expected = "relayed|done";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// A borrowed element return keeps the address the `return` named across a rebinding `finally`.
///
/// The acquisition publishes the SELECTED address as its own snapshot, separately from the
/// managed lease it did not take. A fallthrough `finally` that rebinds the returned variable to
/// an unrelated managed cell therefore cannot change which storage the invoker copies out.
#[test]
fn test_core_boxed_array_walk_borrowed_return_survives_a_rebinding_finally() {
    let source = r#"<?php
function boxedWalkFinallySeed(): mixed { return "rebound"; }
function &boxedWalkFinallyRelay(mixed &$value): mixed {
    $value = "selected";
    $other = boxedWalkFinallySeed();
    try {
        return $value;
    } finally {
        $value = &$other;
    }
}
function boxedWalkFinallyConsumer(mixed &$value): void {
    $copied = boxedWalkFinallyRelay($value);
    echo $copied, "|";
}
function boxedWalkFinallyInput(): array { return ["value" => "start"]; }

$items = boxedWalkFinallyInput();
$callback = "boxedWalkFinallyConsumer";
array_walk($items, $callback);
echo $items["value"], "|";
unset($callback, $items);
echo "done";
"#;
    let expected = "selected|selected|done";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// A reference-returning callback may mutate an entry because its result is copied into owned Mixed.
#[test]
fn test_core_boxed_array_walk_copies_reference_returning_callback_value() {
    let source = r#"<?php
function &boxedWalkReferenceResult(mixed &$value): mixed {
    $value = "returned";
    return $value;
}
function boxedWalkReferenceReturnInput(): array { return ["value" => "start"]; }

$items = boxedWalkReferenceReturnInput();
$callback = "boxedWalkReferenceResult";
array_walk($items, $callback);
echo $items["value"], "|";
unset($callback, $items);
echo "done";
"#;
    let expected = "returned|done";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// A local by-reference return transfers a managed cell owner beyond callee cleanup.
#[test]
fn test_core_local_reference_return_keeps_managed_cell_alive() {
    let source = r#"<?php
function &managedReferenceRelay(mixed &$value): mixed {
    $value = "relayed";
    return $value;
}
function managedReferenceSeed(): mixed { return "start"; }

$source = managedReferenceSeed();
$alias = &managedReferenceRelay($source);
unset($source);
echo $alias, "|";
unset($alias);
echo "done";
"#;
    let expected = "relayed|done";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Managed caller-owned references retain ordinary escaping closure behavior.
#[test]
fn test_core_boxed_array_walk_escape_guard_accepts_managed_reference_cells() {
    let source = r#"<?php
$value = "ready";
$saved = function() use (&$value): string { return strval($value); };
$value = "managed";
unset($value);
echo $saved(), "|";
unset($saved);
echo "done";
"#;
    let expected = "managed|done";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Alias replacement and release during callbacks preserve the rooted walk and COW snapshots.
#[test]
fn test_core_boxed_array_walk_alias_mutation_keeps_rooted_entries_live() {
    let source = r#"<?php
function walkOwnedValues(array &$items): void {
    $snapshot = $items;
    $alias = $items;
    array_walk($items, function(mixed &$value, mixed $key) use (&$alias): void {
        if ($key === "first") {
            $alias = ["detached" => 99];
        }
        $value = $value + 10;
    });
    echo $items["first"], ":", $items["second"], "|";
    echo $snapshot["first"], ":", $snapshot["second"], "|";
    echo $alias["detached"], ";";
    unset($snapshot, $alias);
}

function walkOwnedLeaves(array &$items): void {
    $snapshot = $items;
    $alias = $items;
    array_walk_recursive($items, function(mixed &$value, mixed $key) use (&$alias): void {
        if ($key === "left") {
            $alias = [];
        }
        $value = $value + 20;
    });
    echo $items["row"]["left"], ":", $items["row"]["right"], "|";
    echo $snapshot["row"]["left"], ":", $snapshot["row"]["right"], "|";
    echo count($alias) === 0 ? "released" : "live";
    unset($snapshot);
}

$flat = ["first" => 1, "second" => 2];
$nested = ["row" => ["left" => 3, "right" => 4]];
walkOwnedValues($flat);
walkOwnedLeaves($nested);
unset($flat, $nested);
"#;
    let expected = "11:12|1:2|99;23:24|3:4|released";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// A recursive callback throw releases its owned arguments, descriptor capture, and source root.
#[test]
fn test_core_boxed_array_walk_callback_throw_retires_every_owner() {
    let source = r#"<?php
class BoxedWalkCapture {
    public int $length = 17;
    public function __destruct() { echo "drop|"; }
}
function boxedWalkThrowInput(): array {
    return ["outer" => ["leaf" => "old"], "last" => "kept"];
}

$capture = new BoxedWalkCapture();
$items = boxedWalkThrowInput();
$callback = function(mixed &$value, mixed $key) use ($capture): void {
    $value = str_repeat("x", $capture->length);
    throw new RuntimeException("walk:" . $key);
};
try {
    array_walk_recursive($items, $callback);
    echo "missed|";
} catch (RuntimeException $error) {
    echo $error->getMessage(), "|", strlen($items["outer"]["leaf"]), "|";
    unset($error);
}
unset($callback, $capture, $items);
echo "done";
"#;
    let expected = "walk:leaf|17|drop|done";
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}
