//! Purpose:
//! Regresses reference assignments through mixed enum interface implementations.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - PHP permits an implementation to add a reference return to a by-value contract.
//! - The selected method must decide whether the result aliases storage or is a copy.

use crate::support::*;

/// Value-returning branches cannot inherit another implementor's reference adoption.
#[test]
fn test_enum_reference_selection_review_mixed_returns() {
    let out = compile_and_run(r#"<?php
class ReturnStore { public int $value = 10; }
interface SelectedReturn { public function f(ReturnStore $store): int; }
enum AddedReturn implements SelectedReturn {
    case A; public function &f(ReturnStore $store): int { return $store->value; }
}
enum PlainReturn implements SelectedReturn {
    case A; public function f(ReturnStore $store): int { return $store->value + 1; }
}
function probe(SelectedReturn $target, ReturnStore $store): void {
    $result = &$target->f($store);
    echo $result, ':';
    $result = 40;
    echo $store->value, '|';
}
$store = new ReturnStore();
probe(PlainReturn::A, $store);
probe(AddedReturn::A, $store);
probe(PlainReturn::A, $store);
"#);
    assert_eq!(out, "11:10|10:40|41:40|");
}

/// Selected reference and copied-value branches agree without EIR optimization.
#[test]
fn test_enum_reference_selection_review_mixed_returns_no_ir_opt() {
    without_ir_opt(test_enum_reference_selection_review_mixed_returns);
}

/// New copy cells and transferred array cells balance through repeated mixed dispatch.
#[test]
fn test_enum_reference_selection_review_array_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ArrayReturnStore { public array $values = [1]; }
interface SelectedArrayReturn { public function f(ArrayReturnStore $store): array; }
enum AddedArrayReturn implements SelectedArrayReturn {
    case A; public function &f(ArrayReturnStore $store): array { return $store->values; }
}
enum PlainArrayReturn implements SelectedArrayReturn {
    case A; public function f(ArrayReturnStore $store): array { return [2]; }
}
function warm(SelectedArrayReturn $target): void { echo get_class($target), ':'; }
warm(AddedArrayReturn::A); warm(PlainArrayReturn::A); echo '|';
function probe(SelectedArrayReturn $target, ArrayReturnStore $store): void {
    $result = &$target->f($store);
    echo $result[0], ':';
    $result[0] = 40;
    echo $store->values[0], '|';
}
for ($index = 0; $index < 5; ++$index) {
    $store = new ArrayReturnStore();
    probe(PlainArrayReturn::A, $store);
    probe(AddedArrayReturn::A, $store);
    probe(PlainArrayReturn::A, $store);
    unset($store);
}
"#);
    assert_enum_owners(&out, &format!("AddedArrayReturn:PlainArrayReturn:|{}", "2:1|1:40|2:40|".repeat(5)));
}

/// Array copy/reference ownership does not depend on EIR optimization.
#[test]
fn test_enum_reference_selection_review_array_owners_no_ir_opt() {
    without_ir_opt(test_enum_reference_selection_review_array_owners);
}

/// A fresh copied result survives a throwing destructor while the old target retires.
#[test]
fn test_enum_reference_selection_review_throwing_binding() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SelectedReturnPayload {
    public function __construct(public string $label) {}
    public function __destruct() { echo $this->label; }
}
class ObjectReturnStore {
    public SelectedReturnPayload $value;
    public function __construct() { $this->value = new SelectedReturnPayload('H'); }
}
interface SelectedObjectReturn { public function f(ObjectReturnStore $store): SelectedReturnPayload; }
enum AddedObjectReturn implements SelectedObjectReturn {
    case A; public function &f(ObjectReturnStore $store): SelectedReturnPayload { return $store->value; }
}
enum PlainObjectReturn implements SelectedObjectReturn {
    case A; public function f(ObjectReturnStore $store): SelectedReturnPayload { return new SelectedReturnPayload('N'); }
}
class PreviousReturnBinding {
    public function __destruct() { echo 'X'; throw new Exception('previous'); }
}
function warm(SelectedObjectReturn $target): void { echo get_class($target), ':'; }
warm(AddedObjectReturn::A); warm(PlainObjectReturn::A); echo '|';
function probe(SelectedObjectReturn $target, ObjectReturnStore $store): void {
    $result = new PreviousReturnBinding();
    try { $result = &$target->f($store); echo 'bad'; }
    catch (Exception $error) { echo 'C:', $result->label, '|'; }
}
$store = new ObjectReturnStore();
for ($index = 0; $index < 3; ++$index) { probe(PlainObjectReturn::A, $store); }
unset($store);
"#);
    assert_enum_owners(&out, "AddedObjectReturn:PlainObjectReturn:|XC:N|NXC:N|NXC:N|NH");
}

/// Copied result unwind roots also survive without EIR optimization.
#[test]
fn test_enum_reference_selection_review_throwing_binding_no_ir_opt() {
    without_ir_opt(test_enum_reference_selection_review_throwing_binding);
}

/// A narrower copied return is widened before sharing the interface's boxed cell layout.
#[test]
fn test_enum_reference_selection_review_boxed_copies() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface SelectedMixedReturn { public function f(): mixed; }
enum AddedMixedReturn implements SelectedMixedReturn {
    case A; public function &f(): mixed { $value = 'a'; return $value; }
}
enum PlainStringReturn implements SelectedMixedReturn {
    case A; public function f(): string { return 'b'; }
}
function warm(SelectedMixedReturn $target): void { echo get_class($target), ':'; }
warm(AddedMixedReturn::A); warm(PlainStringReturn::A); echo '|';
function probe(SelectedMixedReturn $target): void {
    $result = &$target->f();
    echo gettype($result), ':', $result, '|';
    $result = 'changed';
}
for ($index = 0; $index < 3; ++$index) {
    probe(PlainStringReturn::A); probe(AddedMixedReturn::A); probe(PlainStringReturn::A);
}
"#);
    assert_enum_owners(&out, &format!("AddedMixedReturn:PlainStringReturn:|{}", "string:b|string:a|string:b|".repeat(3)));
}

/// Interface payload widening does not rely on EIR optimization.
#[test]
fn test_enum_reference_selection_review_boxed_copies_no_ir_opt() {
    without_ir_opt(test_enum_reference_selection_review_boxed_copies);
}

/// Only the two deliberately warmed enum singleton allocations remain after cleanup.
fn assert_enum_owners(out: &ProgramOutput, expected: &str) {
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, expected);
    let summary = out.stderr.lines().next().expect("heap summary");
    for field in ["live_blocks=2", "live_bytes=80"] {
        assert!(summary.split_whitespace().any(|part| part == field), "{}", out.stderr);
    }
}
