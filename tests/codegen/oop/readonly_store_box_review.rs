//! Purpose:
//! Checks readonly first-write storage and boxed-source ownership across property shapes.
//!
//! Called from:
//! - The OOP codegen integration test module.
//!
//! Key details:
//! - Successful stores must publish the declared representation and retire each source once.
//! - Rejected first writes stay uninitialized and release source owners during unwinding.

use crate::support::*;

/// Weak union conversions retire the original source while storing the converted value.
#[test]
fn test_readonly_store_box_review_weak_union_conversions() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class UnionBox { public public(set) readonly int|string $value; }
class TextBox { public public(set) readonly ?string $value; }
for ($i = 0; $i < 16; $i++) {
    $union = new UnionBox(); $union->value = true;
    $text = new TextBox(); $text->value = 7;
    echo $union->value, $text->value, "|"; unset($union, $text);
}
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "17|".repeat(16));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// The shared policy also preserves mutable defaults, named writes and runtime-name writes.
#[test]
fn test_readonly_store_box_review_mutable_store_controls() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box { public mixed $value = [1]; }
function fresh(int $width): mixed { return [str_repeat("x", $width)]; }
function write(mixed $box, string $name, int $width): void { $box->{$name} = fresh($width); }
for ($i = 0; $i < 16; $i++) {
    $box = new Box(); echo $box->value[0];
    $box->value = fresh($argc); echo $box->value[0];
    write($box, "value", $argc); echo $box->value[0], "|"; unset($box);
}
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1xx|".repeat(16));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Weak union conversions keep identical source ownership without EIR optimization.
#[test]
fn test_readonly_store_box_review_weak_union_conversions_no_ir_opt() {
    without_ir_opt(test_readonly_store_box_review_weak_union_conversions);
}

/// Mutable callers of the shared cleanup policy work without EIR optimization.
#[test]
fn test_readonly_store_box_review_mutable_store_controls_no_ir_opt() {
    without_ir_opt(test_readonly_store_box_review_mutable_store_controls);
}

/// Explicit indexed-array storage must not receive the address of a freed Mixed wrapper.
#[test]
fn test_readonly_store_box_review_concrete_array_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box { public readonly array<int> $items; public function __construct() { $this->items = [1]; } }
for ($i = 0; $i < 16; $i++) {
    $box = new Box(); echo $box->items[0], "|"; unset($box);
}
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1|".repeat(16));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Concrete indexed storage has the same ownership without EIR optimization.
#[test]
fn test_readonly_store_box_review_concrete_array_storage_no_ir_opt() {
    without_ir_opt(test_readonly_store_box_review_concrete_array_storage);
}

/// Concrete indexed and associative arrays remain live after their first readonly store.
#[test]
fn test_readonly_store_box_review_array_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class IndexedBox { public readonly array $items; public function __construct() { $this->items = [1]; } }
class AssocBox { public readonly array $items; public function __construct() { $this->items = ["k" => 2]; } }
for ($i = 0; $i < 16; $i++) {
    $indexed = new IndexedBox(); $assoc = new AssocBox();
    echo $indexed->items[0], $assoc->items["k"], "|";
    unset($indexed, $assoc);
}
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "12|".repeat(16));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Mixed-shaped slots retain the published value without abandoning its temporary box.
#[test]
fn test_readonly_store_box_review_mixed_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class MixedBox { public readonly mixed $value; public function __construct() { $this->value = 1; } }
class NullableTextBox { public readonly ?string $value; public function __construct() { $this->value = str_repeat("x", 2); } }
for ($i = 0; $i < 16; $i++) {
    $mixed = new MixedBox(); $text = new NullableTextBox();
    echo $mixed->value, $text->value, "|"; unset($mixed, $text);
}
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1xx|".repeat(16));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Object-typed slots own their payload without retaining a discarded source box.
#[test]
fn test_readonly_store_box_review_object_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Value { public int $number = 3; }
class NamedBox { public readonly Value $value; public function __construct() { $this->value = new Value(); } }
class ObjectBox { public readonly object $value; public function __construct() { $this->value = new Value(); } }
for ($i = 0; $i < 16; $i++) {
    $named = new NamedBox(); $object = new ObjectBox();
    echo $named->value->number, $object->value instanceof Value ? "O|" : "bad";
    unset($named, $object);
}
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3O|".repeat(16));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A rejecting union guard retires the first-write source and leaves its slot uninitialized.
#[test]
fn test_readonly_store_box_review_rejected_union_source() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box { public public(set) readonly ?string $value; }
for ($i = 0; $i < 16; $i++) {
    $box = new Box();
    try { $box->value = [str_repeat("x", 2)]; }
    catch (TypeError $error) { echo "T"; unset($error); }
    echo isset($box->value) ? "bad" : "U|"; unset($box);
}
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "TU|".repeat(16));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Array storage ownership is independent of EIR optimization.
#[test]
fn test_readonly_store_box_review_array_storage_no_ir_opt() {
    without_ir_opt(test_readonly_store_box_review_array_storage);
}

/// Mixed storage ownership is independent of EIR optimization.
#[test]
fn test_readonly_store_box_review_mixed_storage_no_ir_opt() {
    without_ir_opt(test_readonly_store_box_review_mixed_storage);
}

/// Object storage ownership is independent of EIR optimization.
#[test]
fn test_readonly_store_box_review_object_storage_no_ir_opt() {
    without_ir_opt(test_readonly_store_box_review_object_storage);
}

/// Rejecting union guards unwind identically without EIR optimization.
#[test]
fn test_readonly_store_box_review_rejected_union_source_no_ir_opt() {
    without_ir_opt(test_readonly_store_box_review_rejected_union_source);
}
