//! Purpose:
//! Regresses selected enum variadic and union reference parameter contracts.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - Invalid referents throw before entering the selected implementation.
//! - Accepted weak scalar bindings write back through the original caller cell.

use crate::support::*;

/// A reference collector must not bypass the ordinary prefix or its own element contract.
#[test]
fn test_enum_boxed_reference_review_variadic_contracts() {
    let out = compile_and_run(r#"<?php
class RefTailBase {}
class RefTailNarrow extends RefTailBase {}
interface RefTailContract { public function f(RefTailNarrow &$value, int &...$tail): string; }
enum ARefTailTight implements RefTailContract {
    case A; public function f(RefTailNarrow &$value, int &...$tail): string { return get_class($value); }
}
enum ZRefTailLoose implements RefTailContract {
    case A; public function f(RefTailBase &$value, int &...$tail): string { return get_class($value); }
}
function refTailSelected(RefTailContract $target, RefTailBase $value): void {
    try { echo $target->f($value); } catch (TypeError $e) { echo $e->getMessage(); } echo '|';
}
refTailSelected(ZRefTailLoose::A, new RefTailBase());
refTailSelected(ARefTailTight::A, new RefTailBase());
refTailSelected(ARefTailTight::A, new RefTailNarrow());
interface ValuePrefixTail { public function f(RefTailNarrow $value, int &...$tail): string; }
enum AValuePrefixTight implements ValuePrefixTail {
    case A; public function f(RefTailNarrow $value, int &...$tail): string { return get_class($value); }
}
enum ZValuePrefixLoose implements ValuePrefixTail {
    case A; public function f(RefTailBase $value, int &...$tail): string { return get_class($value); }
}
function valuePrefixSelected(ValuePrefixTail $target, RefTailBase $value): void {
    try { echo $target->f($value); } catch (TypeError $e) { echo $e->getMessage(); } echo '|';
}
valuePrefixSelected(ZValuePrefixLoose::A, new RefTailBase());
valuePrefixSelected(AValuePrefixTight::A, new RefTailBase());
valuePrefixSelected(AValuePrefixTight::A, new RefTailNarrow());
interface RefScalarTail { public function f(int &...$tail): string; }
enum ARefScalarTight implements RefScalarTail {
    case A; public function f(int &...$tail): string { echo 'M'; return gettype($tail[0]); }
}
enum ZRefScalarLoose implements RefScalarTail {
    case A; public function f(mixed &...$tail): string { echo 'M'; return gettype($tail[0]); }
}
function refScalarSelected(RefScalarTail $target, mixed $value): void {
    try { echo $target->f($value); } catch (TypeError $e) { echo $e->getMessage(); }
    echo ':', gettype($value), ':', $value, '|';
}
refScalarSelected(ZRefScalarLoose::A, 'nn');
refScalarSelected(ARefScalarTight::A, 'nn');
refScalarSelected(ARefScalarTight::A, '7');
"#);
    assert_eq!(out, "RefTailBase|ARefTailTight::f(): Argument #1 ($value) must be of type RefTailNarrow, RefTailBase given|RefTailNarrow|RefTailBase|AValuePrefixTight::f(): Argument #1 ($value) must be of type RefTailNarrow, RefTailBase given|RefTailNarrow|Mstring:string:nn|ARefScalarTight::f(): Argument #1 must be of type int, string given:string:nn|Minteger:integer:7|");
}

/// Selected variadic guards and writeback do not depend on EIR optimization.
#[test]
fn test_enum_boxed_reference_review_variadic_contracts_no_ir_opt() {
    without_ir_opt(test_enum_boxed_reference_review_variadic_contracts);
}

/// Full nullable-object and scalar-union contracts must guard boxed reference payloads.
#[test]
fn test_enum_boxed_reference_review_union_contracts() {
    let out = compile_and_run(r#"<?php
class RefUnionBase {}
class RefUnionNarrow extends RefUnionBase {}
interface NullableRefContract { public function f(?RefUnionNarrow &$value): string; }
enum ANullableRefTight implements NullableRefContract {
    case A; public function f(?RefUnionNarrow &$value): string { return gettype($value); }
}
enum ZNullableRefLoose implements NullableRefContract {
    case A; public function f(mixed &$value): string { return gettype($value); }
}
function nullableRefSelected(NullableRefContract $target, mixed $value): void {
    try { echo $target->f($value); } catch (TypeError $e) { echo $e->getMessage(); }
    echo ':', gettype($value), '|';
}
nullableRefSelected(ZNullableRefLoose::A, 'nn');
nullableRefSelected(ANullableRefTight::A, 'nn');
nullableRefSelected(ANullableRefTight::A, null);
nullableRefSelected(ANullableRefTight::A, new RefUnionNarrow());
nullableRefSelected(ANullableRefTight::A, new RefUnionBase());
interface ScalarRefUnion { public function f(int|string &$value): string; }
enum AScalarRefTight implements ScalarRefUnion {
    case A; public function f(int|string &$value): string { return gettype($value); }
}
enum ZScalarRefLoose implements ScalarRefUnion {
    case A; public function f(mixed &$value): string { return gettype($value); }
}
function scalarRefSelected(ScalarRefUnion $target, mixed $value): void {
    try { echo $target->f($value); } catch (TypeError $e) { echo $e->getMessage(); }
    echo ':', gettype($value), '|';
}
scalarRefSelected(ZScalarRefLoose::A, []);
scalarRefSelected(AScalarRefTight::A, []);
scalarRefSelected(AScalarRefTight::A, true);
scalarRefSelected(AScalarRefTight::A, '7');
"#);
    assert_eq!(out, "string:string|ANullableRefTight::f(): Argument #1 ($value) must be of type ?RefUnionNarrow, string given:string|NULL:NULL|object:object|ANullableRefTight::f(): Argument #1 ($value) must be of type ?RefUnionNarrow, RefUnionBase given:object|array:array|AScalarRefTight::f(): Argument #1 ($value) must be of type string|int, array given:array|integer:integer|string:string|");
}

/// Boxed union guards and weak writeback remain correct with EIR optimization disabled.
#[test]
fn test_enum_boxed_reference_review_union_contracts_no_ir_opt() {
    without_ir_opt(test_enum_boxed_reference_review_union_contracts);
}

/// Strict callers reject numeric strings and booleans without changing the caller cell.
#[test]
fn test_enum_boxed_reference_review_strict_contracts() {
    let out = compile_and_run(r#"<?php declare(strict_types=1);
interface StrictRefTail { public function f(int &...$tail): void; }
enum AStrictRefTail implements StrictRefTail {
    case A; public function f(int &...$tail): void { echo 'M'; }
}
enum ZStrictRefTail implements StrictRefTail {
    case A; public function f(mixed &...$tail): void { echo 'M'; }
}
function strictRefTailSelected(StrictRefTail $target, mixed $value): void {
    try { $target->f($value); } catch (TypeError $e) { echo $e->getMessage(); }
    echo ':', gettype($value), '|';
}
strictRefTailSelected(ZStrictRefTail::A, '7');
strictRefTailSelected(AStrictRefTail::A, '7');
strictRefTailSelected(AStrictRefTail::A, 7);
interface StrictRefUnion { public function f(int|string &$value): void; }
enum AStrictRefUnion implements StrictRefUnion {
    case A; public function f(int|string &$value): void { echo 'M'; }
}
enum ZStrictRefUnion implements StrictRefUnion {
    case A; public function f(mixed &$value): void { echo 'M'; }
}
function strictRefUnionSelected(StrictRefUnion $target, mixed $value): void {
    try { $target->f($value); } catch (TypeError $e) { echo $e->getMessage(); }
    echo ':', gettype($value), '|';
}
strictRefUnionSelected(ZStrictRefUnion::A, true);
strictRefUnionSelected(AStrictRefUnion::A, true);
strictRefUnionSelected(AStrictRefUnion::A, '7');
"#);
    assert_eq!(out, "M:string|AStrictRefTail::f(): Argument #1 must be of type int, string given:string|M:integer|M:boolean|AStrictRefUnion::f(): Argument #1 ($value) must be of type string|int, bool given:boolean|M:string|");
}

/// Strict reference checks remain present without EIR optimization.
#[test]
fn test_enum_boxed_reference_review_strict_contracts_no_ir_opt() {
    without_ir_opt(test_enum_boxed_reference_review_strict_contracts);
}

/// Named source effects finish before current-referent guards and accepted coercion writeback.
#[test]
fn test_enum_boxed_reference_review_named_current_referent() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface NamedRefTail { public function f(int $prefix, int &...$tail): string; }
enum ANamedRefTight implements NamedRefTail {
    case A; public function f(int $prefix, int &...$tail): string { echo 'M'; return gettype($tail['extra']); }
}
enum ZNamedRefLoose implements NamedRefTail {
    case A; public function f(int $prefix, mixed &...$tail): string { echo 'M'; return gettype($tail['extra']); }
}
function namedRefChange(mixed &$value, bool $valid): int {
    echo 'E'; if ($valid) { $value = '7'; } else { $value = 'nn'; } return 1;
}
function namedRefSelected(NamedRefTail $target, mixed $value, bool $valid): void {
    try { echo $target->f(extra: $value, prefix: namedRefChange($value, $valid)); }
    catch (TypeError $e) { echo $e->getMessage(); }
    echo ':', gettype($value), ':', $value, '|';
}
namedRefSelected(ZNamedRefLoose::A, 'bad', false);
namedRefSelected(ANamedRefTight::A, 'bad', true);
namedRefSelected(ANamedRefTight::A, '7', false);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "EMstring:string:nn|EMinteger:integer:7|EANamedRefTight::f(): Argument #2 must be of type int, string given:string:nn|");
    assert!(out.stderr.lines().next().unwrap().contains("live_blocks=2 live_bytes=80"), "{}", out.stderr);
}

/// Current referents, named tails and heap ownership do not depend on EIR optimization.
#[test]
fn test_enum_boxed_reference_review_named_current_referent_no_ir_opt() {
    without_ir_opt(test_enum_boxed_reference_review_named_current_referent);
}
