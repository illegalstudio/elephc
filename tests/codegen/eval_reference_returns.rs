//! Purpose:
//! Verifies eval reads values returned through native method reference cells.
//!
//! Called from:
//! - Executable codegen CI on every supported host target.
//!
//! Key details:
//! - Value copies outlive the receiver and retire the callee's transferred cell lease.

use crate::support::compile_and_run_with_heap_debug;

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
