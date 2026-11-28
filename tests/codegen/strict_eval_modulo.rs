//! Purpose:
//! Tests protected eval modulo with native numeric cells and PHP warning callbacks.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Heap debugging covers guarded input snapshots on success, zero, kind and callback failures.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Native modulo preserves exact conversion, ordered callbacks and independent borrowed inputs.
#[test]
fn test_strict_eval_modulo_native_conversions_callbacks_and_exception_owners() {
    let source = r#"<?php
extern class ModCell { public int $tag; public int $low; public int $high; }
extern class ModOutput { public ptr $cell; }
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
extern function __elephc_eval_checked_numeric_v1(int $op, ptr $left, ptr $right, ptr $output): int;
extern function __elephc_eval_value_int(int $value): ptr;
extern function __elephc_eval_value_float(float $value): ptr;
extern function __elephc_eval_value_string(string $value, int $length): ptr;
extern function __elephc_eval_value_array_new(int $capacity): ptr;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
function modProbe(ptr $left, ptr $right, ptr $out, ptr $error): void {
    ptr_set($out, 123); $status = __elephc_eval_checked_numeric_v1(10, $left, $right, $out);
    echo $status, ':';
    if ($status === 0) {
        $output = ptr_cast<ModOutput>($out); $value = ptr_cast<ModCell>($output->cell);
        echo $value->low; __elephc_eval_value_release_v3($output->cell, $error);
    } else {
        echo ptr_get($out), ':'; $thrown = __elephc_eval_value_take_pending_throwable_v2();
        echo __elephc_eval_value_release_v3($thrown, $error);
    }
}
$out = malloc(8); $error = malloc(8); ptr_set($error, 0);
$left = __elephc_eval_value_int(PHP_INT_MIN); $right = __elephc_eval_value_int(-1);
modProbe($left, $right, $out, $error);
__elephc_eval_value_release_v3($left, $error); __elephc_eval_value_release_v3($right, $error);
set_error_handler(function (int $severity, string $message): bool { echo $severity === 8192 ? 'D' : 'W'; return true; });
$right = __elephc_eval_value_int(2);
foreach (['9223372036854775807', '9223372036854775808', '1e20', '1e999', '7.5'] as $text) {
    $left = __elephc_eval_value_string($text, strlen($text)); echo '|'; modProbe($left, $right, $out, $error);
    __elephc_eval_value_release_v3($left, $error);
}
foreach ([7.5, 1.0e20] as $number) {
    $left = __elephc_eval_value_float($number); echo '|'; modProbe($left, $right, $out, $error);
    __elephc_eval_value_release_v3($left, $error);
}
__elephc_eval_value_release_v3($right, $error);
$left = __elephc_eval_value_string('5.5x', 4); $right = __elephc_eval_value_string('2.5y', 4);
echo '|'; modProbe($left, $right, $out, $error);
__elephc_eval_value_release_v3($right, $error);
restore_error_handler();
$right = __elephc_eval_value_int(2);
set_error_handler(function (int $severity, string $message) use ($right): bool {
    $source = ptr_cast<ModCell>($right); $source->low = 0; echo 'M'; return true;
});
echo '|'; modProbe($left, $right, $out, $error); $source = ptr_cast<ModCell>($right); echo ':', $source->low;
restore_error_handler(); __elephc_eval_value_release_v3($left, $error);
__elephc_eval_value_release_v3($right, $error);
$left = __elephc_eval_value_string('7.5', 3); $right = __elephc_eval_value_int(2);
set_error_handler(function (int $severity, string $message): bool { echo 'X'; throw new RuntimeException('modulo diagnostic'); });
echo '|'; modProbe($left, $right, $out, $error);
restore_error_handler(); __elephc_eval_value_release_v3($left, $error);
__elephc_eval_value_release_v3($right, $error);
$left = __elephc_eval_value_int(7); $right = __elephc_eval_value_int(0);
echo '|'; modProbe($left, $right, $out, $error);
__elephc_eval_value_release_v3($left, $error); __elephc_eval_value_release_v3($right, $error);
$left = __elephc_eval_value_string('5.5x', 4); $right = __elephc_eval_value_array_new(0);
set_error_handler(function (int $severity, string $message): bool { echo $severity === 8192 ? 'D' : 'W'; return true; });
echo '|'; modProbe($left, $right, $out, $error); restore_error_handler();
__elephc_eval_value_release_v3($left, $error); __elephc_eval_value_release_v3($right, $error);
free($out); free($error);
if ($argc < 0) { throw new DivisionByZeroError('metadata'); }
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "0:0|0:1|0:1|D0:1|D0:0|D0:1|D0:1|W0:0|WDWD0:1|MM0:1:0|X2:123:0|2:123:0|WD2:123:0", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
