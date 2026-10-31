//! Purpose:
//! Checks ownership transfers from native iterator and JSON callback boundaries.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Fresh objects and strings transfer one owner; borrowed strings require a copy.

use crate::support::compile_and_run_with_heap_debug;

/// Foreach releases allocated keys and values while preserving borrowed key results.
#[test]
fn test_iterator_owned_and_borrowed_callback_results_heap_balance() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CallbackResultIterator implements Iterator {
    private int $position = 0;
    private int $copies;
    private string $label;
    public function __construct(int $copies) { $this->copies = $copies; $this->label = str_repeat('b', $copies); }
    public function rewind(): void { $this->position = 0; }
    public function valid(): bool { return $this->position < 2; }
    public function next(): void { $this->position++; }
    public function key(): string { return $this->position === 0 ? $this->label : str_repeat('o', $this->copies); }
    public function current(): stdClass { return (object)['value' => $this->position]; }
}
$iterator = new CallbackResultIterator($argc);
foreach ($iterator as $key => $value) { echo $key, $value->value; }
unset($key, $value, $iterator);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "b0o1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The JSON encoder retires the serializer's temporary result object after recursive encoding.
#[test]
fn test_json_owned_callback_result_heap_balance() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CallbackJsonProjection implements JsonSerializable {
    public function jsonSerialize(): mixed { return (object)['value' => str_repeat('x', 3)]; }
}
$object = new CallbackJsonProjection();
echo json_encode($object);
unset($object);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, r#"{"value":"xxx"}"#);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
