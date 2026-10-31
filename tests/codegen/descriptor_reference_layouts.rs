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

/// Copies of reference variadics retain their destination owner after named or packed refusal.
#[test]
fn test_nullable_variadic_copy_refusal_preserves_named_and_positional_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CopiedNullableReferenceHolder { public ?int $value = null; }
function copiedNullableReferenceTarget(&...$items): void {
    $copy = $items; $key = array_key_first($items);
    try {
        if ($key === 'last') { $copy['last'] = 'invalid'; }
        else { $copy[0] = 'invalid'; }
    } catch (Error $error) { echo 'refused|'; unset($error); }
    echo count($copy), ':', count($items), ':', json_encode($copy[$key]), '|';
    if ($key === 'last') { $copy['last'] = 7; } else { $copy[0] = 7; }
    echo json_encode($copy[$key]), ':', json_encode($items[$key]), '|';
    $copy['extra'] = 'kept'; echo count($copy), ':', count($items), '|';
    unset($copy, $key);
}
function copiedNullableReferenceOther(&...$items): void { echo 'unexpected'; }
$holder = new CopiedNullableReferenceHolder(); $value =& $holder->value;
$target = $argc > 0 ? 'copiedNullableReferenceTarget' : 'copiedNullableReferenceOther';
$target(last: $value); $holder->value = null; $target($value);
unset($value, $holder, $target);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "refused|1:1:null|7:7|2:1|refused|1:1:null|7:7|2:1|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A rejected fresh object's throwing destructor keeps the original nullable-reference refusal.
#[test]
fn test_nullable_reference_refusal_preserves_error_during_throwing_replacement_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingNullableReferenceHolder { public ?int $value = null; }
class ThrowingNullableReferenceReplacement {
    public function __destruct() { throw new Exception('cleanup'); }
}
function throwingNullableReferenceTarget(&...$items): void {
    try { $items['last'] = new ThrowingNullableReferenceReplacement(); }
    catch (Throwable $error) {
        echo $error->getMessage(), '|';
        $previous = $error->getPrevious();
        if ($previous !== null) { echo $previous->getMessage(), '|'; } else { echo 'missing|'; }
        echo json_encode($items['last']);
        unset($previous, $error);
    }
}
function throwingNullableReferenceOther(&...$items): void { echo 'unexpected'; }
function throwingPackedNullableReferenceTarget(&...$items): void {
    try { $items[0] = new ThrowingNullableReferenceReplacement(); }
    catch (Throwable $error) {
        echo $error->getMessage(), '|'; $previous = $error->getPrevious();
        if ($previous !== null) { echo $previous->getMessage(), '|'; } else { echo 'missing|'; }
        echo json_encode($items[0]); unset($previous, $error);
    }
}
$holder = new ThrowingNullableReferenceHolder(); $value =& $holder->value;
$target = $argc > 0 ? 'throwingNullableReferenceTarget' : 'throwingNullableReferenceOther';
$target(last: $value); echo '|';
$target = $argc > 0 ? 'throwingPackedNullableReferenceTarget' : 'throwingNullableReferenceOther';
$target($value); unset($target, $value, $holder);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "cleanup|Unsupported assignment through a nullable integer reference: value must be int or null|null|cleanup|Unsupported assignment through a nullable integer reference: value must be int or null|null");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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
    public function left(iterable &$items): void {
        $count = 0; foreach ($items as $unused) { $count++; } echo 'raw:', $count, '|';
    }
    public function right(iterable &$items): void {
        $count = 0; foreach ($items as $unused) { $count++; } echo 'raw:', $count, '|';
    }
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

/// Unknown descriptor calls adapt boxed object references only after a runtime class match.
#[test]
fn test_unknown_descriptor_named_object_reference_validates_runtime_class() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RuntimeObjectReferenceValue {
    public function __construct(public string $name) {}
}
class RuntimeObjectReferenceChild extends RuntimeObjectReferenceValue {}
class RuntimeObjectReferenceWrong {}
interface RuntimeObjectReferenceContract { public function label(): string; }
class RuntimeObjectReferenceImplementation implements RuntimeObjectReferenceContract {
    public function __construct(public string $name) {}
    public function label(): string { return $this->name; }
}
function runtimeObjectReferenceLeft(RuntimeObjectReferenceValue &$value): void {
    echo 'accepted:', $value->name, '|';
    $value = new RuntimeObjectReferenceValue('left');
}
function runtimeObjectReferenceRight(RuntimeObjectReferenceValue &$value): void {
    echo 'accepted:', $value->name, '|';
    $value = new RuntimeObjectReferenceValue('right');
}
function runtimeObjectReferenceInterfaceLeft(RuntimeObjectReferenceContract &$value): void {
    echo 'interface:', $value->label(), '|';
    $value = new RuntimeObjectReferenceImplementation('interface-left');
}
function runtimeObjectReferenceInterfaceRight(RuntimeObjectReferenceContract &$value): void {
    echo 'interface:', $value->label(), '|';
    $value = new RuntimeObjectReferenceImplementation('interface-right');
}
function runtimeObjectReferenceVariadic(RuntimeObjectReferenceValue &...$values): void {
    echo 'variadic:', $values[0]->name, '|';
    $values[0] = new RuntimeObjectReferenceValue('variadic-changed');
}
class RuntimeObjectReferenceMethods {
    public function left(RuntimeObjectReferenceValue &$value): void {
        $value = new RuntimeObjectReferenceValue('method-left');
    }
    public function right(RuntimeObjectReferenceValue &$value): void {
        $value = new RuntimeObjectReferenceValue('method-right');
    }
    public static function staticLeft(RuntimeObjectReferenceValue &$value): void {
        $value = new RuntimeObjectReferenceValue('static-left');
    }
    public static function staticRight(RuntimeObjectReferenceValue &$value): void {
        $value = new RuntimeObjectReferenceValue('static-right');
    }
}
$target = $argc > 0 ? 'runtimeObjectReferenceLeft' : 'runtimeObjectReferenceRight';
$value = new RuntimeObjectReferenceValue('start');
$target($value); echo 'changed:', $value->name, '|';
$child = new RuntimeObjectReferenceChild('child');
$target($child); echo 'subclass:', $child->name, '|';
$interfaceTarget = $argc > 0 ? 'runtimeObjectReferenceInterfaceLeft' : 'runtimeObjectReferenceInterfaceRight';
$implementation = new RuntimeObjectReferenceImplementation('contract');
$interfaceTarget($implementation); echo 'interface-changed:', $implementation->label(), '|';
$variadicTarget = $argc > 0 ? 'runtimeObjectReferenceVariadic' : 'runtimeObjectReferenceUnused';
$variadic = new RuntimeObjectReferenceChild('variadic-child');
$variadicTarget($variadic); echo 'variadic-result:', $variadic->name, '|';
$wrong = new RuntimeObjectReferenceWrong();
try { $target($wrong); echo 'unexpected'; }
catch (TypeError $error) { echo $error->getMessage(), ':wrong|'; unset($error); }
try { $target(value: $wrong); echo 'unexpected'; }
catch (TypeError $error) { echo $error->getMessage(), ':named|'; unset($error); }
try { $variadicTarget($wrong); echo 'unexpected'; }
catch (TypeError $error) { echo $error->getMessage(), ':variadic|'; unset($error); }
$methods = new RuntimeObjectReferenceMethods();
$method = $argc > 0 ? 'left' : 'right';
try { $methods->$method($wrong); echo 'unexpected'; }
catch (TypeError $error) { echo $error->getMessage(), ':method|'; unset($error); }
$staticMethod = $argc > 0 ? 'staticLeft' : 'staticRight';
try { RuntimeObjectReferenceMethods::$staticMethod($wrong); echo 'unexpected'; }
catch (TypeError $error) { echo $error->getMessage(), ':static'; unset($error); }
unset($implementation, $interfaceTarget, $variadicTarget, $variadic, $child, $wrong, $value, $target, $methods, $method, $staticMethod);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "accepted:start|changed:left|accepted:child|subclass:left|interface:contract|interface-changed:interface-left|variadic:variadic-child|variadic-result:variadic-changed|runtimeObjectReferenceLeft(): Argument #1 ($value) must be of type RuntimeObjectReferenceValue, RuntimeObjectReferenceWrong given:wrong|runtimeObjectReferenceLeft(): Argument #1 ($value) must be of type RuntimeObjectReferenceValue, RuntimeObjectReferenceWrong given:named|runtimeObjectReferenceVariadic(): Argument #1 must be of type RuntimeObjectReferenceValue, RuntimeObjectReferenceWrong given:variadic|RuntimeObjectReferenceMethods::left(): Argument #1 ($value) must be of type RuntimeObjectReferenceValue, RuntimeObjectReferenceWrong given:method|RuntimeObjectReferenceMethods::staticLeft(): Argument #1 ($value) must be of type RuntimeObjectReferenceValue, RuntimeObjectReferenceWrong given:static",
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Closure object references reject incompatible boxed and direct marker layouts.
#[test]
fn test_unknown_descriptor_closure_object_reference_guards_relative_and_direct_sources() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RuntimeClosureReferenceOwner {
    public function __construct(public string $name) {}
    public static function callback(): callable {
        return function(RuntimeClosureReferenceOwner &$value): void {
            echo 'self:', $value->name, '|';
            $value = new RuntimeClosureReferenceOwner('changed');
        };
    }
}
class RuntimeClosureReferenceOther {}
function runtimeClosureReferenceOther(mixed &$value): void {}
class RuntimeClosureReferenceHolder {
    public int $number = 4;
    public RuntimeClosureReferenceOther $other;
    public function __construct() { $this->other = new RuntimeClosureReferenceOther(); }
}
$callback = $argc > 0
    ? RuntimeClosureReferenceOwner::callback()
    : 'runtimeClosureReferenceOther';
$value = new RuntimeClosureReferenceOwner('start');
$callback($value); echo 'changed:', $value->name, '|';
$other = new RuntimeClosureReferenceOther();
try { $callback($other); echo 'unexpected'; }
catch (TypeError $error) { echo 'boxed-class|'; unset($error); }
$holder = new RuntimeClosureReferenceHolder();
$number =& $holder->number;
try { $callback($number); echo 'unexpected'; }
catch (Error $error) { echo 'direct-layout|'; unset($error); }
$otherAlias =& $holder->other;
try { $callback($otherAlias); echo 'unexpected'; }
catch (TypeError $error) { echo 'direct-class'; unset($error); }
unset($otherAlias, $number, $holder, $other, $value, $callback);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "self:start|changed:changed|boxed-class|direct-layout|direct-class");
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
