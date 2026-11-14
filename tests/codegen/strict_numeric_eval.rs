//! Purpose:
//! Verifies the checked numeric callback ABI used by the eval runtime.
//!
//! Called from:
//! - Focused native codegen integration tests.
//!
//! Key details:
//! - Staged profiles stay unavailable at public compilation boundaries.
//! - Both optimizer settings must contain exceptions and retire each native owner.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// The native callback contains arithmetic failures and transfers only successful result owners.
#[test]
fn test_strict_eval_numeric_staged_native_callback_status_and_ownership() {
    let source = r#"<?php
extern class CheckedNumericCell { public int $tag; public int $low; public int $high; }
extern class CheckedNumericOutput { public ptr $cell; }
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
extern function __elephc_eval_checked_numeric_v1(int $op, ptr $left, ptr $right, ptr $output): int;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
$left = ptr_cast<CheckedNumericCell>(malloc(24));
$right = ptr_cast<CheckedNumericCell>(malloc(24));
$output = ptr_cast<CheckedNumericOutput>(malloc(8));
$error = malloc(8); ptr_set($error, 0);
$left->tag = 0; $left->low = 9007199254740993; $left->high = 0;
$right->tag = 0; $right->low = 0; $right->high = 0;
ptr_set($output, 123);
$status = __elephc_eval_checked_numeric_v1(0, $left, $right, $output);
if ($status === 1) { echo 'unsupported:', ptr_get($output); } else {
echo $status, ':';
$result = ptr_cast<CheckedNumericCell>($output->cell);
echo $result->tag, ':', $result->low, ':';
echo __elephc_eval_value_release_v3($output->cell, $error), '|';
foreach ([0, 1, 2, 3, 4] as $op) {
    $left->low = 9223372036854775807; $right->low = 1;
    if ($op === 1) { $left->low = PHP_INT_MIN; }
    if ($op === 2) { $right->low = 2; }
    if ($op === 3) { $left->low = 2; $right->low = 63; }
    if ($op === 4) { $left->low = PHP_INT_MIN; }
    ptr_set($output, 123);
    echo __elephc_eval_checked_numeric_v1($op, $left, $right, $output), ':', ptr_get($output), ':';
    $thrown = __elephc_eval_value_take_pending_throwable_v2();
    echo __elephc_eval_value_release_v3($thrown, $error), '|';
}
ptr_set($output, 123);
echo __elephc_eval_checked_numeric_v1(99, $left, $right, $output), ':', ptr_get($output);
}
free($left); free($right); free($output); free($error);
if ($argc < 0) { throw new ArithmeticError('callback metadata'); }
"#;
    for checked in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
        let expected = if checked { "0:0:9007199254740993:0|2:123:0|2:123:0|2:123:0|2:123:0|2:123:0|1:123" }
            else { "unsupported:123" };
        for optimize in [false, true] {
            let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
            assert!(output.success, "checked={checked}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, expected, "checked={checked}, opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "checked={checked}, opt={optimize}: {}", output.stderr);
        }
    }
}


/// Checked scalar arithmetic keeps exact strings, kind failures and callback exceptions observable.
#[test]
fn test_strict_numeric_operands_staged_values_callbacks_and_exception_owners() {
    let source = r#"<?php
function exactNumericAdd(mixed $left, mixed $right): mixed { return $left + $right; }
function exactNumericSub(mixed $left, mixed $right): mixed { return $left - $right; }
function exactNumericMul(mixed $left, mixed $right): mixed { return $left * $right; }
function discardNumericPrefix(mixed $left): void {
    try { $left + 1.0; } catch (RuntimeException $error) { echo 'Q'; }
}
$bad = $argc > 0 ? 'invalid' : 'also invalid';
try { exactNumericAdd($bad, 1); } catch (TypeError $error) { echo 'A'; }
try { exactNumericSub($bad, 1); } catch (TypeError $error) { echo 'S'; }
try { exactNumericMul($bad, 1.0); } catch (TypeError $error) { echo 'M'; }
echo ':', exactNumericAdd('9007199254740993', 0);
echo ':', is_float(exactNumericMul('9223372036854775808', 1));
$prefix = $argc > 0 ? '5x' : '0'; $right = 3;
set_error_handler(function (int $severity, string $message) use (&$right): bool { $right = 63; echo 'W'; return true; });
echo ':', exactNumericAdd($prefix, $right), ':', $right;
$right = 3; echo ':', exactNumericSub($prefix, $right);
$right = 3; echo ':', exactNumericMul($prefix, $right);
restore_error_handler();
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('numeric prefix'); });
echo ':'; discardNumericPrefix($prefix);
restore_error_handler();
"#;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "ASM:9007199254740993:1:W8:63:W2:W15:Q", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}


/// Reduction callbacks keep input arrays alive and retire partial carries across native exceptions.
#[test]
fn test_strict_eval_reductions_staged_native_callback_ownership() {
    let source = r#"<?php
extern class ReductionCell { public int $tag; public int $low; public int $high; }
extern class ReductionOutput { public ptr $cell; }
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
extern function __elephc_eval_value_array_new(int $capacity): ptr;
extern function __elephc_eval_value_array_set(ptr $array, ptr $index, ptr $value): ptr;
extern function __elephc_eval_value_int(int $value): ptr;
extern function __elephc_eval_checked_numeric_v1(int $op, ptr $left, ptr $right, ptr $output): int;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
$error = malloc(8); ptr_set($error, 0);
$output = ptr_cast<ReductionOutput>(malloc(8));
foreach ([5, 6] as $op) {
    $array = __elephc_eval_value_array_new(2);
    $zero = __elephc_eval_value_int(0); $one = __elephc_eval_value_int(1);
    $first = __elephc_eval_value_int(3); $second = __elephc_eval_value_int(4);
    __elephc_eval_value_array_set($array, $zero, $first);
    __elephc_eval_value_array_set($array, $one, $second);
    __elephc_eval_value_release_v3($first, $error); __elephc_eval_value_release_v3($second, $error);
    ptr_set($output, 123);
    echo __elephc_eval_checked_numeric_v1($op, $array, $array, $output), ':';
    $result = ptr_cast<ReductionCell>($output->cell); echo $result->tag, ':', $result->low, ':';
    echo __elephc_eval_value_release_v3($output->cell, $error), '|';
    $first = __elephc_eval_value_int(PHP_INT_MAX);
    $second = __elephc_eval_value_int($op === 5 ? 1 : 2);
    __elephc_eval_value_array_set($array, $zero, $first);
    __elephc_eval_value_array_set($array, $one, $second);
    __elephc_eval_value_release_v3($first, $error); __elephc_eval_value_release_v3($second, $error);
    ptr_set($output, 123);
    echo __elephc_eval_checked_numeric_v1($op, $array, $array, $output), ':', ptr_get($output), ':';
    $thrown = __elephc_eval_value_take_pending_throwable_v2();
    echo __elephc_eval_value_release_v3($thrown, $error), ':';
    echo __elephc_eval_value_release_v3($array, $error), '|';
    __elephc_eval_value_release_v3($zero, $error); __elephc_eval_value_release_v3($one, $error);
}
free($output); free($error);
if ($argc < 0) { throw new ArithmeticError('reduction callback metadata'); }
"#;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "0:0:7:0|2:123:0:0|0:0:12:0|2:123:0:0|", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}


/// Mutation-capable kernels receive borrowed stack cells and return independent result owners.
#[test]
fn test_strict_eval_updates_staged_native_callback_borrows_and_failures() {
    let source = r#"<?php
extern class UpdateCell { public int $tag; public int $low; public int $high; }
extern class UpdateOutput { public ptr $cell; }
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
extern function __elephc_eval_value_int(int $value): ptr;
extern function __elephc_eval_value_null(): ptr;
extern function __elephc_eval_value_string(string $value, int $length): ptr;
extern function __elephc_eval_value_array_new(int $capacity): ptr;
extern function __elephc_eval_checked_numeric_v1(int $op, ptr $left, ptr $right, ptr $output): int;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
$error = malloc(8); ptr_set($error, 0);
$output = ptr_cast<UpdateOutput>(malloc(8));
foreach ([7, 8] as $op) {
    $input = __elephc_eval_value_int(10); $original = ptr_cast<UpdateCell>($input);
    echo __elephc_eval_checked_numeric_v1($op, $input, $input, $output), ':';
    $result = ptr_cast<UpdateCell>($output->cell); echo $result->low, ':', $original->low, ':';
    echo __elephc_eval_value_release_v3($output->cell, $error), ':';
    echo __elephc_eval_value_release_v3($input, $error), '|';
    $input = __elephc_eval_value_int($op === 7 ? PHP_INT_MAX : PHP_INT_MIN);
    $original = ptr_cast<UpdateCell>($input); ptr_set($output, 123);
    echo __elephc_eval_checked_numeric_v1($op, $input, $input, $output), ':', ptr_get($output), ':', $original->low, ':';
    $thrown = __elephc_eval_value_take_pending_throwable_v2();
    echo __elephc_eval_value_release_v3($thrown, $error), ':';
    echo __elephc_eval_value_release_v3($input, $error), '|';
}
$input = __elephc_eval_value_string('9223372036854775807', 19); ptr_set($output, 123);
echo __elephc_eval_checked_numeric_v1(7, $input, $input, $output), ':', ptr_get($output), ':';
$thrown = __elephc_eval_value_take_pending_throwable_v2();
echo __elephc_eval_value_release_v3($thrown, $error), ':';
echo __elephc_eval_value_release_v3($input, $error), '|';
$input = __elephc_eval_value_null(); ptr_set($output, 123);
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('null decrement'); });
echo __elephc_eval_checked_numeric_v1(8, $input, $input, $output), ':', ptr_get($output), ':';
$thrown = __elephc_eval_value_take_pending_throwable_v2();
echo __elephc_eval_value_release_v3($thrown, $error), ':';
restore_error_handler();
echo __elephc_eval_value_release_v3($input, $error), '|';
$input = __elephc_eval_value_array_new(0); ptr_set($output, 123);
echo __elephc_eval_checked_numeric_v1(7, $input, $input, $output), ':', ptr_get($output), ':';
$thrown = __elephc_eval_value_take_pending_throwable_v2();
echo __elephc_eval_value_release_v3($thrown, $error), ':';
echo __elephc_eval_value_release_v3($input, $error);
free($output); free($error);
if ($argc < 0) { throw new ArithmeticError('update metadata'); throw new TypeError('type metadata'); }
"#;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "0:11:10:0:0|2:123:9223372036854775807:0:0|0:9:10:0:0|2:123:-9223372036854775808:0:0|2:123:0:0|2:123:0:0|2:123:0:0", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}


/// Checked update callbacks keep catch clauses and argument owners alive through optimization.
#[test]
fn test_strict_eval_updates_staged_aot_warning_and_type_failures() {
    let source = r#"<?php
function strictNullUpdate(): void {
    $value = null;
    try { $value--; } catch (RuntimeException $error) { echo 'N'; }
}
function strictInvalidUpdate(mixed $value): void {
    try { $value++; } catch (TypeError $error) { echo 'A'; }
}
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('update warning'); });
strictNullUpdate();
restore_error_handler();
strictInvalidUpdate([1]);
"#;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "NA", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}


/// Left prefix diagnostics precede a right kind failure, while an invalid left operand stops first.
#[test]
fn test_strict_numeric_diagnostics_native_ordered_kind_failures_and_owners() {
    let source = r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
extern function __elephc_eval_value_string(string $value, int $length): ptr;
extern function __elephc_eval_checked_numeric_v1(int $op, ptr $left, ptr $right, ptr $output): int;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
$output = malloc(8); $error = malloc(8); ptr_set($error, 0);
$prefix = __elephc_eval_value_string('5x', 2); $bad = __elephc_eval_value_string('bad', 3);
set_error_handler(function (int $severity, string $message): bool { echo 'W'; return true; });
foreach ([0, 1, 2, 3, 9] as $op) {
    ptr_set($output, 123); echo __elephc_eval_checked_numeric_v1($op, $prefix, $bad, $output), ':', ptr_get($output), ':';
    $thrown = __elephc_eval_value_take_pending_throwable_v2(); echo __elephc_eval_value_release_v3($thrown, $error), '|';
    ptr_set($output, 123); echo __elephc_eval_checked_numeric_v1($op, $bad, $prefix, $output), ':', ptr_get($output), ':';
    $thrown = __elephc_eval_value_take_pending_throwable_v2(); echo __elephc_eval_value_release_v3($thrown, $error), ';';
}
restore_error_handler();
set_error_handler(function (int $severity, string $message): bool { echo 'X'; throw new RuntimeException('first diagnostic'); });
ptr_set($output, 123); echo __elephc_eval_checked_numeric_v1(0, $prefix, $bad, $output), ':', ptr_get($output), ':';
$thrown = __elephc_eval_value_take_pending_throwable_v2(); echo __elephc_eval_value_release_v3($thrown, $error);
restore_error_handler();
__elephc_eval_value_release_v3($prefix, $error); __elephc_eval_value_release_v3($bad, $error);
free($output); free($error);
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, format!("{}X2:123:0", "W2:123:0|2:123:0;".repeat(5)), "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}


/// Invalid aggregate sources return a protected TypeError without publishing a null result owner.
#[test]
fn test_strict_eval_aggregate_source_failures_preserve_output_and_pending_owners() {
    let source = r#"<?php
extern class AggregateSourceCell { public int $tag; public ptr $payload; public int $high; }
extern class AggregateSourceInteger { public int $tag; public int $low; public int $high; }
extern class AggregateSourceThrowable { public int $runtime_class; public ptr $message; public int $length; }
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
extern function memcmp(ptr $left, string $right, int $length): int;
extern function __elephc_eval_value_int(int $value): ptr;
extern function __elephc_eval_checked_numeric_v1(int $op, ptr $left, ptr $right, ptr $output): int;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
$input = __elephc_eval_value_int(7);
$output = malloc(8); $error = malloc(8); ptr_set($error, 0);
foreach ([5, 6] as $operation) {
    ptr_set($output, 123);
    $status = __elephc_eval_checked_numeric_v1($operation, $input, $input, $output);
    echo $status, ':', ptr_get($output);
    if ($status === 2) {
        $thrown = __elephc_eval_value_take_pending_throwable_v2();
        $cell = ptr_cast<AggregateSourceCell>($thrown);
        $object = ptr_cast<AggregateSourceThrowable>($cell->payload);
        $message = $operation === 5 ? 'array_sum(): Argument #1 ($array) must be of type array' : 'array_product(): Argument #1 ($array) must be of type array';
        echo ':', $cell->tag, ':', $object->length === strlen($message) && memcmp($object->message, $message, strlen($message)) === 0 ? 'M' : 'X';
        echo ':', __elephc_eval_value_release_v3($thrown, $error);
    }
    echo '|';
}
$integer = ptr_cast<AggregateSourceInteger>($input);
echo $integer->tag, ':', $integer->low, ':', __elephc_eval_value_release_v3($input, $error);
free($output); free($error);
if ($argc < 0) { throw new TypeError('aggregate source metadata'); }
"#;
    for overflow in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, overflow);
        let expected = if overflow { "2:123:6:M:0|2:123:6:M:0|0:7:0" } else { "1:123|1:123|0:7:0" };
        for optimize in [false, true] {
            let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
            assert!(output.success, "overflow={overflow}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, expected, "overflow={overflow}, opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "overflow={overflow}, opt={optimize}: {}", output.stderr);
        }
    }
}
