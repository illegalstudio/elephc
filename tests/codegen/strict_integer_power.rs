//! Purpose:
//! Cross-checks shared power folding against native integer-power result kinds and rounding.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Overflow remains independently disabled while both numeric domains are exercised.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Runtime-unknown integer power agrees with constant folding under both numeric domains.
#[test]
fn test_strict_integer_power_native_exact_kinds_promotion_and_folding_parity() {
    let source = r#"<?php
function domainIntegerPower(mixed $base, mixed $exponent): mixed { return $base ** $exponent; }
$exponent = $argc > 0 ? 1 : 2;
$exact = domainIntegerPower(9007199254740993, $exponent);
echo is_int($exact) ? 'I:' : 'F:', $exact, '|';
$minimum = domainIntegerPower(PHP_INT_MIN, $exponent);
echo is_int($minimum) ? 'I:' : 'F:', $minimum, '|';
$base = $argc > 0 ? 654 : 1;
$promoted = domainIntegerPower($base, 32);
echo is_float($promoted) ? 'F:' : 'I:'; printf('%.17g|', $promoted);
echo is_float(domainIntegerPower(2.0, 3)) ? 'F' : 'I';
"#;
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for optimize in [false, true] {
            let output = StrictFixture::new(source).capture_staged_lowering(
                SemanticProfile::PERMISSIVE.with(feature, true), optimize);
            assert!(output.success, "{feature:?}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, "I:9007199254740993|I:-9223372036854775808|F:1.2545499179770422e+90|F",
                "{feature:?}, opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{feature:?}, opt={optimize}: {}", output.stderr);
        }
    }
}


/// Builtin powers keep integer/float contracts through calls, descriptors and native value objects.
#[test]
fn test_strict_numeric_builtin_pow_native_profiles_result_kinds_and_owners() {
    let source = r#"<?php
function numericBuiltinPow(mixed $base, mixed $exponent): mixed { return pow($base, $exponent); }
$exponent = $argc > 0 ? 1 : 2;
echo gettype(numericBuiltinPow(9007199254740993, $exponent)), ':', numericBuiltinPow(9007199254740993, $exponent), '|';
echo gettype(pow(2, 3)), ':', pow(exponent: 3, num: 2), '|';
$callable = pow(...); echo gettype($callable(2, 3)), ':', $callable(2, 3), '|';
echo call_user_func('pow', 2, 3), ':', call_user_func_array('pow', [2, 3]), '|';
echo gettype(numericBuiltinPow(2.0, 3)), ':', numericBuiltinPow(2.0, 3), '|';
try { numericBuiltinPow('2', 3); } catch (TypeError $error) { echo 'S'; }
try { numericBuiltinPow(false, 3); } catch (TypeError $error) { echo 'B'; }
try { $inverse = numericBuiltinPow(2, -1); echo ':', $inverse; } catch (ArithmeticError $error) { echo ':A'; }
echo '|', is_float(numericBuiltinPow(2, 63)) ? 'F' : 'I';
$number = new BcMath\Number('2');
$result = numericBuiltinPow($number, 3); echo '|', get_class($result), ':', $result->value;
"#;
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let inverse = if feature == StrictFeature::Arithmetic { ":A" } else { ":0.5" };
        let expected = format!("integer:9007199254740993|integer:8|integer:8|8:8|double:8|SB{inverse}|F|BcMath\\Number:8");
        for optimize in [false, true] {
            let output = StrictFixture::new(source).capture_staged_bcmath(SemanticProfile::PERMISSIVE.with(feature, true), optimize);
            assert!(output.success, "{feature:?}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, expected, "{feature:?}, opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{feature:?}, opt={optimize}: {}", output.stderr);
        }
    }
}
