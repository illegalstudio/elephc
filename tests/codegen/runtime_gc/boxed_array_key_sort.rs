//! Purpose:
//! Verifies key ordering and ownership across boxed PHP array boundaries.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Returned arrays and declared properties force boxed storage instead of literal specialization.
//! - Sorting preserves numeric keys, object owners, reference aliases and prior value copies.

use crate::support::*;

/// Direct, named and callable key sorts separate boxed arrays while preserving original keys.
#[test]
fn test_core_boxed_array_key_sort_preserves_keys_and_aliases() {
    let source = r#"<?php
class BoxedKeySortOwner {
    public array $items = [3, 1, 2];
    public static array $shared = ['b' => 2, 'a' => 1];
}
function returnedKeySortArray(): array { return [5 => 'five', -2 => 'low', 2 => 'two']; }
$owner = new BoxedKeySortOwner();
$reference = &$owner->items;
$snapshot = $owner->items;
echo krsort(array: $owner->items) ? 'T|' : 'F|';
echo implode(',', array_keys($reference)), ':', implode(',', $reference), '|';
echo implode(',', array_keys($snapshot)), ':', implode(',', $snapshot), '|';
$items = returnedKeySortArray();
$copy = $items;
$ascending = ksort(...);
$ascending($items);
echo implode(',', array_keys($items)), '|', implode(',', array_keys($copy)), '|';
$staticCopy = BoxedKeySortOwner::$shared;
ksort(array: BoxedKeySortOwner::$shared);
echo implode(',', array_keys(BoxedKeySortOwner::$shared)), '|', implode(',', array_keys($staticCopy));
unset($reference, $owner, $snapshot, $items, $copy, $staticCopy);
"#;
    let expected = "T|2,1,0:2,1,3|0,1,2:3,1,2|-2,2,5|5,-2,2|a,b|b,a";
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Sorting a boxed array of objects changes only its key order and never duplicates or drops owners.
#[test]
fn test_core_boxed_array_key_sort_retains_object_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SortedArrayValue {
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
    public function __destruct() { echo 'd', $this->id, '|'; }
}
function boxedKeySortObjects(): array { return [new SortedArrayValue(1), new SortedArrayValue(2)]; }
$items = boxedKeySortObjects();
$copy = $items;
krsort($items);
echo $items[0]->id, ':', $items[1]->id, '|';
unset($items);
echo 'copy|';
unset($copy);
echo 'done';
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:2|copy|d1|d2|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
