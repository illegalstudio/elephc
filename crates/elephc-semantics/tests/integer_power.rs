//! Purpose:
//! Verifies exact integer power and the first-overflow floating continuation.
//!
//! Called from:
//! - Focused dependency-neutral semantic tests.
//!
//! Key details:
//! - Float expectations were cross-checked against PHP's integer power operation.

use elephc_semantics::integer_power::{php_integer_power, PowerResult};

/// Fitting integer powers preserve signed width and extreme exponent identities.
#[test]
fn strict_integer_power_preserves_exact_fitting_results() {
    for (base, exponent, expected) in [(0, 0, 1), (0, i64::MAX, 0), (1, i64::MAX, 1),
        (-1, i64::MAX, -1), (i64::MAX, 1, i64::MAX), (i64::MIN, 1, i64::MIN),
        (9007199254740993, 1, 9007199254740993), (2, 62, 4611686018427387904)] {
        assert_eq!(php_integer_power(base, exponent), PowerResult::Int(expected), "{base} ** {exponent}");
    }
}

/// Promotion reproduces PHP's ordered rounding and retains IEEE floating domains.
#[test]
fn strict_integer_power_promotes_from_the_first_overflowing_product() {
    for (base, exponent, expected) in [(2, 64, 1.8446744073709552e19_f64),
        (654, 32, 1.2545499179770422e90), (-133, 101, -3.2286111158631344e214),
        (2, -1, 0.5), (2, i64::MAX, f64::INFINITY)] {
        let PowerResult::Float(value) = php_integer_power(base, exponent) else { panic!("{base} ** {exponent}"); };
        assert_eq!(value.to_bits(), expected.to_bits(), "{base} ** {exponent}");
    }
}
