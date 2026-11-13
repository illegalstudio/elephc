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
