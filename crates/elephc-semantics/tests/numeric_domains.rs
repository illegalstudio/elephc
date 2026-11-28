//! Purpose:
//! Verifies numeric operand domains without compiler or runtime storage dependencies.
//!
//! Called from:
//! - Focused semantic contract tests.
//!
//! Key details:
//! - Arithmetic and coercion rules remain independent of checked integer overflow.

use elephc_semantics::{conversions::ValueKind, numeric_domains::*, SemanticProfile, StrictFeature};

/// Independent flags select exact operator kinds without changing the resolved profile.
#[test]
fn strict_numeric_domain_contracts_preserve_independent_guarantees() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        assert!(!profile.contains(StrictFeature::Overflow));
        for operator in [NumericOperator::Arithmetic, NumericOperator::Division, NumericOperator::Power,
            NumericOperator::Unary, NumericOperator::Update] {
            let rule = operand_rule(profile, operator).unwrap();
            assert_eq!(rule.feature, feature);
            for kind in [ValueKind::Integer, ValueKind::Float] { assert!(rule.domain.allows(kind)); }
            for kind in [ValueKind::Boolean, ValueKind::Null, ValueKind::String, ValueKind::NonScalar] {
                assert!(!rule.domain.allows(kind));
            }
        }
        for operator in [NumericOperator::Modulo, NumericOperator::Bitwise, NumericOperator::Shift] {
            assert_eq!(operand_rule(profile, operator).unwrap().domain, NumericDomain::Integer);
        }
        assert_eq!(operand_rule(profile, NumericOperator::BitNot).unwrap().domain,
            if feature == StrictFeature::Coercions { NumericDomain::IntegerOrString } else { NumericDomain::Integer });
    }
    assert!(operand_rule(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true), NumericOperator::Modulo).is_none());
}

/// Integer power and bit operations use value domains independent of arithmetic overflow.
#[test]
fn strict_numeric_domain_power_and_signed_bit_boundaries() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Arithmetic, true);
    assert!(rejects_negative_integer_power(profile, ValueKind::Integer, ValueKind::Integer, -1));
    assert!(!rejects_negative_integer_power(profile, ValueKind::Float, ValueKind::Integer, -1));
    assert!(!rejects_negative_integer_power(profile.with(StrictFeature::Arithmetic, false)
        .with(StrictFeature::Coercions, true), ValueKind::Integer, ValueKind::Integer, -1));
    assert_eq!(integer_bitwise(IntegerBitOp::ShiftLeft, 1, 63), Ok(i64::MIN));
    assert_eq!(integer_bitwise(IntegerBitOp::ShiftLeft, 1, 64), Ok(0));
    assert_eq!(integer_bitwise(IntegerBitOp::ShiftRight, -1, i64::MAX), Ok(-1));
    assert_eq!(integer_bitwise(IntegerBitOp::ShiftRight, 1, 64), Ok(0));
    assert!(integer_bitwise(IntegerBitOp::ShiftLeft, 1, -1).is_err());
}
