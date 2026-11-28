//! Purpose:
//! Tests protected checked eval division with real native values and PHP warning handlers.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Raw ABI probes retain independent input owners and verify failure output atomicity.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Division captures values before callbacks and contains kind, zero and warning-handler failures.
#[test]
fn test_strict_eval_division_native_snapshots_errors_and_ownership() {
    let source = r#"<?php
extern class DivCell { public int $tag; public int $low; public int $high; }
extern class DivFloat { public int $tag; public float $number; public int $high; }
extern class DivOutput { public ptr $cell; }
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
extern function __elephc_eval_checked_numeric_v1(int $op, ptr $left, ptr $right, ptr $output): int;
extern function __elephc_eval_value_int(int $value): ptr;
extern function __elephc_eval_value_string(string $value, int $length): ptr;
extern function __elephc_eval_value_array_new(int $capacity): ptr;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
$output = ptr_cast<DivOutput>(malloc(8)); $error = malloc(8); ptr_set($error, 0);
$left = __elephc_eval_value_int(8); $right = __elephc_eval_value_int(2);
echo __elephc_eval_checked_numeric_v1(9, $left, $right, $output), ':';
$number = ptr_cast<DivFloat>($output->cell); echo $number->tag, ':', $number->number;
__elephc_eval_value_release_v3($output->cell, $error); __elephc_eval_value_release_v3($left, $error);
$left = __elephc_eval_value_string('5x', 2);
set_error_handler(function (int $severity, string $message) use ($right): bool {
    $input = ptr_cast<DivCell>($right); $input->low = 100; echo 'W'; return true;
});
echo '|', __elephc_eval_checked_numeric_v1(9, $left, $right, $output), ':';
$number = ptr_cast<DivFloat>($output->cell); echo $number->number;
__elephc_eval_value_release_v3($output->cell, $error);
restore_error_handler();
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('division prefix'); });
ptr_set($output, 123); echo '|', __elephc_eval_checked_numeric_v1(9, $left, $right, $output), ':', ptr_get($output), ':';
$thrown = __elephc_eval_value_take_pending_throwable_v2(); echo __elephc_eval_value_release_v3($thrown, $error);
restore_error_handler(); __elephc_eval_value_release_v3($left, $error); __elephc_eval_value_release_v3($right, $error);
$left = __elephc_eval_value_int(1);
foreach (['0', '-0.0'] as $zero) {
    $right = __elephc_eval_value_string($zero, strlen($zero)); ptr_set($output, 123);
    echo '|', __elephc_eval_checked_numeric_v1(9, $left, $right, $output), ':', ptr_get($output), ':';
    $thrown = __elephc_eval_value_take_pending_throwable_v2(); echo __elephc_eval_value_release_v3($thrown, $error);
    __elephc_eval_value_release_v3($right, $error);
}
$right = __elephc_eval_value_array_new(0); ptr_set($output, 123);
echo '|', __elephc_eval_checked_numeric_v1(9, $left, $right, $output), ':', ptr_get($output), ':';
$thrown = __elephc_eval_value_take_pending_throwable_v2(); echo __elephc_eval_value_release_v3($thrown, $error);
__elephc_eval_value_release_v3($left, $error); __elephc_eval_value_release_v3($right, $error);
free($output); free($error);
if ($argc < 0) { throw new DivisionByZeroError('metadata'); }
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "0:2:4|W0:2.5|2:123:0|2:123:0|2:123:0|2:123:0", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
