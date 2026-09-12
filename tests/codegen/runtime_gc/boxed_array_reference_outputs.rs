//! Purpose:
//! Verifies caller reads after declared PHP array reference parameters replace array layouts.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Named and callable calls must leave the same storage contract as direct calls.
//! - Conditional conversions must preserve both taken and skipped paths and COW aliases.

use crate::support::*;

/// Runtime index operands keep the detached array base across direct, named and first-class calls.
#[test]
fn test_core_runtime_index_reference_addresses_preserve_array_base_and_cow() {
    let source = r#"<?php
function runtimeReferenceIndex(): int { echo "i"; return 1; }
function replaceRuntimeReference(mixed &$value): void { $value = "updated"; }
function replaceRuntimeStringReference(string &$value): void { $value = "kept"; }
for ($i = 0; $i < 3; $i++) {
    $direct = [10, 20]; $directCopy = $direct;
    replaceRuntimeReference($direct[runtimeReferenceIndex()]);
    echo $direct[1], ":", $directCopy[1], "|";
    $named = [30, 40]; $namedCopy = $named;
    replaceRuntimeReference(value: $named[runtimeReferenceIndex()]);
    echo $named[1], ":", $namedCopy[1], "|";
    $fcc = [50, 60]; $fccCopy = $fcc;
    $callback = replaceRuntimeReference(...);
    $callback($fcc[runtimeReferenceIndex()]);
    echo $fcc[1], ":", $fccCopy[1], "|";
    $strings = ["first", str_repeat("s", 12)]; $stringsCopy = $strings;
    replaceRuntimeStringReference($strings[runtimeReferenceIndex()]);
    echo $strings[1], ":", $stringsCopy[1], "|";
    unset($direct, $directCopy, $named, $namedCopy, $fcc, $fccCopy, $callback, $strings, $stringsCopy);
}
"#;
    let expected = "iupdated:20|iupdated:40|iupdated:60|ikept:ssssssssssss|".repeat(3);
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Push separates boxed local and property cells while preserving reference aliases and value copies.
#[test]
fn test_core_boxed_array_push_publishes_local_and_property_owners() {
    let source = r#"<?php
class BoxedPushOwner {
    public array $items = [1];
    public static array $shared = [2];
}
function appendBoxedValue(array $items): array { array_push($items, 3); return $items; }
$owner = new BoxedPushOwner();
$reference = &$owner->items;
$snapshot = $owner->items;
array_push($owner->items, 4);
$push = array_push(...);
$push($reference, 5);
$staticSnapshot = BoxedPushOwner::$shared;
array_push(BoxedPushOwner::$shared, 6);
$original = ['key' => 7];
$returned = appendBoxedValue($original);
echo implode(',', $owner->items), '|', implode(',', $reference), '|', implode(',', $snapshot), '|';
echo implode(',', BoxedPushOwner::$shared), '|', implode(',', $staticSnapshot), '|';
echo implode(',', $returned), '|', implode(',', $original);
unset($owner, $reference, $snapshot, $staticSnapshot, $original, $returned);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1,4,5|1,4,5|1|2,6|2|7,3|7", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "1,4,5|1,4,5|1|2,6|2|7,3|7");
}

/// Named and first-class calls widen scalar element slots without changing earlier value copies.
#[test]
fn test_core_array_element_mixed_reference_named_and_callable_storage() {
    let source = r#"<?php
function replaceBoxedSlot(mixed &$value): void { $value = "updated"; }
$named = [1];
$namedCopy = $named;
replaceBoxedSlot(value: $named[0]);
$callable = replaceBoxedSlot(...);
$fcc = [2];
$fccCopy = $fcc;
$callable($fcc[0]);
echo $named[0], ':', $namedCopy[0], '|', $fcc[0], ':', $fccCopy[0];
unset($named, $namedCopy, $fcc, $fccCopy);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "updated:1|updated:2", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "updated:1|updated:2");
}

/// Reordered named places and spread prefixes evaluate once and mutate only the selected COW copy.
#[test]
fn test_core_named_element_references_preserve_source_order_and_spread_prefixes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function referenceIndex(): int { echo "i"; return 0; }
function referenceMark(string $label, int $value): int { echo $label; return $value; }
function replaceNamedElement(int $prefix, mixed &$value, int $suffix): void {
    echo $prefix, ":", $suffix, "|";
    $value = "changed";
}
class NamedElementWriter {
    public function write(int $prefix, mixed &$value, int $suffix): void {
        $value = "method";
    }
    public static function writeStatic(int $prefix, mixed &$value, int $suffix): void {
        $value = "static";
    }
}
$values = [1]; $copy = $values;
replaceNamedElement(suffix: referenceMark("s", 2), value: $values[referenceIndex()], prefix: referenceMark("p", 1));
echo $values[0], ":", $copy[0], "|";
$writer = new NamedElementWriter();
$method = [2]; $methodCopy = $method;
$writer->write(suffix: 2, value: $method[0], prefix: 1);
echo $method[0], ":", $methodCopy[0], "|";
$static = [3]; $staticCopy = $static;
NamedElementWriter::writeStatic(suffix: 2, value: $static[0], prefix: 1);
echo $static[0], ":", $staticCopy[0], "|";
$spread = [4]; $spreadCopy = $spread;
replaceNamedElement(...[1], value: $spread[referenceIndex()], suffix: referenceMark("s", 2));
echo $spread[0], ":", $spreadCopy[0];
unset($values, $copy, $writer, $method, $methodCopy, $static, $staticCopy, $spread, $spreadCopy);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "sip1:2|changed:1|method:2|static:3|is1:2|changed:4", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Element references carry addresses while their parent arrays own boxed, string and scalar values.
#[test]
fn test_core_array_element_reference_addresses_preserve_pointee_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function updateNestedArrayValue(array &$value): void { $value["added"] = 7; }
function replaceMixedElement(mixed &$value): void { $value = "changed"; }
function replaceStringElement(string &$value): void { $value = "new"; }
$nested = [[1]];
$nestedCopy = $nested;
updateNestedArrayValue($nested[0]);
echo $nested[0]["added"], ":", count($nestedCopy[0]), "|";
$numbers = [10, 20];
$numberCopy = $numbers;
replaceMixedElement($numbers[0]);
echo $numbers[0], ":", $numberCopy[0], "|";
$strings = [str_repeat("x", 8)];
$stringCopy = $strings;
replaceStringElement($strings[0]);
echo $strings[0], ":", $stringCopy[0];
unset($nested, $nestedCopy, $numbers, $numberCopy, $strings, $stringCopy);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "7:1|changed:10|new:xxxxxxxx", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Key results retain their runtime types after a hash reference widens only one of two value copies.
#[test]
fn test_core_native_php_array_reference_output_preserves_snapshot_key_types() {
    let source = r#"<?php
function addNumericArrayKey(array &$items): void { $items[7] = 42; }
$items = ["first" => 1, "second" => 2];
$snapshot = $items;
addNumericArrayKey($items);
$keys = array_keys($items);
$oldKeys = array_keys($snapshot);
echo implode(",", $keys), ":", gettype($keys[2]), "|";
echo implode(",", $oldKeys), ":", gettype($oldKeys[0]);
"#;
    assert_eq!(compile_and_run(source), "first,second,7:integer|first,second:string");
}

/// Reference iteration changes the property and its reference alias, never an earlier value copy.
#[test]
fn test_core_native_php_array_reference_property_iteration_preserves_value_copy() {
    let source = r#"<?php
class BoxedIterationProperty {
    public array $items = [1, "two"];
}
$owner = new BoxedIterationProperty();
$reference = &$owner->items;
$snapshot = $owner->items;
foreach ($owner->items as &$value) { $value = "v" . $value; }
unset($value);
echo implode(",", $owner->items), "|", implode(",", $reference), "|", implode(",", $snapshot);
"#;
    assert_eq!(compile_and_run(source), "v1,vtwo|v1,vtwo|1,two");
}

/// Every statically resolved call surface updates the caller's array key and value storage types.
#[test]
fn test_core_native_php_array_reference_output_call_surfaces() {
    let source = r#"<?php
function writeArrayKey(array &$items): void { $items["added"] = 7; }
class ReferenceArrayWriter {
    public function write(array &$items): void { $items["added"] = 8; }
    public static function writeStatic(array &$items): void { $items["added"] = 9; }
}
$named = [1];
writeArrayKey(items: $named);
echo implode(",", array_keys($named)), ":", implode(",", $named), "|";
$callback = writeArrayKey(...);
$callable = [2];
$callback($callable);
echo implode(",", array_keys($callable)), ":", implode(",", $callable), "|";
$writer = new ReferenceArrayWriter();
$method = [3];
$writer->write(items: $method);
echo implode(",", array_keys($method)), ":", implode(",", $method), "|";
$static = [4];
ReferenceArrayWriter::writeStatic($static);
echo implode(",", array_keys($static)), ":", implode(",", $static), "|";
$closure = function(array &$items): void { $items["added"] = 10; };
$captured = [5];
$closure($captured);
echo implode(",", array_keys($captured)), ":", implode(",", $captured);
"#;
    assert_eq!(compile_and_run(source), "0,added:1,7|0,added:2,7|0,added:3,8|0,added:4,9|0,added:5,10");
}

/// Both arms of short-circuit calls keep packed and keyed locals valid after reference boxing.
#[test]
fn test_core_native_php_array_reference_output_conditional_storage() {
    let source = r#"<?php
function conditionallyWriteArray(array &$items): bool { $items["added"] = 7; return true; }
function checkConditionalArray(bool $write): void {
    $packed = [1];
    $hash = ["old" => 2];
    $copy = $packed;
    $packedChanged = $write && conditionallyWriteArray($packed);
    $hashChanged = $write && conditionallyWriteArray($hash);
    echo implode(",", array_keys($packed)), ":", implode(",", $packed), "|";
    echo implode(",", array_keys($hash)), ":", implode(",", $hash), "|";
    echo implode(",", array_keys($copy)), ":", implode(",", $copy), ";";
}
checkConditionalArray($argc > 0);
checkConditionalArray($argc < 0);
"#;
    assert_eq!(compile_and_run(source), "0,added:1,7|old,added:2,7|0:1;0:1|old:2|0:1;");
}
