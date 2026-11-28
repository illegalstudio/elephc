//! Purpose:
//! Computes PHP integer power while preserving exact integers and ordered float promotion.
//!
//! Called from:
//! - Compiler constant folding and non-overflow eval numeric operators.
//!
//! Key details:
//! - Promotion starts at the first overflowing multiplication, preserving PHP rounding.
//! - Value-domain admission and checked-overflow policy remain owned by callers.

/// The PHP value kind produced by an integer power with ordinary promotion enabled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PowerResult { Int(i64), Float(f64) }

/// Evaluates signed integer power without discarding integer precision before overflow.
pub fn php_integer_power(base: i64, exponent: i64) -> PowerResult {
    if exponent < 0 { return PowerResult::Float((base as f64).powf(exponent as f64)); }
    if exponent == 0 { return PowerResult::Int(1); }
    if base == 0 { return PowerResult::Int(0); }
    let (mut carry, mut factor, mut exponent) = (1i64, base, exponent);
    while exponent > 0 {
        if exponent & 1 != 0 {
            exponent -= 1;
            match carry.checked_mul(factor) {
                Some(value) => carry = value,
                None => return PowerResult::Float((carry as f64 * factor as f64)
                    * (factor as f64).powf(exponent as f64)),
            }
        } else {
            exponent /= 2;
            match factor.checked_mul(factor) {
                Some(value) => factor = value,
                None => return PowerResult::Float(carry as f64
                    * (factor as f64 * factor as f64).powf(exponent as f64)),
            }
        }
    }
    PowerResult::Int(carry)
}
