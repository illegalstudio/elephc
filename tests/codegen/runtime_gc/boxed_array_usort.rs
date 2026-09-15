//! Purpose:
//! Verifies boxed user-sort receivers, comparator evaluation and exceptional ownership cleanup.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Declared array returns and properties force the PHP boxed storage boundary.
//! - Both normal and tagged execution must preserve value copies and reference write-back.

use crate::support::*;

/// A temporary callback passed through a user function is retired before its caller's catch.
#[test]
fn test_user_call_temporary_callback_unwind_retires_captures() {
    let source = r#"<?php
class TemporaryCallbackOwner { public function __destruct() { echo "released|"; } }
function temporaryThrowingCallback(): callable {
    $owner = new TemporaryCallbackOwner();
    return function() use ($owner): void { throw new Exception("stop"); };
}
function runTemporaryCallback(callable $callback): void { $callback(); }
function keepTemporaryCallback(callable $callback): callable { return $callback; }
try { runTemporaryCallback(temporaryThrowingCallback()); }
catch (Exception $error) { echo "caught|"; unset($error); }
$kept = keepTemporaryCallback(temporaryThrowingCallback());
try { runTemporaryCallback($kept); }
catch (Exception $error) { echo "kept|"; unset($error); }
unset($kept);
"#;
    assert_clean_sort(source, "released|caught|kept|released|");
}

/// Runtime-selected sort descriptors share the direct boxed-array copy and exceptional publication.
#[test]
fn test_core_boxed_usort_opaque_descriptor_cow_and_throw_cleanup() {
    let source = r#"<?php
function dispatchBoxedSort(callable $sort, array &$items, callable $compare): void {
    $sort($items, $compare);
}
function descriptorSortValues(): array { return ['b' => 2, 'a' => 1]; }
$sort = usort(...);
$items = descriptorSortValues();
$copy = $items;
dispatchBoxedSort($sort, $items, fn(int $a, int $b): int => $a <=> $b);
echo implode(',', $items), ':', implode(',', array_keys($items)), '|', implode(',', $copy), '|';
try {
    dispatchBoxedSort($sort, $items, function(int $a, int $b) use (&$items): int {
        $items = ['replacement' => 9];
        throw new Exception('stop');
    });
} catch (Exception $error) {
    echo $error->getMessage(), '|';
    unset($error);
}
echo implode(',', $items), ':', implode(',', array_keys($items));
unset($items, $copy, $sort);
"#;
    assert_clean_sort(source, "1,2:0,1|2,1|stop|1,2:0,1");
}

/// Declared arrays keep comparator parameter declarations instead of inventing integer elements.
#[test]
fn test_core_boxed_usort_array_contract_keeps_typed_string_callback() {
    let source = r#"<?php
function sortDeclaredWords(array &$words): void {
    usort($words, fn(string $a, string $b): int => strcmp($a, $b));
}
$words = ['last' => str_repeat('z', 3), 'first' => str_repeat('a', 3)];
$copy = $words;
sortDeclaredWords($words);
echo implode(',', $words), '|', implode(',', $copy);
unset($words, $copy);
"#;
    assert_clean_sort(source, "aaa,zzz|zzz,aaa");
}

/// Capturing a property reference borrows the rooted receiver until the sort finalizer releases it.
#[test]
fn test_core_boxed_usort_property_root_keeps_receiver_alive_until_unset() {
    let source = r#"<?php
class RootedSortBag {
    public array $items = ['b' => 2, 'a' => 1];
    public function __destruct() { echo 'bag|'; }
}
$bag = new RootedSortBag();
usort($bag->items, fn(int $a, int $b): int => $a <=> $b);
echo 'sorted|', implode(',', $bag->items), '|';
unset($bag);
echo 'done';
"#;
    assert_clean_sort(source, "sorted|1,2|bag|done");
}

/// Returned arrays support empty and nonempty sorting, typed callbacks and first-class calls.
#[test]
fn test_core_boxed_usort_returned_arrays_and_callable_forms() {
    let source = r#"<?php
function boxedSortWords(): array { return ['last' => 'z', 'first' => 'a', 'middle' => 'm']; }
function emptySortWords(): array { $words = ['a']; array_pop($words); return $words; }
$words = boxedSortWords();
$copy = $words;
usort($words, fn($a, $b) => strcmp($a, $b));
echo implode(',', $words), ':', implode(',', array_keys($words)), '|';
echo implode(',', $copy), ':', implode(',', array_keys($copy)), '|';
$sort = usort(...);
$sort($words, fn(string $a, string $b): int => strcmp($b, $a));
echo implode(',', $words), '|';
$empty = emptySortWords();
usort($empty, fn($a, $b) => strcmp($a, $b));
echo count($empty), ':', implode(',', $empty);
unset($words, $copy, $sort, $empty);
"#;
    assert_clean_sort(source, "a,m,z:0,1,2|z,a,m:last,first,middle|z,m,a|0:");
}

/// Factories finish before the referenced array is copied, including named callback-first calls.
#[test]
fn test_core_boxed_usort_property_source_order_and_snapshot() {
    let source = r#"<?php
class BoxedSortBag {
    public array $items = ['old' => 'old'];
    public bool $observed = true;
    public static array $shared = ['b' => 'b', 'a' => 'a'];
}
function prepareBoxedSort(BoxedSortBag $bag): callable {
    $bag->items = ['newb' => 'b', 'newa' => 'a'];
    return function(string $a, string $b) use ($bag): int {
        if (implode(',', array_keys($bag->items)) !== 'newb,newa') { $bag->observed = false; }
        return strcmp($a, $b);
    };
}
$bag = new BoxedSortBag();
$copy = $bag->items;
usort($bag->items, prepareBoxedSort($bag));
echo implode(',', $bag->items), ':', $bag->observed ? 'T' : 'F', '|', implode(',', $copy), '|';
usort(callback: prepareBoxedSort($bag), array: $bag->items);
echo implode(',', $bag->items), ':', $bag->observed ? 'T' : 'F', '|';
$staticCopy = BoxedSortBag::$shared;
usort(BoxedSortBag::$shared, fn(string $a, string $b): int => strcmp($a, $b));
echo implode(',', BoxedSortBag::$shared), ':', implode(',', $staticCopy);
unset($bag, $copy, $staticCopy);
BoxedSortBag::$shared = [];
"#;
    assert_clean_sort(source, "a,b:T|old|a,b:T|a,b:b,a");
}

/// A comparator may replace the receiver but its sorted private snapshot is published afterwards.
#[test]
fn test_core_boxed_usort_reference_and_comparator_mutation() {
    let source = r#"<?php
function mutateDuringBoxedSort(array &$items): void {
    usort($items, function(int $a, int $b) use (&$items): int {
        $items = ['replacement' => 9];
        return $a <=> $b;
    });
}
function originalBoxedSortNumbers(): array { return ['b' => 2, 'a' => 1]; }
$items = originalBoxedSortNumbers();
$copy = $items;
$alias = &$items;
mutateDuringBoxedSort($items);
echo implode(',', $items), ':', implode(',', $alias), ':', implode(',', $copy);
unset($alias, $items, $copy);
"#;
    assert_clean_sort(source, "1,2:1,2:2,1");
}

/// Throwing comparators still publish a dense snapshot and retire callback and temporary owners.
#[test]
fn test_core_boxed_usort_throw_publishes_snapshot_without_leaks() {
    let source = r#"<?php
class ThrowingSortBag { public array $items = ['b' => 2, 'a' => 1]; }
$bag = new ThrowingSortBag();
$copy = $bag->items;
try {
    usort($bag->items, function(int $a, int $b) use ($bag): int {
        $bag->items = ['replacement' => 9];
        throw new Exception('stop');
    });
} catch (Exception $error) {
    echo $error->getMessage(), '|';
    unset($error);
}
echo count($bag->items), ':', implode(',', array_keys($bag->items)), '|';
echo implode(',', $copy), ':', implode(',', array_keys($copy));
unset($bag, $copy);
"#;
    assert_clean_sort(source, "stop|2:0,1|2,1:b,a");
}

/// An exception while constructing the comparator does not publish or retain a working snapshot.
#[test]
fn test_core_boxed_usort_factory_throw_preserves_receiver() {
    let source = r#"<?php
class FactorySortBag { public array $items = ['b' => 2, 'a' => 1]; }
function failedBoxedComparator(): callable { throw new Exception('factory'); }
$bag = new FactorySortBag();
try { usort($bag->items, failedBoxedComparator()); }
catch (Exception $error) { echo $error->getMessage(), '|'; unset($error); }
echo implode(',', array_keys($bag->items)), ':', implode(',', $bag->items);
unset($bag);
"#;
    assert_clean_sort(source, "factory|b,a:2,1");
}

/// Checks ordinary and tagged execution while requiring balanced heap ownership on the native path.
fn assert_clean_sort(source: &str, expected: &str) {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\nGenerated user assembly:\n{}", out.stdout, out.stderr, asm);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nGenerated user assembly:\n{}", out.stderr, asm);
    assert_eq!(compile_and_run_tagged(source), expected);
}
