//! Purpose:
//! Verifies eval reads values returned through native method reference cells.
//!
//! Called from:
//! - Executable codegen CI on every supported host target.
//!
//! Key details:
//! - Value copies outlive the receiver and retire the callee's transferred cell lease.

use crate::support::compile_and_run_with_heap_debug;

/// Exceptional cancellation retires the inner lease before the catch unsets its source binding.
#[test]
fn test_cancelled_reference_return_lease_retires_before_catch_side_unset() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CancelledReturnLeaseNode {
    public string $id;
    public function __construct(string $id) { $this->id = $id; }
    public function __destruct() { echo 'drop:', $this->id, '|'; }
}
function &cancelledReturnLease(): CancelledReturnLeaseNode {
    $outer = new CancelledReturnLeaseNode('a'); $inner = new CancelledReturnLeaseNode('b');
    try { return $outer; }
    finally {
        try { try { return $inner; } finally { throw new Exception('cancel'); } }
        catch (Exception $error) { unset($inner); echo 'after|'; }
    }
}
$alias =& cancelledReturnLease(); echo $alias->id, '|'; unset($alias);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop:b|after|a|drop:a|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Retiring an owned property receiver must preserve the newer fixed return-lease guard.
#[test]
fn test_pending_property_return_guard_survives_receiver_owner_retirement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class GuardedPropertyReturnNode {
    public string $id;
    public function __construct(string $id) { $this->id = $id; }
    public function __destruct() { echo 'drop:', $this->id, '|'; }
}
class GuardedPropertyReturnHolder {
    public GuardedPropertyReturnNode $node;
    public function __construct() { $this->node = new GuardedPropertyReturnNode('temporary'); }
    public function __destruct() { echo 'holder|'; }
}
function guardedPropertyReturnHolder(): GuardedPropertyReturnHolder {
    return new GuardedPropertyReturnHolder();
}
function &guardedPropertyReturn(): GuardedPropertyReturnNode {
    $fallback = new GuardedPropertyReturnNode('fallback');
    try {
        try { return guardedPropertyReturnHolder()->node; }
        finally { echo 'finally|'; throw new Exception('cancel'); }
    } catch (Exception $error) { echo 'caught|'; }
    return $fallback;
}
$alias =& guardedPropertyReturn(); echo 'return:', $alias->id, '|'; unset($alias);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "holder|finally|drop:temporary|caught|return:fallback|drop:fallback|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A cancelled inner reference return preserves the outer lease and releases superseded values.
#[test]
fn test_nested_pending_reference_returns_keep_independent_cell_leases() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class PendingReferenceLeaseNode {
    public string $id;
    public function __construct(string $id) { $this->id = $id; }
    public function __destruct() { echo 'drop:', $this->id, '|'; }
}
function &caughtInnerReferenceLease(): PendingReferenceLeaseNode {
    $outer = new PendingReferenceLeaseNode('outer'); $inner = new PendingReferenceLeaseNode('inner');
    try { return $outer; }
    finally {
        try { try { return $inner; } finally { throw new Exception('cancel inner'); } }
        catch (Exception $error) { echo 'caught|'; }
    }
}
function &supersededReferenceLease(): PendingReferenceLeaseNode {
    $outer = new PendingReferenceLeaseNode('old'); $inner = new PendingReferenceLeaseNode('new');
    try { return $outer; }
    finally { unset($outer); try { return $inner; } finally { echo 'inner-finally|'; } }
}
$first =& caughtInnerReferenceLease(); echo 'return:', $first->id, '|'; unset($first);
$second =& supersededReferenceLease(); echo 'return:', $second->id, '|'; unset($second);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|drop:inner|return:outer|drop:outer|inner-finally|drop:old|return:new|drop:new|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Consuming reference-return conversions preserve unique, shared and empty source containers.
#[test]
fn test_reference_return_container_conversions_own_their_input() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function &uniqueContainerReference(): array<mixed> { $items = [1, 2]; return $items; }
function &sharedContainerReference(array<int> $input): array<mixed> {
    $items = $input; return $items;
}
function &emptyHashContainerReference(): array<string, int> { $items = []; return $items; }
$unique =& uniqueContainerReference(); $unique[] = 3;
$original = [4, 5]; $shared =& sharedContainerReference($original); $shared[] = 6;
$map =& emptyHashContainerReference(); $map['answer'] = 7;
echo count($unique), ':', json_encode($original), ':', count($shared), ':', json_encode($map);
unset($unique, $original, $shared, $map);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3:[4,5]:3:{\"answer\":7}");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval native calls preserve nullable-integer reference tags, hidden argument reads and variadics.
#[test]
fn test_eval_native_nullable_integer_reference_arguments_preserve_both_words() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function nativeMaybeReference(?int &$value, ?int $replacement): void {
    echo json_encode($value), ':'; $value = $replacement;
    echo json_encode(func_get_args()[0]), '|';
}
class NativeMaybeReferenceWriter {
    public function write(?int &$value, ?int $replacement): void {
        echo json_encode($value), ':'; $value = $replacement; echo json_encode($value), '|';
    }
}
function nativeMaybeReferenceVariadic(?int &...$values): void {
    $values[0] = null; $values['last'] = 9;
    echo json_encode($values[0]), ':', json_encode($values['last']), '|';
}
function runNativeMaybeReference(string $source): void {
    $value = null; $other = 4; $writer = new NativeMaybeReferenceWriter();
    eval($source);
    echo json_encode($value), ':', json_encode($other);
    unset($value, $other, $writer, $source);
}
$source = $argc > 0 ? 'nativeMaybeReference($value, 3); nativeMaybeReference($value, null); nativeMaybeReference($value, 7); $writer->write($value, null); $writer->write($value, 5); $writer->write($value, 6); nativeMaybeReferenceVariadic($value, last: $other);' : '';
runNativeMaybeReference($source); unset($source);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "null:3|3:null|null:7|7:null|null:5|5:6|null:9|null:9");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Typed reference payloads remain readable when nullable stores widen their original frame slot.
#[test]
fn test_concrete_reference_return_payloads_survive_final_frame_widening() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class WidenedReturnNode { public string $label = 'node'; }
class WidenedReturnHolder {
    public ?WidenedReturnNode $node = null;
    public ?string $text = null;
}
function &widenedReturnBefore(WidenedReturnHolder $holder): WidenedReturnNode {
    $value = $holder->node; $value = new WidenedReturnNode(); return $value;
}
function &widenedReturnAfter(WidenedReturnHolder $holder, bool $first): WidenedReturnNode {
    $value = new WidenedReturnNode(); if ($first) { return $value; }
    $value = $holder->node; $value = new WidenedReturnNode(); return $value;
}
function &widenedReturnText(WidenedReturnHolder $holder, bool $first): string {
    $value = 'early'; if ($first) { return $value; }
    $value = $holder->text; $value = 'late'; return $value;
}
$holder = new WidenedReturnHolder();
$before =& widenedReturnBefore($holder);
$early =& widenedReturnAfter($holder, $argc > 0);
$late =& widenedReturnAfter($holder, $argc < 0);
$textEarly =& widenedReturnText($holder, $argc > 0);
$textLate =& widenedReturnText($holder, $argc < 0);
$before->label = 'changed'; $textEarly .= '!';
echo $before->label, ':', $early->label, ':', $late->label, ':', $textEarly, ':', $textLate;
unset($before, $early, $late, $textEarly, $textLate, $holder);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "changed:node:node:early!:late");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An arity-selected reference method returns an independent value to an erased receiver call.
#[test]
fn test_erased_reference_method_candidates_copy_payloads_before_cell_retirement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ErasedReferenceCandidate {
    public string $text = 'before';
    public array $items = [1, 2];
    public int $number = 7;
    public stdClass $object;
    public function __construct() { $this->object = new stdClass(); $this->object->label = 'before'; }
    public function &readText(): string { return $this->text; }
    public function &readItems(): array { return $this->items; }
    public function &readNumber(): int { return $this->number; }
    public function &readObject(): stdClass { return $this->object; }
    public function change(): void {
        $this->text = 'after'; $this->items = [9]; $this->number = 8; $this->object->label = 'after';
    }
}
class ErasedReferenceOtherArity {
    public function readText(int $unused): mixed { return null; }
    public function readItems(int $unused): mixed { return null; }
    public function readNumber(int $unused): mixed { return null; }
    public function readObject(int $unused): mixed { return null; }
}
function snapshotErasedReferenceCandidate(mixed $holder): void {
    $text = $holder->readText(); $items = $holder->readItems();
    $number = $holder->readNumber(); $object = $holder->readObject();
    $holder->change();
    echo $text, ':', $holder->text, ':', json_encode($items), ':', json_encode($holder->items),
        ':', $number, ':', $holder->number, ':', $object->label;
    unset($holder, $text, $items, $number, $object);
}
$holder = new ErasedReferenceCandidate(); snapshotErasedReferenceCandidate($holder); unset($holder);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "before:after:[1,2]:[9]:7:8:after");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native method bridges update typed aliases even when they rewrite the imported box in place.
#[test]
fn test_eval_native_method_reference_writeback_dirties_typed_snapshots() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeDirtySnapshotBox {
    public string $text = 'a';
    public array $items = [1];
    public function &textRef(): string { return $this->text; }
    public function &itemsRef(): array { return $this->items; }
}
class NativeDirtySnapshotWriter {
    public function append(string &$text, array &$items): void { $text .= 'b'; $items[] = 2; }
    public static function appendStatic(string &$text, array &$items): void { $text .= 'c'; $items[] = 3; }
}
function nativeDirtySnapshotRun(string $source): void {
    $box = new NativeDirtySnapshotBox();
    $text = &$box->textRef();
    $items = &$box->itemsRef();
    $writer = new NativeDirtySnapshotWriter();
    eval($source);
    echo $text, ':', $box->text, ':', json_encode($items), ':', json_encode($box->items);
    unset($text, $items, $writer, $box, $source);
}
$source = $argc > 0 ? '$writer->append($text, $items); NativeDirtySnapshotWriter::appendStatic($text, $items);' : '';
nativeDirtySnapshotRun($source);
unset($source);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "abc:abc:[1,2,3]:[1,2,3]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Rejected assignments are catchable inside eval, and unset detaches typed property aliases.
#[test]
fn test_eval_typed_native_aliases_reject_invalid_values_and_detach_on_unset() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CheckedNativeScopeAliases {
    public stdClass $node;
    public string $text = 'original';
    public int $number = 7;
    public function __construct() { $this->node = new stdClass(); $this->node->label = 'node'; }
    public function &nodeRef(): stdClass { return $this->node; }
    public function &textRef(): string { return $this->text; }
    public function &numberRef(): int { return $this->number; }
}
function checkedNativeScopeAliases(bool $run): void {
    $box = new CheckedNativeScopeAliases();
    $node =& $box->nodeRef();
    $text =& $box->textRef();
    $number =& $box->numberRef();
    $source = $run ? 'try { $node = null; } catch (TypeError $error) { echo "null|"; } try { $node = 5; } catch (TypeError $error) { echo "int|"; } unset($error);' : '';
    eval($source);
    echo $node->label, ':', $box->node->label, '|';
    $source = $run ? '$replacement = new stdClass(); $replacement->label = "rebound"; unset($node); $node = $replacement; unset($replacement);' : '';
    eval($source);
    echo $node->label, ':', $box->node->label, '|';
    $source = $run ? 'unset($node, $text, $number);' : '';
    eval($source);
    echo $box->node->label, ':', $box->text, ':', $box->number, '|';
    unset($node, $text, $number, $box, $source);
}
checkedNativeScopeAliases($argc > 0);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "null|int|node:node|rebound:node|node:original:7|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval mutates the same boxed array group while by-value snapshots keep their contents.
#[test]
fn test_eval_array_reference_snapshots_reload_in_place_writes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function mutateNativeArrayReference(bool $run): void {
    $items = [1, 2];
    $snapshot = $items;
    $alias =& $items;
    $source = $run ? '$items[0] = 4; $items[] = 3;' : '';
    eval($source);
    echo json_encode($items), ':', json_encode($alias), ':', json_encode($snapshot), '|';
    $source = $run ? '$alias[1] += 5; unset($alias[0]);' : '';
    eval($source);
    echo json_encode($items), ':', json_encode($alias), ':', json_encode($snapshot), '|';
    unset($items, $alias, $snapshot, $source);
}

mutateNativeArrayReference($argc > 0);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[4,2,3]:[4,2,3]:[1,2]|{\"1\":7,\"2\":3}:{\"1\":7,\"2\":3}:[1,2]|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nullable integer aliases convert weak scalar assignments without retaining a borrowed string.
#[test]
fn test_eval_nullable_integer_alias_reload_preserves_only_integer_and_null_tags() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class EvalNullableIntegerAlias {
    public ?int $number = null;
    public function &numberRef(): ?int { return $this->number; }
}
$box = new EvalNullableIntegerAlias();
$number =& $box->numberRef();
$source = $argc > 0 ? '$number = "12";' : '';
eval($source);
echo $number + 1, ':', $box->number + 1, '|';
$source = $argc > 0 ? '$number = null;' : '';
eval($source);
var_dump($number, $box->number);
unset($number, $box, $source);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "13:13|NULL\nNULL\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Actual scope writes preserve raw locals and native aliases even with a full or colliding mask.
#[test]
fn test_eval_unaccessed_native_reference_aliases_keep_conditional_bindings() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ConditionalNativeAlias {
    public mixed $value = 'before';
    public function &valueRef(): mixed { return $this->value; }
}
function exerciseConditionalNativeAlias(bool $bind, bool $run): void {
    $box = new ConditionalNativeAlias();
    $value = 'raw';
    if ($bind) { $value =& $box->valueRef(); }
    $source = $run ? '$box->value = "updated";' : '';
    $K = 0;
    eval($source);
    echo $value, ':', $box->value, '|';
    eval($source);
    echo $value, ':', $box->value, '|';
    $source = $run ? '$K++;' : '';
    eval($source);
    echo $value, ':', $box->value, ':', $K, '|';
    $source = $run ? '$value = "by-name";' : '';
    eval($source);
    echo $value, ':', $box->value, '|';
    unset($value, $box, $source, $K);
}
exerciseConditionalNativeAlias(false, $argc > 0);
exerciseConditionalNativeAlias(true, $argc > 0);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "raw:updated|raw:updated|raw:updated:1|by-name:updated|updated:updated|updated:updated|updated:updated:1|by-name:by-name|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval writes preserve promoted native cell aliases and by-value array snapshots.
#[test]
fn test_eval_native_reference_property_payloads_and_aliases() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeEvalReferencePayloads {
    public mixed $value = null;
    public string $text = 'old';
    public int $number = 1;
    public float $weight = 1.5;
    public ?int $maybe = null;
    public function &valueRef(): mixed { return $this->value; }
    public function &textRef(): string { return $this->text; }
    public function &numberRef(): int { return $this->number; }
    public function &weightRef(): float { return $this->weight; }
    public function &maybeRef(): ?int { return $this->maybe; }
}
$box = new NativeEvalReferencePayloads();
$value = &$box->valueRef(); $text = &$box->textRef();
$number = &$box->numberRef(); $weight = &$box->weightRef(); $maybe = &$box->maybeRef();
$source = $argc > 0 ? '
$box->value = [1, 2]; $box->text = "new";
$box->number = 7; $box->weight = 2.5; $box->maybe = 3;
$copy = $box->value; $copy[0] = 9;
echo json_encode($box->value), ":", json_encode($copy), "|";
echo $box->text, ":", $box->number, ":", $box->weight, ":", $box->maybe, "|";
try { unset($box->value); }
catch (Error $error) { echo $error->getMessage(), "|"; }
unset($copy, $error);
' : '';
eval($source);
echo json_encode($value), ':', $text, ':', $number, ':', $weight, ':', $maybe;
$source = $argc > 0 ? '$text = "alias"; $number = 8; $weight = 3.5; $maybe = 4;' : '';
eval($source);
echo '|', $box->text, ':', $box->number, ':', $box->weight, ':', $box->maybe;
unset($box, $value, $text, $number, $weight, $maybe, $source);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[1,2]:[9,2]|new:7:2.5:3|Unsupported unset(): by-reference property|[1,2]:new:7:2.5:3|alias:8:3.5:4");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Scalar, array and object reference returns become owned eval values, including static calls.
#[test]
fn test_eval_native_reference_method_returns_copy_and_release_cells_bundle() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeEvalReferenceReturns {
    public int $number = 7;
    public float $decimal = 1.5;
    public string $text = 'text';
    public array $rows = [1, 2];
    public stdClass $node;
    public mixed $boxed;
    public static array $staticRows = [3, 4];
    public function __construct() {
        $this->node = new stdClass();
        $this->node->label = 'object';
        $this->boxed = $this->node;
    }
    public function &integerValue(): int { return $this->number; }
    public function &floatValue(): float { return $this->decimal; }
    public function &stringValue(): string { return $this->text; }
    public function &arrayValue(): array { return $this->rows; }
    public function &objectValue(): stdClass { return $this->node; }
    public function &mixedValue(): mixed { return $this->boxed; }
    public static function &staticValue(): array { $value = self::$staticRows; return $value; }
}
$receiver = new NativeEvalReferenceReturns();
$source = $argc > 0 ? '
$integer = $receiver->integerValue();
$float = $receiver->floatValue();
$string = $receiver->stringValue();
$array = $receiver->arrayValue();
$object = $receiver->objectValue();
$mixed = $receiver->mixedValue();
$static = NativeEvalReferenceReturns::staticValue();
unset($receiver);
echo $integer, "|", $float, "|", $string, "|", json_encode($array), "|";
echo $object->label, "|", $mixed->label, "|", json_encode($static), "|";
unset($integer, $float, $string, $array, $object, $mixed, $static);
' : '';
eval($source);
unset($source);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "7|1.5|text|[1,2]|object|object|[3,4]|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
