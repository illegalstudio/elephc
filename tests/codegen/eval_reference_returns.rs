//! Purpose:
//! Verifies eval reads values returned through native method reference cells.
//!
//! Called from:
//! - Executable codegen CI on every supported host target.
//!
//! Key details:
//! - Value copies outlive the receiver and retire the callee's transferred cell lease.

use crate::support::compile_and_run_with_heap_debug;

/// A conservative eval mask preserves raw locals and unaccessed live native aliases independently.
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
    eval($source);
    echo $value, ':', $box->value, '|';
    eval($source);
    echo $value, ':', $box->value, '|';
    unset($value, $box, $source);
}
exerciseConditionalNativeAlias(false, $argc > 0);
exerciseConditionalNativeAlias(true, $argc > 0);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "raw:updated|raw:updated|updated:updated|updated:updated|");
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
unset($box, $value, $text, $number, $weight, $maybe, $source);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[1,2]:[9,2]|new:7:2.5:3|Unsupported unset(): by-reference property|[1,2]:new:7:2.5:3");
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
