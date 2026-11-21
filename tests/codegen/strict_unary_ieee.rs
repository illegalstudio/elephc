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

/// Typed and boxed unary plus/negation keep the same signed-zero semantics with both optimizers.
#[test]
fn test_strict_unary_ieee_signed_zero_survives_typed_and_mixed_operators() {
    let source = r#"<?php
function ieeeTypedPlus(float $value): float { return +$value; }
function ieeeMixedPlus(mixed $value): mixed { return +$value; }
function ieeeTypedNegate(float $value): float { return -$value; }
function ieeeMixedNegate(mixed $value): mixed { return -$value; }
function ieeeNarrowedPlus(mixed $value): float { if (is_float($value)) { return +$value; } return 0.0; }
function ieeeNarrowedNegate(mixed $value): float { if (is_float($value)) { return -$value; } return 0.0; }
function ieeeZeroSign(mixed $value): string { return fdiv(1.0, $value) < 0.0 ? 'N' : 'P'; }
$negative = $argc > 0 ? -0.0 : 1.0;
$positive = $argc > 0 ? 0.0 : 1.0;
echo ieeeZeroSign(ieeeTypedPlus($negative)), ieeeZeroSign(ieeeMixedPlus($negative)),
    ieeeZeroSign(ieeeTypedNegate($positive)), ieeeZeroSign(ieeeMixedNegate($positive)),
    ieeeZeroSign(ieeeNarrowedPlus($negative)), ieeeZeroSign(ieeeNarrowedNegate($positive));
"#;
    for feature in [None, Some(StrictFeature::Overflow), Some(StrictFeature::Arithmetic), Some(StrictFeature::Coercions)] {
        let profile = feature.map_or(SemanticProfile::PERMISSIVE, |feature| SemanticProfile::PERMISSIVE.with(feature, true));
        for optimize in [false, true] {
            let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
            assert!(output.success, "{profile:?}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, "NNNNNN", "{profile:?}, opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{profile:?}, opt={optimize}: {}", output.stderr);
        }
    }
}


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


/// Unary domain guards and PHP warning paths keep discarded results observable and owners balanced.
#[test]
fn test_strict_unary_ieee_plus_domains_warning_effects_and_temporary_owners() {
    let source = r#"<?php
function exactUnaryPlus(mixed $value): mixed { return +$value; }
function unaryPlusArray(): mixed { return ['owned', str_repeat('x', 20)]; }
foreach ([false, null, '9007199254740993'] as $input) {
    try { $result = exactUnaryPlus($input); echo gettype($result), ':', $result, '|'; }
    catch (TypeError $error) { echo 'T|'; }
}
try { exactUnaryPlus(unaryPlusArray()); } catch (TypeError $error) { echo 'A|'; }
$prefix = $argc > 0 ? '5x' : '0';
set_error_handler(function (int $severity, string $message): bool { global $unaryPlusState; $unaryPlusState = 9; echo 'W'; return true; });
$unaryPlusState = 1;
try { +$prefix; } catch (TypeError $error) { echo 'T'; }
echo ':', $unaryPlusState, '|'; restore_error_handler();
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('unary warning'); });
try { +str_repeat('5x', 2); } catch (RuntimeException $error) { echo 'Q'; } catch (TypeError $error) { echo 'T'; }
restore_error_handler();
"#;
    for feature in [None, Some(StrictFeature::Overflow), Some(StrictFeature::Arithmetic), Some(StrictFeature::Coercions)] {
        let profile = feature.map_or(SemanticProfile::PERMISSIVE, |feature| SemanticProfile::PERMISSIVE.with(feature, true));
        let expected = if matches!(feature, Some(StrictFeature::Arithmetic | StrictFeature::Coercions)) {
            "T|T|T|A|T:1|T"
        } else { "integer:0|integer:0|integer:9007199254740993|A|W:9|Q" };
        for optimize in [false, true] {
            let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
            assert!(output.success, "{profile:?}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, expected, "{profile:?}, opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{profile:?}, opt={optimize}: {}", output.stderr);
        }
    }
}


/// Defaults, constants, closures, arrays and literal eval share the explicit unary-plus value contract.
#[test]
fn test_strict_unary_ieee_plus_literal_defaults_reflection_and_eval() {
    let source = r#"<?php
const UNARY_SOURCE_ZERO = -0.0;
const UNARY_PLUS_ZERO = +UNARY_SOURCE_ZERO;
function ieeeDefault(float $number = +UNARY_SOURCE_ZERO): float { return +$number; }
function ieeeBoolDefault(mixed $number = +false): mixed { return $number; }
$exact = fn(mixed $number = +'9007199254740993'): mixed => +$number;
class IeeePlusDefaults {
    public mixed $number = +'9007199254740993';
    public const ZERO = +UNARY_SOURCE_ZERO;
}
function defaultIeeeSign(mixed $number): string { return fdiv(1.0, $number) < 0.0 ? 'N' : 'P'; }
echo defaultIeeeSign(UNARY_PLUS_ZERO), defaultIeeeSign(ieeeDefault());
$parameter = (new ReflectionFunction('ieeeDefault'))->getParameters()[0];
echo defaultIeeeSign($parameter->getDefaultValue());
$object = new IeeePlusDefaults(); echo ':', $object->number, ':', $exact();
$metadata = new ReflectionClass('IeeePlusDefaults'); echo ':', defaultIeeeSign($metadata->getConstant('ZERO'));
$array = [(array)+(-0.0)]; echo ':', defaultIeeeSign($array[0][0]);
echo ':', defaultIeeeSign(eval('return +"-0.0";'));
echo ':', eval('return array_key_exists(+0, [+0 => +"5"]);');
echo ':', str_contains((string)new ReflectionFunction('ieeeBoolDefault'), '$number = 0');
"#;
    for dynamic in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::DynamicCode, dynamic);
        for optimize in [false, true] {
            let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
            assert!(output.success, "dynamic={dynamic}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, "NNN:9007199254740993:9007199254740993:N:N:N:1:1", "dynamic={dynamic}, opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "dynamic={dynamic}, opt={optimize}: {}", output.stderr);
        }
    }
}
