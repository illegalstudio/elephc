//! Purpose:
//! Verifies eval reads values returned through native method reference cells.
//!
//! Called from:
//! - Executable codegen CI on every supported host target.
//!
//! Key details:
//! - Value copies outlive the receiver and retire the callee's transferred cell lease.

use crate::support::compile_and_run_with_heap_debug;

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
    public static function &staticValue(): array { return self::$staticRows; }
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
