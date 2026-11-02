//! Purpose:
//! Regresses evaluation of overwritten values in selected-enum argument unpacks.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - Array construction evaluates every value before last-wins argument binding.
//! - Overwritten values can mutate caller storage or throw before method entry.
//! - Enum cases are warmed before temporary allocations to keep the heap baseline stable.

use crate::support::*;

/// Allows only the two persistent enum singleton objects after argument cleanup.
fn assert_heap_baseline(out: &ProgramOutput, expected: &str) {
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, expected);
    let summary = out.stderr.lines().next().expect("heap debug summary");
    assert!(summary.split_whitespace().any(|field| field == "live_blocks=2"), "{}", out.stderr);
    assert!(summary.split_whitespace().any(|field| field == "live_bytes=80"), "{}", out.stderr);
}

/// Keeps both concrete signatures reachable through the same interface call.
const DECLARATIONS: &str = r#"<?php
interface UnpackEffects { public function f(int $a): int; }
enum UnpackTight implements UnpackEffects {
    case A; public function f(int $a): int { echo 'M'; return $a; }
}
enum UnpackLoose implements UnpackEffects {
    case A; public function f(mixed $a): int { echo 'L'; return $a; }
}
function mark(int $n, int &$seen): int { echo $n; $seen = $seen * 10 + $n; return $n; }
function warm(UnpackEffects $target): void { echo get_class($target), ':'; }
warm(UnpackTight::A); warm(UnpackLoose::A); echo '|';
"#;

/// Duplicate named keys preserve every value's output and by-reference side effect.
#[test]
fn test_enum_unpack_effect_review_last_wins() {
    let out = compile_and_run_with_heap_debug(&format!(r#"{DECLARATIONS}
function invoke(UnpackEffects $target): void {{
    $seen = 0;
    echo $target->f(...['a' => mark(1, $seen), 'a' => mark(2, $seen)]), ':', $seen, '|';
}}
invoke(UnpackTight::A); invoke(UnpackLoose::A);
"#));
    assert_heap_baseline(&out, "UnpackTight:UnpackLoose:|12M2:12|12L2:12|");
}

/// Last-wins binding cannot erase earlier effects with EIR optimization disabled.
#[test]
fn test_enum_unpack_effect_review_last_wins_no_ir_opt() {
    without_ir_opt(test_enum_unpack_effect_review_last_wins);
}

/// A throwing overwritten value stops later values and prevents selected method entry.
#[test]
fn test_enum_unpack_effect_review_throw() {
    let out = compile_and_run_with_heap_debug(&format!(r#"{DECLARATIONS}
function stop(): int {{ echo 'F'; throw new Exception('stop'); }}
function invoke(UnpackEffects $target): void {{
    $seen = 0;
    try {{ $target->f(...['a' => mark(1, $seen), 'a' => stop(), 'a' => mark(3, $seen)]); }}
    catch (Exception $error) {{ echo $error->getMessage(), ':', $seen, '|'; }}
}}
invoke(UnpackTight::A); invoke(UnpackLoose::A);
"#));
    assert_heap_baseline(&out, "UnpackTight:UnpackLoose:|1Fstop:1|1Fstop:1|");
}

/// Throwing overwritten values keep their source-order semantics without EIR optimization.
#[test]
fn test_enum_unpack_effect_review_throw_no_ir_opt() {
    without_ir_opt(test_enum_unpack_effect_review_throw);
}

/// Temporary string keys and values survive insertion and unwind without retaining producers.
#[test]
fn test_enum_unpack_effect_review_string_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface StringUnpack { public function f(string $a): int; }
enum StringUnpackTight implements StringUnpack {
    case A; public function f(string $a): int { echo 'M'; return strlen($a); }
}
enum StringUnpackLoose implements StringUnpack {
    case A; public function f(mixed $a): int { echo 'L'; return strlen($a); }
}
function sourceKey(): string { echo 'K'; return str_repeat('a', 1); }
function sourceValue(): string { echo 'V'; return str_repeat('x', 2); }
function sourceStop(): string { echo 'F'; throw new Exception('stop'); }
function warm(StringUnpack $target): void { echo get_class($target), ':'; }
warm(StringUnpackTight::A); warm(StringUnpackLoose::A); echo '|';
function invoke(StringUnpack $target): void {
    echo $target->f(...[sourceKey() => sourceValue()]), '|';
    try { $target->f(...[sourceKey() => sourceStop()]); }
    catch (Exception $error) { echo $error->getMessage(), '|'; }
}
invoke(StringUnpackTight::A); invoke(StringUnpackLoose::A);
"#);
    assert_heap_baseline(&out, "StringUnpackTight:StringUnpackLoose:|KVM2|KFstop|KVL2|KFstop|");
}

/// String construction cleanup also holds with EIR optimization disabled.
#[test]
fn test_enum_unpack_effect_review_string_owners_no_ir_opt() {
    without_ir_opt(test_enum_unpack_effect_review_string_owners);
}

/// Rooting literal operands must retain callable entries in heterogeneous source arrays.
#[test]
fn test_enum_unpack_effect_review_callable_literal_control() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function source(): array { return ['callback' => function(): int { return 7; }, 'other' => 1]; }
$total = 0;
for ($index = 0; $index < 5; ++$index) {
    $args = source(); $total += ($args['callback'])(); unset($args);
}
echo $total;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "35");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Callable literal stores remain owning when EIR optimization is disabled.
#[test]
fn test_enum_unpack_effect_review_callable_literal_control_no_ir_opt() {
    without_ir_opt(test_enum_unpack_effect_review_callable_literal_control);
}
