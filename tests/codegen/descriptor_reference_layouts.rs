//! Purpose:
//! Covers physical reference layouts selected by runtime callable dispatch.
//!
//! Called from:
//! - Executable codegen CI on each supported host target.
//!
//! Key details:
//! - Unknown signatures box fresh locals, while shared nullable slots reject incompatible writes.
//! - Every case requires balanced heap ownership after both successful and refused calls.

use crate::support::compile_and_run_with_heap_debug;

/// Ref-cell storebacks retire the helper's extra owner on both indexed and named-hash refusal.
#[test]
fn test_nullable_marker_ref_cell_storeback_refusals_retire_extra_destination_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RetiringNullableReferenceHolder { public ?int $value = null; }
function retiringNullableReferenceWrite(array<mixed> &$items): void {
    $key = array_key_first($items);
    try { $items[$key] = 'invalid'; }
    catch (Error $error) { echo 'refused:', json_encode($items[$key]), '|'; unset($error); }
    $items[$key] = 9; echo json_encode($items[$key]), '|';
    $items[$key] = null; echo json_encode($items[$key]), '|';
    if ($key === 0) {
        set_error_handler(function(int $severity, string $message, string $file, int $line): bool {
            echo 'warn|'; return true;
        });
        $key = json_decode('0.5');
        try { $items[$key] = 'invalid'; }
        catch (Error $error) { echo 'float-refused|'; unset($error); }
        restore_error_handler();
    }
}
function retiringNullableReferenceTarget(&...$items): void { retiringNullableReferenceWrite($items); }
function retiringNullableReferenceOther(&...$items): void { echo 'unexpected'; }
$holder = new RetiringNullableReferenceHolder(); $value =& $holder->value;
$target = $argc > 0 ? 'retiringNullableReferenceTarget' : 'retiringNullableReferenceOther';
$target($value); $target(last: $value);
unset($value, $holder, $target);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "refused:null|9|null|warn|float-refused|refused:null|9|null|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime Iterable references refuse boxed fresh locals but preserve existing raw Iterable aliases.
#[test]
fn test_unknown_descriptor_iterable_reference_layout_subset() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RuntimeIterableReferenceMethods {
    public function left(iterable &$items): void { echo 'raw:', count($items), '|'; }
    public function right(iterable &$items): void { echo 'raw:', count($items), '|'; }
}
class RuntimeIterableReferenceHolder { public iterable $items = [1, 2]; }
$writer = new RuntimeIterableReferenceMethods(); $method = $argc > 0 ? 'left' : 'right';
$array = [1, 2];
try { $writer->$method($array); }
catch (Error $error) { echo $error->getMessage(), ':array:', count($array), '|'; unset($error); }
$object = new ArrayObject([1, 2]);
try { $writer->$method($object); }
catch (Error $error) { echo $error->getMessage(), ':object:', count($object), '|'; unset($error); }
$holder = new RuntimeIterableReferenceHolder(); $alias =& $holder->items;
$writer->$method($alias);
$holder->items = new ArrayObject([1, 2, 3]); $writer->$method($alias);
unset($alias, $holder, $array, $object, $writer, $method);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    let message = "Unsupported reference argument: caller storage does not match the selected parameter layout";
    assert_eq!(out.stdout, format!("{message}:array:2|{message}:object:2|raw:2|raw:3|"));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A boxed runtime key preserves packed-marker owners on refusal and valid scalar writeback.
#[test]
fn test_unknown_descriptor_nullable_reference_mixed_key_write_keeps_destination_owner() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class MixedKeyNullableProperty { public ?int $value = null; }
function mixedKeyNullableReplacement(): mixed { return 'invalid'; }
function mixedKeyNullableWriter(MixedKeyNullableProperty $holder, &...$items): void {
    $key = array_key_first($items); $copy = $items;
    try { $items[$key] = mixedKeyNullableReplacement(); }
    catch (Error $error) {
        echo $error->getMessage(), ':', json_encode($items[$key]), ':', count($items), ':', count($copy), '|';
        unset($error);
    }
    $key = json_decode('"0"');
    try { $items[$key] = mixedKeyNullableReplacement(); }
    catch (Error $error) {
        echo 'numeric-string:', json_encode($holder->value), ':', count($items), ':', count($copy), '|';
        unset($error);
    }
    $items[$key] = 7;
    echo json_encode($holder->value), ':', json_encode($copy[$key]), '|';
    $items[$key] = null; echo json_encode($holder->value);
    unset($copy, $key);
}
function mixedKeyNullableOther(MixedKeyNullableProperty $holder, &...$items): void { echo 'unexpected'; }
$holder = new MixedKeyNullableProperty(); $value =& $holder->value;
$target = $argc > 0 ? 'mixedKeyNullableWriter' : 'mixedKeyNullableOther';
$target($holder, $value);
unset($value, $holder, $target);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Unsupported assignment through a nullable integer reference: value must be int or null:null:1:1|numeric-string:null:1:1|7:7|null");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Unknown function names preserve positional and named reference retyping through variadics.
#[test]
fn test_unknown_descriptor_reference_arguments_widen_fresh_locals() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function unknownReferenceLeft(&...$items): void { $items[0] = 'position'; $items['last'] = 'named'; }
function unknownReferenceRight(&...$items): void { $items[0] = 'position'; $items['last'] = 'named'; }
class UnknownReferenceMethods {
    public function left(&$first, &$second): void { $first = 'method'; $second = 'reference'; }
    public function right(&$first, &$second): void { $first = 'method'; $second = 'reference'; }
}
$target = $argc > 0 ? 'unknownReferenceLeft' : 'unknownReferenceRight';
$first = 1; $second = null;
$target($first, last: $second);
echo $first, ':', $second;
$writer = new UnknownReferenceMethods(); $method = $argc > 0 ? 'left' : 'right';
$third = 3; $fourth = null; $writer->$method($third, $fourth);
echo '|', $third, ':', $fourth;
unset($first, $second, $third, $fourth, $target, $writer, $method);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "position:named|method:reference");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime selection refuses incompatible boxed and inline reference layouts before mutation.
#[test]
fn test_unknown_descriptor_nullable_reference_layout_mismatch_is_catchable() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function unknownNullableLeft(?int &$value): void { $value = 3; }
function unknownNullableRight(?int &$value): void { $value = 4; }
$target = $argc > 0 ? 'unknownNullableLeft' : 'unknownNullableRight';
$value = null;
try { $target($value); echo 'unexpected'; }
catch (Error $error) {
    echo $error->getMessage(), ':', json_encode($value); unset($error);
}
unset($value, $target);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Unsupported reference argument: caller storage does not match the selected parameter layout:null");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Indexed and named variadic writes cannot corrupt an existing nullable-integer property cell.
#[test]
fn test_unknown_descriptor_variadic_nullable_reference_writes_validate_tags() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class DescriptorNullableProperty { public ?int $value = null; }
function invalidPositionalScalar(&...$items): void { $items[0] = 'invalid'; }
function invalidNamedScalar(&...$items): void { $items['last'] = 'invalid'; }
function descriptorReplacementMixed(): mixed { return 'boxed-invalid'; }
function invalidPositionalBox(&...$items): void { $items[0] = descriptorReplacementMixed(); }
function invalidNamedBox(&...$items): void {
    $copy = $items;
    try { $items['last'] = descriptorReplacementMixed(); }
    finally { echo 'copy:', count($copy), '|'; unset($copy); }
}
function validPositionalScalar(&...$items): void { $items[0] = 7; }
function validNamedScalar(&...$items): void { $items['last'] = null; }
$holder = new DescriptorNullableProperty(); $value =& $holder->value;
$positional = $argc > 0 ? 'invalidPositionalScalar' : 'invalidPositionalBox';
try { $positional($value); } catch (Error $error) { echo $error->getMessage(), '|'; unset($error); }
$named = $argc > 0 ? 'invalidNamedScalar' : 'invalidNamedBox';
try { $named(last: $value); } catch (Error $error) { echo $error->getMessage(), '|'; unset($error); }
$positional = $argc > 0 ? 'invalidPositionalBox' : 'invalidPositionalScalar';
try { $positional($value); } catch (Error $error) { echo $error->getMessage(), '|'; unset($error); }
$named = $argc > 0 ? 'invalidNamedBox' : 'invalidNamedScalar';
try { $named(last: $value); } catch (Error $error) { echo $error->getMessage(), '|'; unset($error); }
echo json_encode($holder->value), '|';
$positional = $argc > 0 ? 'validPositionalScalar' : 'validNamedScalar';
$positional($value); echo json_encode($holder->value), '|';
$named = $argc > 0 ? 'validNamedScalar' : 'validPositionalScalar';
$named(last: $value); echo json_encode($holder->value);
unset($value, $holder, $positional, $named);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    let message = "Unsupported assignment through a nullable integer reference: value must be int or null|";
    assert_eq!(out.stdout, format!("{}copy:1|{message}null|7|null", message.repeat(3)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
