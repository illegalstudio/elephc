//! Purpose:
//! Verifies pop/shift across boxed PHP array boundaries, aliases and ownership transitions.
//!
//! Called from:
//! - The runtime GC codegen integration suite on executable targets.
//!
//! Key details:
//! - Declared and concrete array inputs exercise both runtime storage paths.
//! - Empty results, sparse numeric keys, named keys and retained removed values are covered.

use crate::support::*;

/// Boxed pop restores the next append key only for the immediate integer predecessor.
#[test]
fn test_core_boxed_array_pop_append_cursor() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function appendAfterPop(array $items): string {
    array_pop($items);
    $items[] = 'c';
    return implode(',', array_keys($items));
}
for ($i = 0; $i < 6; $i++) {
    echo appendAfterPop([0 => 'a', 1 => 'b']), ';';
    echo appendAfterPop([2 => 'a', 6 => 'b']), ';';
    echo appendAfterPop([9 => 'a', 2 => 'b']), ';';
    echo appendAfterPop(['x' => 'a', 4 => 'b']), ';';
    echo appendAfterPop([4 => 'a', 'x' => 'b']), ';';
    echo appendAfterPop([-2 => 'a', -1 => 'b']), ';';
    echo appendAfterPop([PHP_INT_MAX => 'a']), ';';
    echo appendAfterPop([PHP_INT_MIN => 'a']), ';';
    echo appendAfterPop(['x' => 'a', PHP_INT_MIN => 'b']), ';';
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0,1;2,6;9,10;x,4;4,5;-2,-1;9223372036854775807;0;x,0;".repeat(6));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Concrete hash pop updates only its separated receiver's append history, not a COW copy.
#[test]
fn test_core_assoc_array_pop_append_cursor() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 6; $i++) {
    $items = [2 => 'a', 6 => 'b'];
    $copy = $items;
    array_pop($items);
    $items[] = 'c';
    echo implode(',', array_keys($items)), '|', implode(',', array_keys($copy)), '|',
        implode(',', $items), '|', implode(',', $copy), ';';
    unset($items, $copy);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2,6|2,6|a,c|a,b;".repeat(6));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Concrete string slots transfer their owners on both discarded and retained pop/shift results.
#[test]
fn test_core_concrete_array_pop_shift_transfer_string_owners() {
    let source = r#"<?php
function discardConcreteEnds(int $length): array {
    $items = [str_repeat('a', $length), str_repeat('b', $length)];
    array_pop($items);
    array_shift($items);
    return $items;
}
for ($i = 0; $i < 6; $i++) {
    $items = [str_repeat('a', 12), str_repeat('b', 12)];
    $copy = $items;
    $first = array_shift($items);
    $last = array_pop($items);
    unset($items);
    echo strlen($first), ':', strlen($last), ':', strlen($copy[0]), ':', strlen($copy[1]), '|';
    unset($first, $last, $copy);
    $empty = discardConcreteEnds(12);
    echo count($empty), '|';
    unset($empty);
}
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "12:12:12:12|0|".repeat(6));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Concrete object arrays keep COW aliases alive but release each removed owner exactly once.
#[test]
fn test_core_concrete_array_pop_shift_transfer_object_owners() {
    let source = r#"<?php
class ConcreteRemovedObject {
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
    public function __destruct() { echo 'd', $this->id, '|'; }
}
$items = [new ConcreteRemovedObject(1), new ConcreteRemovedObject(2)];
$copy = $items;
$first = array_shift($items);
$last = array_pop($items);
unset($items, $copy);
echo $first->id, ':', $last->id, '|';
unset($first, $last);
echo 'done';
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:2|d1|d2|done");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Pop keeps numeric holes while shift renumbers only integer keys and leaves value copies unchanged.
#[test]
fn test_core_boxed_array_pop_shift_preserve_keys_and_value_copies() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function shiftBoxedArray(array &$items): mixed { return array_shift($items); }
function popBoxedArray(array &$items): mixed { return array_pop($items); }
$list = [1, 'two', 3.5];
$copy = $list;
echo shiftBoxedArray($list), ':', popBoxedArray($list), ':', implode(',', $list), '|';
echo implode(',', $copy), '|';
$map = [10 => 'ten', 'keep' => 'note', 30 => 'thirty', 'tail' => 'last'];
$snapshot = $map;
echo shiftBoxedArray($map), ':', popBoxedArray($map), ':';
echo implode(',', array_keys($map)), ':', implode(',', $map), '|';
echo implode(',', array_keys($snapshot));
unset($list, $copy, $map, $snapshot);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:3.5:two|1,two,3.5|ten:last:keep,0:note,thirty|10,keep,30,tail", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Named and first-class builtin calls publish the separated cell into a declared property reference.
#[test]
fn test_core_boxed_array_pop_shift_named_callable_property_references() {
    let source = r#"<?php
class BoxedTakeOwner { public array $items = [1, 2, 3]; }
$owner = new BoxedTakeOwner();
$alias = &$owner->items;
$copy = $owner->items;
$pop = array_pop(...);
echo $pop($alias), ':', array_shift(array: $owner->items), ':';
echo implode(',', $alias), '|', implode(',', $copy), '|', implode(',', $owner->items);
"#;
    assert_eq!(compile_and_run(source), "3:1:2|1,2,3|2");
    assert_eq!(compile_and_run_tagged(source), "3:1:2|1,2,3|2");
}

/// Removed objects remain alive after both array owners disappear and are destroyed exactly once.
#[test]
fn test_core_boxed_array_pop_shift_transfer_removed_object_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RemovedArrayObject {
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
    public function __destruct() { echo 'd', $this->id, '|'; }
}
function boxedTakeObjects(): array { return [new RemovedArrayObject(1), new RemovedArrayObject(2)]; }
$items = boxedTakeObjects();
$copy = $items;
$first = array_shift($items);
$last = array_pop($items);
unset($items, $copy);
echo $first->id, ':', $last->id, '|';
unset($first, $last);
echo 'done';
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:2|d1|d2|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Both edge operations return PHP null on empty boxed lists and emptied hashes.
#[test]
fn test_core_boxed_array_pop_shift_empty_results_are_null() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function emptyTakeArray(array $items): void {
    while (count($items) > 0) { array_pop($items); }
    echo gettype(array_pop($items)), ':', gettype(array_shift($items)), ':', count($items), '|';
}
emptyTakeArray([]);
emptyTakeArray(['key' => 7]);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "NULL:NULL:0|NULL:NULL:0|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
