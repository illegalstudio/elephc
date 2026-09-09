//! Purpose:
//! Verifies reference-cell ownership across alias creation, rebinding and retirement.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Heap-backed cells survive their original variable while aliases remain.
//! - Borrowed indexed-element addresses never become independently owned heap allocations.

use crate::support::*;

/// Temporary receivers and callee-local objects transfer cells before their cleanup destroys them.
#[test]
fn test_core_returned_property_reference_survives_temporary_owners() {
    let source = r#"<?php
class TemporaryReferenceHolder {
    public array $items = [7];
    public function &reference(): array { return $this->items; }
    public function __destruct() { echo 'holder|'; }
}
function &temporaryArgumentReference(TemporaryReferenceHolder $holder): array { return $holder->items; }
function &localObjectReference(): array {
    $holder = new TemporaryReferenceHolder();
    return $holder->items;
}
function temporaryReferenceOwners(): void {
    $method = &(new TemporaryReferenceHolder())->reference();
    $argument = &temporaryArgumentReference(new TemporaryReferenceHolder());
    $local = &localObjectReference();
    echo implode(',', $method), ':', implode(',', $argument), ':', implode(',', $local), '|';
    unset($method, $argument, $local);
    echo 'done';
}
temporaryReferenceOwners();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "holder|holder|holder|7:7:7|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "holder|holder|holder|7:7:7|done");
}

/// Scalar and string cell pointers stay in the integer ABI across callee-local cleanup.
#[test]
fn test_core_reference_returns_dereference_typed_values() {
    let source = r#"<?php
class TypedReferenceHolder {
    public string $text = 'alpha';
    public float $number = 1.5;
}
function &typedStringReference(): string {
    $holder = new TypedReferenceHolder();
    return $holder->text;
}
function &typedFloatReference(): float {
    $holder = new TypedReferenceHolder();
    return $holder->number;
}
function typedReferenceValues(): void {
    $text = &typedStringReference();
    $number = &typedFloatReference();
    echo $text, ':', $number, '|';
    $text = 'beta';
    $number = 2.5;
    echo $text, ':', $number, '|', typedStringReference(), ':', typedFloatReference();
    unset($text, $number);
}
typedReferenceValues();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "alpha:1.5|beta:2.5|alpha:1.5", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "alpha:1.5|beta:2.5|alpha:1.5");
}

/// Ordinary direct and descriptor calls copy referenced arrays instead of returning the cell address.
#[test]
fn test_core_reference_return_value_copies_and_descriptor_calls() {
    let source = r#"<?php
class ValueReferenceHolder { public array $items = [1]; }
function &valueReference(ValueReferenceHolder $holder): array { return $holder->items; }
function &otherValueReference(ValueReferenceHolder $holder): array { return $holder->items; }
function referenceValueCopies(int $choice): void {
    $holder = new ValueReferenceHolder();
    $direct = valueReference($holder);
    $callback = $choice > 0 ? 'valueReference' : 'otherValueReference';
    $dynamic = $callback($holder);
    $firstClass = valueReference(...);
    $first = $firstClass($holder);
    array_push($holder->items, 2);
    echo implode(',', $direct), ':', implode(',', $dynamic), ':', implode(',', $first), '|';
    unset($holder);
    echo implode(',', $direct), ':', implode(',', $dynamic), ':', implode(',', $first);
    unset($direct, $dynamic, $first, $callback, $firstClass);
}
referenceValueCopies($argc);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:1:1|1:1:1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "1:1:1|1:1:1");
}

/// An exception during callee epilogue cleanup retires the unpublished returned cell lease.
#[test]
fn test_core_reference_return_lease_released_when_callee_cleanup_throws() {
    let source = r#"<?php
class ReturnedLeasePayload { public function __destruct() { echo 'payload|'; } }
class ThrowingReturnedLeaseHolder {
    public array $items = [];
    public function __destruct() { echo 'holder|'; throw new Exception('cleanup'); }
}
function &throwingReturnedLease(): array {
    $holder = new ThrowingReturnedLeaseHolder();
    $holder->items = [new ReturnedLeasePayload()];
    return $holder->items;
}
try { $value = &throwingReturnedLease(); } catch (Exception $error) { echo 'caught'; }
unset($error);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "holder|payload|caught", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "holder|payload|caught");
}

/// Rebinding the object's own local retains its property cell before destroying the old receiver.
#[test]
fn test_core_owned_property_reference_replaces_its_object_binding() {
    let source = r#"<?php
class ReboundPropertyHolder {
    public array $items = [6];
    public function __destruct() { echo 'holder|'; }
}
function rebindPropertyOwner(): void {
    $holder = new ReboundPropertyHolder();
    $holder = &$holder->items;
    echo implode(',', $holder), '|';
    unset($holder);
    echo 'done';
}
rebindPropertyOwner();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "holder|6|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "holder|6|done");
}

/// References returned by methods or free functions must survive retirement of their object owner.
#[test]
fn test_core_returned_property_reference_outlives_object() {
    let source = r#"<?php
class ReturnedPropertyHolder {
    public array $items = [4];
    public function &reference(): array { return $this->items; }
    public function __destruct() { echo 'holder|'; }
}
function &returnedProperty(ReturnedPropertyHolder $holder): array { return $holder->items; }
function survivingReturnedProperty(): void {
    $holder = new ReturnedPropertyHolder();
    $method = &$holder->reference();
    $function = &returnedProperty($holder);
    unset($holder);
    array_push($method, 5);
    echo implode(',', $function), '|';
    unset($method, $function);
    echo 'done';
}
survivingReturnedProperty();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "holder|4,5|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "holder|4,5|done");
}

/// A retained property cell survives its object and releases its payload at the last alias.
#[test]
fn test_core_owned_property_reference_outlives_object() {
    let source = r#"<?php
class OwnedPropertyPayload {
    public int $id = 9;
    public function __destruct() { echo 'payload|'; }
}
class OwnedPropertyHolder {
    public array $items = [];
    public function __destruct() { echo 'holder|'; }
}
function survivingPropertyReference(): void {
    $holder = new OwnedPropertyHolder();
    $holder->items = [new OwnedPropertyPayload()];
    $first = &$holder->items;
    $last = &$first;
    unset($holder, $first);
    echo $last[0]->id, '|';
    unset($last);
    echo 'done';
}
survivingPropertyReference();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "holder|9|payload|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "holder|9|payload|done");
}

/// Cloning separates singleton reference cells but preserves cells with a live external alias.
#[test]
fn test_core_owned_property_reference_clone_preserves_php_aliases() {
    let source = r#"<?php
class ClonePropertyHolder { public array $items = [1]; }
function clonePropertyReferences(): void {
    $original = new ClonePropertyHolder();
    $reference = &$original->items;
    unset($reference);
    $copy = clone $original;
    array_push($copy->items, 2);
    echo implode(',', $original->items), ':', implode(',', $copy->items), '|';
    unset($copy);
    $reference = &$original->items;
    $shared = clone $original;
    array_push($shared->items, 3);
    echo implode(',', $original->items), ':', implode(',', $reference), '|';
    unset($original, $shared);
    echo implode(',', $reference);
    unset($reference);
}
clonePropertyReferences();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:1,2|1,3:1,3|1,3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "1:1,2|1,3:1,3|1,3");
}

/// An external cell alias roots its object's cycle until the alias is explicitly retired.
#[test]
fn test_core_owned_property_reference_participates_in_cycle_collection() {
    let source = r#"<?php
class CyclicPropertyHolder {
    public array $items = [];
    public function __destruct() { echo 'collected|'; }
}
function cyclicPropertyReference(): void {
    $holder = new CyclicPropertyHolder();
    $holder->items = [$holder];
    $reference = &$holder->items;
    unset($holder);
    gc_collect_cycles();
    echo count($reference), '|';
    unset($reference);
    gc_collect_cycles();
    echo 'done';
}
cyclicPropertyReference();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1|collected|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "1|collected|done");
}

/// Transitive aliases keep a heap cell alive until its last binding releases the payload.
#[test]
fn test_core_reference_cell_aliases_outlive_original_binding() {
    let source = r#"<?php
class ReferenceCellPayload {
    public int $id = 9;
    public function __destruct() { echo 'd', $this->id, '|'; }
}
function retainedCellAliases(): void {
    $original = [new ReferenceCellPayload()];
    $first = &$original;
    $last = &$first;
    unset($original, $first);
    echo $last[0]->id, '|';
    unset($last);
    echo 'done';
}
retainedCellAliases();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9|d9|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "9|d9|done");
}

/// Rebinding retires exactly the previous owner without making inline element aliases own their parent.
#[test]
fn test_core_reference_cell_rebinding_and_borrowed_element_owners() {
    let source = r#"<?php
function reboundCellAliases(): void {
    $a = ['old'];
    $b = ['new'];
    $alias = &$a;
    $alias = &$a;
    $alias = &$b;
    unset($a, $b);
    echo $alias[0], '|';
    unset($alias);
    $numbers = [3, 4];
    $first = &$numbers[0];
    $second = &$first;
    unset($first);
    $second = 8;
    echo $numbers[0], ':', $numbers[1];
    unset($second, $numbers);
}
reboundCellAliases();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "new|8:4", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "new|8:4");
}
