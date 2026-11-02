//! Purpose:
//! Rejects arguments not accepted by the selected enum implementation of an interface.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - One implementation deliberately widens the interface parameter to mixed.
//! - Source effects must finish before parameter guards, without losing reference identity.

use crate::support::*;

/// Allows only the two persistent native enum singletons, not argument or exception leaks.
fn assert_enum_heap_baseline(stderr: &str) {
    let summary = stderr.lines().next().expect("heap debug summary");
    assert!(summary.split_whitespace().any(|field| field == "live_blocks=2"), "{stderr}");
    assert!(summary.split_whitespace().any(|field| field == "live_bytes=80"), "{stderr}");
}

/// Checks selected parameter types after positional, named and static-unpack source effects.
#[test]
fn test_enum_selected_type_review_source_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface SelectedType { public function f(int $a, int $b = 0): int; }
enum LooseType implements SelectedType {
    case A; public function f(mixed $a, mixed $b = 0): int { return 11; }
}
enum StrictType implements SelectedType {
    case A; public function f(int $a, int $b = 0): int { return $a + $b; }
}
function warmSelected(SelectedType $value): void { echo get_class($value), ':'; }
warmSelected(LooseType::A); warmSelected(StrictType::A); echo '|';
function badSelected(): string { echo 'a'; return str_repeat('n', 2); }
function tailSelected(): int { echo 'b'; return 2; }
function invokeSelected(SelectedType $value): void {
    try { echo $value->f(badSelected(), tailSelected()); }
    catch (TypeError $error) { echo $error->getMessage(); } echo '|';
    try { echo $value->f(b: tailSelected(), a: badSelected()); }
    catch (TypeError $error) { echo $error->getMessage(); } echo '|';
    try { echo $value->f(...['b' => tailSelected(), 'a' => badSelected()]); }
    catch (TypeError $error) { echo $error->getMessage(); } echo '|';
    try { echo $value->f(...[badSelected(), tailSelected()]); }
    catch (TypeError $error) { echo $error->getMessage(); } echo '|';
}
invokeSelected(LooseType::A); invokeSelected(StrictType::A);
"#);
    assert!(out.success, "{}", out.stderr);
    let error = "StrictType::f(): Argument #1 ($a) must be of type int, string given";
    assert_eq!(out.stdout, format!("LooseType:StrictType:|ab11|ba11|ba11|ab11|ab{error}|ba{error}|ba{error}|ab{error}|"));
    assert_enum_heap_baseline(&out.stderr);
}

/// Selected parameter rejection does not depend on EIR optimization.
#[test]
fn test_enum_selected_type_review_source_order_no_ir_opt() {
    without_ir_opt(test_enum_selected_type_review_source_order);
}

/// Strict callers keep numeric strings unconverted until the selected type guard rejects them.
#[test]
fn test_enum_selected_type_review_strict_scalar() {
    let out = compile_and_run(r#"<?php declare(strict_types=1);
interface StrictSelected { public function f(int $a): int; }
enum LooseSelected implements StrictSelected {
    case A; public function f(mixed $a): int { return 11; }
}
enum TightSelected implements StrictSelected {
    case A; public function f(int $a): int { return $a; }
}
function selectedNumber(): string { return str_repeat('7', 1); }
function invokeStrictSelected(StrictSelected $value): void {
    try { echo $value->f(selectedNumber()); }
    catch (TypeError $error) { echo $error->getMessage(); } echo '|';
}
invokeStrictSelected(LooseSelected::A); invokeStrictSelected(TightSelected::A);
"#);
    assert_eq!(out, "11|TightSelected::f(): Argument #1 ($a) must be of type int, string given|");
}

/// Strict selected binding retains its source type without EIR optimization.
#[test]
fn test_enum_selected_type_review_strict_scalar_no_ir_opt() {
    without_ir_opt(test_enum_selected_type_review_strict_scalar);
}

/// Reference returns retain their managed storage while selected value arguments are checked.
#[test]
fn test_enum_selected_type_review_references() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface ReferenceSelected { public function &f(int $a): int; }
enum ReferenceLoose implements ReferenceSelected {
    case A; public function &f(mixed $a): int { $answer = 11; return $answer; }
}
enum ReferenceTight implements ReferenceSelected {
    case A; public function &f(int $a): int { return $a; }
}
function warmReference(ReferenceSelected $value): void { echo get_class($value), ':'; }
warmReference(ReferenceLoose::A); warmReference(ReferenceTight::A); echo '|';
function selectedReferenceValue(): string { return str_repeat('n', 2); }
function invokeReferenceSelected(ReferenceSelected $value): void {
    $anchor = 20; $result = &$anchor;
    try {
        $result = &$value->f(selectedReferenceValue());
        echo $result, ':'; ++$result; echo $result;
    }
    catch (TypeError $error) { echo $error->getMessage(), ':', $result; }
    echo '|';
    $valid = &$value->f(7); echo $valid, ':'; ++$valid; echo $valid, '|';
}
invokeReferenceSelected(ReferenceLoose::A); invokeReferenceSelected(ReferenceTight::A);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ReferenceLoose:ReferenceTight:|11:12|11:12|ReferenceTight::f(): Argument #1 ($a) must be of type int, string given:20|7:8|");
    assert_enum_heap_baseline(&out.stderr);
}

/// Selected reference binding remains checked without EIR optimization.
#[test]
fn test_enum_selected_type_review_references_no_ir_opt() {
    without_ir_opt(test_enum_selected_type_review_references);
}

/// Weak accepted scalars are converted only after the selected implementation accepts them.
#[test]
fn test_enum_selected_type_review_weak_scalars() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface WeakSelected { public function f(int $a): int; }
enum WeakLoose implements WeakSelected {
    case A; public function f(mixed $a): int { return 11; }
}
enum WeakTight implements WeakSelected {
    case A; public function f(int $a): int { return $a; }
}
function warmWeak(WeakSelected $value): void { echo get_class($value), ':'; }
warmWeak(WeakLoose::A); warmWeak(WeakTight::A); echo '|';
function weakSelectedNumber(): string { return str_repeat('7', 1); }
function invokeWeakSelected(WeakSelected $value): void {
    echo $value->f(weakSelectedNumber()), '|';
}
invokeWeakSelected(WeakLoose::A); invokeWeakSelected(WeakTight::A);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "WeakLoose:WeakTight:|11|7|");
    assert_enum_heap_baseline(&out.stderr);
}

/// Weak binding produces the same scalar ABI without EIR optimization.
#[test]
fn test_enum_selected_type_review_weak_scalars_no_ir_opt() {
    without_ir_opt(test_enum_selected_type_review_weak_scalars);
}

/// An unrelated reference parameter must not disable checks on a by-value parameter.
#[test]
fn test_enum_selected_type_review_reference_parameter() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface RefParameterSelected { public function f(int &$out, int $a): int; }
enum RefParameterLoose implements RefParameterSelected {
    case A; public function f(int &$out, mixed $a): int { ++$out; return 11; }
}
enum RefParameterTight implements RefParameterSelected {
    case A; public function f(int &$out, int $a): int { ++$out; return $a; }
}
function warmParameter(RefParameterSelected $value): void { echo get_class($value), ':'; }
warmParameter(RefParameterLoose::A); warmParameter(RefParameterTight::A); echo '|';
function refParameterValue(): string { echo 'a'; return str_repeat('n', 2); }
function invokeRefParameterSelected(RefParameterSelected $value): void {
    $out = 2;
    try { echo $value->f($out, refParameterValue()); }
    catch (TypeError $error) { echo $error->getMessage(); }
    echo ':', $out, '|';
    echo $value->f(a: 7, out: $out), ':', $out, '|';
}
invokeRefParameterSelected(RefParameterLoose::A); invokeRefParameterSelected(RefParameterTight::A);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "RefParameterLoose:RefParameterTight:|a11:3|11:4|aRefParameterTight::f(): Argument #2 ($a) must be of type int, string given:2|7:3|");
    assert_enum_heap_baseline(&out.stderr);
}

/// Reference-place identity and selected value guards do not depend on EIR optimization.
#[test]
fn test_enum_selected_type_review_reference_parameter_no_ir_opt() {
    without_ir_opt(test_enum_selected_type_review_reference_parameter);
}

/// Earlier type errors take precedence over a deferred error in an omitted default.
#[test]
fn test_enum_selected_type_review_default_order() {
    let out = compile_and_run(r#"<?php
interface DefaultSelected { public function f(int $a, int $b = 0): int; }
enum DefaultLoose implements DefaultSelected {
    case A; public function f(mixed $a, int $b = 0): int { return 11; }
}
enum DefaultTight implements DefaultSelected {
    case A; public function f(int $a, int $b = parent::A): int { return $a + $b; }
}
function defaultSelectedValue(): string { echo 'a'; return str_repeat('n', 2); }
function invokeDefaultSelected(DefaultSelected $value): void {
    try { echo $value->f(defaultSelectedValue()); }
    catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(); }
    echo '|';
}
invokeDefaultSelected(DefaultLoose::A); invokeDefaultSelected(DefaultTight::A);
"#);
    assert_eq!(out, "a11|aTypeError:DefaultTight::f(): Argument #1 ($a) must be of type int, string given|");
}

/// Parameter-order guarding precedes deferred defaults without EIR optimization too.
#[test]
fn test_enum_selected_type_review_default_order_no_ir_opt() {
    without_ir_opt(test_enum_selected_type_review_default_order);
}

/// Selected variadic element types and unrelated reference places are both preserved.
#[test]
fn test_enum_selected_type_review_variadics() {
    let out = compile_and_run(r#"<?php
interface TailSelected { public function f(int &$out, int ...$values): int; }
enum TailLoose implements TailSelected {
    case A;
    public function f(int &$out, mixed ...$values): int { ++$out; return 11; }
}
enum TailTight implements TailSelected {
    case A;
    public function f(int &$out, int ...$values): int {
        ++$out; $sum = 0; foreach ($values as $value) { $sum += $value; } return $sum;
    }
}
function tailSelectedBad(): string { echo 'a'; return str_repeat('n', 2); }
function tailSelectedGood(): int { echo 'b'; return 2; }
function invokeTailSelected(TailSelected $value): void {
    $out = 2;
    try { echo $value->f($out, tailSelectedBad(), tailSelectedGood()); }
    catch (TypeError $error) { echo $error->getMessage(); }
    echo ':', $out, '|';
    echo $value->f($out, 7, 2), ':', $out, '|';
    try { echo $value->f(out: $out, one: tailSelectedGood(), two: tailSelectedBad()); }
    catch (TypeError $error) { echo $error->getMessage(); }
    echo ':', $out, '|';
}
invokeTailSelected(TailLoose::A); invokeTailSelected(TailTight::A);
"#);
    let error = "TailTight::f(): Argument #2 must be of type int, string given";
    assert_eq!(out, format!("ab11:3|11:4|ba11:5|ab{error}:2|9:3|ba{error}:3|"));
}

/// Static variadic guards retain source effects without EIR optimization.
#[test]
fn test_enum_selected_type_review_variadics_no_ir_opt() {
    without_ir_opt(test_enum_selected_type_review_variadics);
}
