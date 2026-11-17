//! Purpose:
//! Evaluates numeric constants without runtime effects, diagnostics or target dependencies.
//!
//! Called from:
//! - Compiler folding and shared constant-initializer analysis.
//!
//! Key details:
//! - Failure declines folding so the execution boundary keeps its own diagnostic.
//! - Integer division, promotion and power preserve the selected semantic profile.

use crate::{SemanticProfile, StrictFeature, numeric::IntegerOp, numeric_unary::NumericValue};

/// Computes a safely foldable numeric result while preserving integer and floating kinds.
pub fn fold(profile: SemanticProfile, op: IntegerOp, left: NumericValue, right: NumericValue) -> Option<NumericValue> {
    evaluate(profile, op, left, right).filter(|value| match value {
        NumericValue::Int(_) => true, NumericValue::Float(value) => value.is_finite(),
    })
}

/// Computes silent numeric constants, retaining IEEE results for runtime metadata consumers.
pub fn evaluate(profile: SemanticProfile, op: IntegerOp, left: NumericValue, right: NumericValue) -> Option<NumericValue> {
    if let (NumericValue::Int(left), NumericValue::Int(right)) = (left, right) {
        return evaluate_integers(profile, op, left, right);
    }
    if op == IntegerOp::Mod { return None; }
    let number = |value| match value { NumericValue::Int(value) => value as f64, NumericValue::Float(value) => value };
    evaluate_floats(op, number(left), number(right)).map(NumericValue::Float)
}

/// Keeps exact integer results and explicitly selects checked failure or PHP promotion.
fn evaluate_integers(profile: SemanticProfile, op: IntegerOp, left: i64, right: i64) -> Option<NumericValue> {
    match op {
        IntegerOp::Div if right == 0 => None,
        IntegerOp::Div if profile.contains(StrictFeature::Arithmetic) =>
            Some(NumericValue::Float(left as f64 / right as f64)),
        IntegerOp::Div => Some(match (left.checked_rem(right), left.checked_div(right)) {
            (Some(0), Some(result)) => NumericValue::Int(result),
            _ => NumericValue::Float(left as f64 / right as f64),
        }),
        IntegerOp::Mod => crate::numeric::checked_integer(op, left, right).ok().map(NumericValue::Int),
        IntegerOp::Pow if right < 0 && (left == 0 || profile.contains(StrictFeature::Arithmetic)) => None,
        IntegerOp::Pow if right >= 0 && profile.contains(StrictFeature::Overflow) =>
            crate::numeric::checked_integer(op, left, right).ok().map(NumericValue::Int),
        IntegerOp::Pow => match crate::integer_power::php_integer_power(left, right) {
            crate::integer_power::PowerResult::Int(value) => Some(NumericValue::Int(value)),
            crate::integer_power::PowerResult::Float(value) => Some(NumericValue::Float(value)),
        },
        _ => match crate::numeric::checked_integer(op, left, right) {
            Ok(value) => Some(NumericValue::Int(value)),
            Err(_) if profile.contains(StrictFeature::Overflow) => None,
            Err(_) => evaluate_floats(op, left as f64, right as f64).map(NumericValue::Float),
        },
    }
}

/// Declines zero division and zero to a negative power before their runtime diagnostics.
fn evaluate_floats(op: IntegerOp, left: f64, right: f64) -> Option<f64> {
    if op == IntegerOp::Div && right == 0.0 { return None; }
    if op == IntegerOp::Pow && left == 0.0 && right < 0.0 { return None; }
    let result = match op {
        IntegerOp::Add => left + right, IntegerOp::Sub => left - right,
        IntegerOp::Mul => left * right, IntegerOp::Div => left / right,
        IntegerOp::Pow => left.powf(right), IntegerOp::Mod => return None,
    };
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Metadata preparation keeps silent IEEE results while AST folding retains its finite policy.
    #[test]
    fn constant_numeric_metadata_preserves_ieee_without_folding_failures() {
        let profile = SemanticProfile::PERMISSIVE;
        let float = NumericValue::Float;
        assert_eq!(evaluate(profile, IntegerOp::Mul, float(f64::MAX), float(2.0)), Some(float(f64::INFINITY)));
        assert!(matches!(evaluate(profile, IntegerOp::Sub, float(f64::INFINITY), float(f64::INFINITY)), Some(NumericValue::Float(value)) if value.is_nan()));
        assert_eq!(fold(profile, IntegerOp::Mul, float(f64::MAX), float(2.0)), None);
        assert_eq!(evaluate(profile, IntegerOp::Pow, float(0.0), float(-1.0)), None);
        assert_eq!(evaluate(profile, IntegerOp::Pow, NumericValue::Int(0), NumericValue::Int(-1)), None);
    }

    /// Constant evaluation retains PHP promotion, checked failure and independent division policy.
    #[test]
    fn constant_numeric_folding_preserves_modes_and_failure_boundaries() {
        for arithmetic in [false, true] {
            for overflow in [false, true] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Arithmetic, arithmetic)
                    .with(StrictFeature::Overflow, overflow);
                let int = NumericValue::Int;
                assert_eq!(fold(profile, IntegerOp::Div, int(6), int(3)),
                    Some(if arithmetic { NumericValue::Float(2.0) } else { int(2) }));
                assert_eq!(fold(profile, IntegerOp::Div, int(1), int(0)), None);
                assert_eq!(fold(profile, IntegerOp::Mod, int(i64::MIN), int(-1)), Some(int(0)));
                assert_eq!(fold(profile, IntegerOp::Add, int(i64::MAX), int(1)),
                    (!overflow).then_some(NumericValue::Float(i64::MAX as f64 + 1.0)));
                assert_eq!(fold(profile, IntegerOp::Pow, int(2), int(-1)),
                    (!arithmetic).then_some(NumericValue::Float(0.5)));
                assert_eq!(fold(profile, IntegerOp::Pow, int(0), int(-1)), None);
                assert_eq!(fold(profile, IntegerOp::Mul, NumericValue::Float(f64::MAX), NumericValue::Float(2.0)), None);
            }
        }
    }
}
