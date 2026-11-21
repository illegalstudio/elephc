//! Purpose:
//! Shares exact unary numeric results independently of runtime storage and target ABI.
//!
//! Called from:
//! - Compiler constant evaluation and interpreter numeric adapters.
//!
//! Key details:
//! - Floating operators preserve signed zero and NaN payloads without synthetic addition.

use crate::numeric::NumericFailure;

/// Stable unary selectors small enough for typed EIR metadata and a versioned C boundary.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericUnaryOp { Plus = 0, Negate = 1 }

/// A normalized PHP number, with its integer or floating kind retained.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumericValue { Int(i64), Float(f64) }

/// Applies unary arithmetic after operand normalization without losing floating bit patterns.
pub fn apply(op: NumericUnaryOp, value: NumericValue, checked: bool) -> Result<NumericValue, NumericFailure> {
    Ok(match (op, value) {
        (NumericUnaryOp::Plus, value) => value,
        (NumericUnaryOp::Negate, NumericValue::Float(value)) => NumericValue::Float(f64::from_bits(value.to_bits() ^ (1 << 63))),
        (NumericUnaryOp::Negate, NumericValue::Int(value)) => match value.checked_neg() {
            Some(value) => NumericValue::Int(value),
            None if checked => return Err(NumericFailure::Overflow),
            None => NumericValue::Float(-(value as f64)),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unary operations preserve IEEE bits and promote only an unchecked integer minimum.
    #[test]
    fn strict_unary_ieee_neutral_results_preserve_bits_and_integer_domains() {
        for bits in [0, 1 << 63, 0x7ff8_1234_5678_9abc, 0xfff8_1234_5678_9abc] {
            for checked in [false, true] {
                let value = NumericValue::Float(f64::from_bits(bits));
                for (op, expected) in [(NumericUnaryOp::Plus, bits), (NumericUnaryOp::Negate, bits ^ (1 << 63))] {
                    let NumericValue::Float(result) = apply(op, value, checked).unwrap() else { panic!("floating kind changed"); };
                    assert_eq!(result.to_bits(), expected);
                }
            }
        }
        assert_eq!(apply(NumericUnaryOp::Plus, NumericValue::Int(i64::MIN), true), Ok(NumericValue::Int(i64::MIN)));
        assert_eq!(apply(NumericUnaryOp::Negate, NumericValue::Int(i64::MIN), true), Err(NumericFailure::Overflow));
        assert_eq!(apply(NumericUnaryOp::Negate, NumericValue::Int(i64::MIN), false), Ok(NumericValue::Float(9_223_372_036_854_775_808.0)));
        assert_eq!(std::mem::size_of::<NumericUnaryOp>(), 1);
    }
}
