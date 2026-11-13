//! Purpose:
//! Verifies unary numeric operators preserve IEEE signed zero through every storage shape.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Reciprocal signs distinguish negative zero without depending on output formatting.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// The versioned native unary ABI keeps exact payloads, atomic failures and borrowed owners.
#[test]
fn test_strict_unary_ieee_callback_payloads_capabilities_and_owners() {
    let source = r#"<?php
extern class UnaryCell { public int $tag; public int $low; public int $high; }
extern class UnaryOutput { public ptr $cell; }
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
extern function __elephc_eval_numeric_unary_v1(int $op, int $policy, ptr $value, ptr $output): int;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
$input = ptr_cast<UnaryCell>(malloc(24)); $output = ptr_cast<UnaryOutput>(malloc(8));
$error = malloc(8); ptr_set($error, 0); $input->high = 0;
$input->tag = 2; $input->low = PHP_INT_MIN;
echo __elephc_eval_numeric_unary_v1(0, __POLICY__, $input, $output), ':';
$result = ptr_cast<UnaryCell>($output->cell); echo $result->tag, ':', $result->low, '|';
__elephc_eval_value_release_v3($output->cell, $error);
$input->low = 0;
echo __elephc_eval_numeric_unary_v1(1, __POLICY__, $input, $output), ':';
$result = ptr_cast<UnaryCell>($output->cell); echo $result->tag, ':', $result->low, '|';
__elephc_eval_value_release_v3($output->cell, $error);
$input->tag = 0; $input->low = 9007199254740993;
echo __elephc_eval_numeric_unary_v1(0, __POLICY__, $input, $output), ':';
$result = ptr_cast<UnaryCell>($output->cell); echo $result->tag, ':', $result->low, '|';
__elephc_eval_value_release_v3($output->cell, $error);
$input->low = PHP_INT_MIN; ptr_set($output, 123);
$status = __elephc_eval_numeric_unary_v1(1, __POLICY__, $input, $output); echo $status, ':';
if ($status === 0) { $result = ptr_cast<UnaryCell>($output->cell); echo $result->tag; __elephc_eval_value_release_v3($output->cell, $error); }
else { echo ptr_get($output); $thrown = __elephc_eval_value_take_pending_throwable_v2(); __elephc_eval_value_release_v3($thrown, $error); }
echo ':', $input->low, '|';
$input->tag = 3; $input->low = 1; ptr_set($output, 123);
$status = __elephc_eval_numeric_unary_v1(0, __POLICY__, $input, $output); echo $status, ':';
if ($status === 0) { $result = ptr_cast<UnaryCell>($output->cell); echo $result->tag, ':', $result->low; __elephc_eval_value_release_v3($output->cell, $error); }
else { echo ptr_get($output); $thrown = __elephc_eval_value_take_pending_throwable_v2(); __elephc_eval_value_release_v3($thrown, $error); }
echo '|';
ptr_set($output, 123);
echo __elephc_eval_numeric_unary_v1(99, 0, ptr_null(), $output), ':', ptr_get($output), '|';
echo __elephc_eval_numeric_unary_v1(0, __MISSING__, ptr_null(), $output), ':', ptr_get($output), '|';
echo __elephc_eval_numeric_unary_v1(0, 1099511627776, ptr_null(), $output), ':', ptr_get($output);
free($input); free($output); free($error);
if ($argc < 0) { throw new ArithmeticError('unary metadata'); }
"#;
    for feature in [None, Some(StrictFeature::Overflow), Some(StrictFeature::Arithmetic), Some(StrictFeature::Coercions)] {
        let profile = feature.map_or(SemanticProfile::PERMISSIVE, |feature| SemanticProfile::PERMISSIVE.with(feature, true));
        let missing = if feature == Some(StrictFeature::Overflow) { StrictFeature::Arithmetic } else { StrictFeature::Overflow };
        let source = source.replace("__POLICY__", &profile.bits().to_string())
            .replace("__MISSING__", &SemanticProfile::PERMISSIVE.with(missing, true).bits().to_string());
        let middle = if feature == Some(StrictFeature::Overflow) { "2:123" } else { "0:2" };
        let kind = if matches!(feature, Some(StrictFeature::Arithmetic | StrictFeature::Coercions)) { "2:123" } else { "0:0:1" };
        let expected = format!("0:2:-9223372036854775808|0:2:-9223372036854775808|0:0:9007199254740993|{middle}:-9223372036854775808|{kind}|1:123|1:123|1:123");
        for optimize in [false, true] {
            let output = StrictFixture::new(&source).capture_staged_eval_runtime(profile, optimize);
            assert!(output.success, "{profile:?}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, expected, "{profile:?}, opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{profile:?}, opt={optimize}: {}", output.stderr);
        }
    }
}

/// Normalization snapshots bytes before warning callbacks and contains a throwing handler.
#[test]
fn test_strict_unary_ieee_callback_warning_snapshot_and_atomic_failure() {
    let source = r#"<?php
extern class UnarySnapshotCell { public int $tag; public ptr $low; public int $high; }
extern class UnarySnapshotBits { public int $tag; public int $low; public int $high; }
extern class UnarySnapshotOutput { public ptr $cell; }
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
extern function __elephc_eval_numeric_unary_v1(int $op, int $policy, ptr $value, ptr $output): int;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
$bytes = malloc(8); ptr_set($bytes, 516204408877);
$input = ptr_cast<UnarySnapshotCell>(malloc(24)); $input->tag = 1; $input->low = $bytes; $input->high = 5;
$output = ptr_cast<UnarySnapshotOutput>(malloc(8)); $error = malloc(8); ptr_set($error, 0);
set_error_handler(function (int $severity, string $message) use ($bytes): bool { ptr_set($bytes, 245773184569); echo 'W'; return true; });
echo __elephc_eval_numeric_unary_v1(0, 0, $input, $output), ':';
$result = ptr_cast<UnarySnapshotBits>($output->cell); echo $result->tag, ':', $result->low;
__elephc_eval_value_release_v3($output->cell, $error); restore_error_handler();
ptr_set($bytes, 516204408877); ptr_set($output, 123);
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('unary prefix'); });
echo '|', __elephc_eval_numeric_unary_v1(1, 0, $input, $output), ':', ptr_get($output), ':';
$thrown = __elephc_eval_value_take_pending_throwable_v2(); echo __elephc_eval_value_release_v3($thrown, $error);
restore_error_handler(); free($input); free($bytes); free($output); free($error);
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(SemanticProfile::PERMISSIVE, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "W0:2:-9223372036854775808|2:123:0", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
