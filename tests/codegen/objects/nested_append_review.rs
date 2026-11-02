//! Purpose:
//! Regressions for nested ArrayAccess appends and deeper false-parent updates.
//!
//! Called from:
//! - The objects codegen integration suite.
//!
//! Key details:
//! - Fixtures verify PHP-visible mutation and ownership with EIR optimization on and off.
//! - Deeper write promotion must not broaden ordinary scalar array reads.

use crate::support::*;

/// Checks nested-write output and complete cleanup of every heap owner.
fn assert_nested_write(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A selected ArrayObject receives an append instead of an array-only runtime operation.
#[test]
fn test_nested_append_review_array_object() {
    assert_nested_write(r#"<?php
$items = [new ArrayObject([1])];
$items[0][] = 2;
echo count($items[0]), ':', $items[0][0], ',', $items[0][1];
unset($items);
"#, "2:1,2");
}

/// ArrayObject append dispatch remains correct without EIR optimization.
#[test]
fn test_nested_append_review_array_object_no_ir_opt() {
    without_ir_opt(test_nested_append_review_array_object);
}

/// A constructor-only control distinguishes baseline SPL ownership from append ownership.
#[test]
fn test_nested_append_review_array_object_control() {
    assert_nested_write(r#"<?php
$items = [new ArrayObject([1])];
echo count($items[0]), ':', $items[0][0];
unset($items);
"#, "1:1");
}

/// A deeper generated increment promotes its false ancestor before reading the leaf.
#[test]
fn test_nested_append_review_deep_false_increment() {
    assert_nested_write(r#"<?php
$items = [false]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { return true; });
++ $items[0][0][1];
echo json_encode($items), ':', json_encode($copy);
restore_error_handler(); unset($items, $copy);
"#, r#"[[{"1":1}]]:[false]"#);
}

/// Deeper false promotion retains the same semantics without EIR optimization.
#[test]
fn test_nested_append_review_deep_false_increment_no_ir_opt() {
    without_ir_opt(test_nested_append_review_deep_false_increment);
}

/// Boxed ArrayAccess children receive a null key once, including exceptional returns.
#[test]
fn test_nested_append_review_custom_object_throw() {
    assert_nested_write(r#"<?php
class Bag implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return false; }
    public function offsetGet(mixed $offset): mixed { return null; }
    public function offsetUnset(mixed $offset): void {}
    public function offsetSet(mixed $offset, mixed $value): void {
        echo $offset === null ? 'null' : 'bad', ':', $value, '|'; throw new Error('stop');
    }
}
function append(mixed $object): void {
    $items = [$object];
    try { $items[0][] = str_repeat('v', 2); }
    catch (Error $error) { echo $error->getMessage(); }
}
append(new Bag());
"#, "null:vv|stop");
}

/// Throwing ArrayAccess appends balance operand owners without optimization.
#[test]
fn test_nested_append_review_custom_object_throw_no_ir_opt() {
    without_ir_opt(test_nested_append_review_custom_object_throw);
}

/// Ordinary objects with a similarly named method still fail as non-ArrayAccess objects.
#[test]
fn test_nested_append_review_non_indexable_object() {
    assert_nested_write(r#"<?php
class NotBag { public function offsetSet(mixed $key, mixed $value): void { echo 'bad'; } }
function append(mixed $object): void {
    $items = [$object];
    try { $items[0][] = str_repeat('v', 2); }
    catch (Error $error) { echo $error->getMessage(); }
}
append(new NotBag());
"#, "Cannot use object of type NotBag as array");
}

/// Non-indexable object errors remain catchable without optimization.
#[test]
fn test_nested_append_review_non_indexable_object_no_ir_opt() {
    without_ir_opt(test_nested_append_review_non_indexable_object);
}

/// Closures and first-class callables reject nested appends instead of discarding them.
#[test]
fn test_nested_append_review_closure_child() {
    assert_nested_write(r#"<?php
function append(mixed $object): void {
    $items = [$object];
    try { $items[0][] = str_repeat('v', 2); echo 'bad'; }
    catch (Error $error) { echo $error->getMessage(), '|'; }
}
append(function() {}); append(strlen(...));
"#, "Cannot use object of type Closure as array|Cannot use object of type Closure as array|");
}

/// Callable-object rejection retains the same error without EIR optimization.
#[test]
fn test_nested_append_review_closure_child_no_ir_opt() {
    without_ir_opt(test_nested_append_review_closure_child);
}

/// Keyed nested writes share the same consuming Closure rejection as append fallback.
#[test]
fn test_nested_append_review_closure_keyed_write() {
    assert_nested_write(r#"<?php
function write(mixed $object): void {
    $items = [$object];
    try { $items[0][0] = str_repeat('v', 2); echo 'bad'; }
    catch (Error $error) { echo $error->getMessage(); }
}
write(function() {});
"#, "Cannot use object of type Closure as array");
}

/// The shared keyed-write error remains catchable without EIR optimization.
#[test]
fn test_nested_append_review_closure_keyed_write_no_ir_opt() {
    without_ir_opt(test_nested_append_review_closure_keyed_write);
}

/// Mixed child cells retain object identity while ordinary arrays still detach on write.
#[test]
fn test_nested_append_review_mixed_children() {
    assert_nested_write(r#"<?php
$items = [new ArrayObject([1]), [3], null]; $copy = $items;
for ($i = 0; $i < 3; $i++) { $items[$i][] = 2; }
echo count($items[0]), ':', $items[0][1], ':', $copy[0][1], '|';
echo json_encode($items[1]), ':', json_encode($copy[1]), '|';
echo json_encode($items[2]), ':', json_encode($copy[2]);
unset($items, $copy);
"#, "2:2:2|[3,2]:[3]|[2]:null");
}

/// Mixed array and object append dispatch does not rely on optimization.
#[test]
fn test_nested_append_review_mixed_children_no_ir_opt() {
    without_ir_opt(test_nested_append_review_mixed_children);
}

/// A selected WeakMap retains PHP's distinct unsupported append diagnostic.
#[test]
fn test_nested_append_review_weak_map() {
    assert_nested_write(r#"<?php
$items = [new WeakMap()];
try { $items[0][] = str_repeat('v', 2); }
catch (Error $error) { echo $error->getMessage(); }
unset($items, $error);
"#, "Cannot append to WeakMap");
}

/// WeakMap append errors remain unchanged without optimization.
#[test]
fn test_nested_append_review_weak_map_no_ir_opt() {
    without_ir_opt(test_nested_append_review_weak_map);
}

/// The writeback of a property compound reuses its read's float-key diagnostic.
#[test]
fn test_nested_append_review_property_float_key() {
    assert_nested_write(r#"<?php
class Owner { public array $nested = [[1, 2], [3, 4]]; }
$object = new Owner();
set_error_handler(function($level, $message) {
    if ($level === E_DEPRECATED) { echo 'W'; } return true;
});
$object->nested[1.5][0] += 1;
restore_error_handler(); echo '|', json_encode($object->nested); unset($object);
"#, "W|[[1,2],[4,4]]");
}

/// Property writeback diagnoses float keys once even without EIR optimization.
#[test]
fn test_nested_append_review_property_float_key_no_ir_opt() {
    without_ir_opt(test_nested_append_review_property_float_key);
}
