//! Purpose:
//! Implements BCMath powers, modular powers, and square roots using exact decimal coefficients.
//!
//! Called from:
//! - Public Rust operations used by Magician.
//! - C ABI wrappers used by AOT-generated programs.
//!
//! Key details:
//! - Exponents and modular operands accept fractional zeroes but reject non-integral values.
//! - Square root uses decimal digit pairs and truncates at the requested scale.

use crate::error::BcError;
use crate::format::format_bcmath_number;
use crate::num::{
    add_small, append_zeros, cmp_digits, div_two, is_zero_digits, mod_digits, mul_digits,
    mul_small, sub_digits, BcNum,
};
use crate::ops::divide_numbers;
use crate::parse::parse_bcmath_number_for;
use crate::scale::resolve_scale;

/// Raises one BCMath decimal to an integral exponent.
pub fn bc_pow(base: &str, exponent: &str, scale: Option<i64>) -> Result<String, BcError> {
    let result_scale = resolve_scale(scale, "bcpow", 3)?;
    let mut base = parse_bcmath_number_for(base, "bcpow", 1, "num")?;
    // Insignificant fractional zeroes do not belong to the coefficient being exponentiated.
    // The caller's requested result scale independently preserves any visible zeroes.
    if base.is_zero() {
        base.scale = 0;
    } else {
        while base.scale > 0 && base.digits.last() == Some(&0) {
            base.digits.pop();
            base.scale -= 1;
        }
    }
    let exponent = parse_integral_operand(exponent, "bcpow", 2, "exponent")?;
    let exponent = signed_i64(&exponent).ok_or(BcError::PowRange)?;
    if exponent < 0 && base.is_zero() {
        return Err(BcError::DivisionByZero { func: "bcpow" });
    }

    let magnitude = exponent.unsigned_abs();
    let powered_digits = pow_digits(&base.digits, magnitude);
    let powered_scale_wide = i128::from(base.scale) * i128::from(magnitude);
    let powered_scale = i32::try_from(powered_scale_wide).map_err(|_| BcError::PowRange)?;
    let powered = BcNum::new(
        base.negative && magnitude % 2 == 1,
        powered_digits,
        powered_scale,
    );
    let result = if exponent < 0 {
        divide_numbers(&BcNum::new(false, vec![1], 0), &powered, result_scale)
    } else {
        powered
    };
    format_bcmath_number(&result, result_scale)
}

/// Computes an integral modular exponent and formats it at the selected scale.
pub fn bc_powmod(
    base: &str,
    exponent: &str,
    modulus: &str,
    scale: Option<i64>,
) -> Result<String, BcError> {
    let result_scale = resolve_scale(scale, "bcpowmod", 4)?;
    let base = parse_integral_operand(base, "bcpowmod", 1, "num")?;
    let exponent = parse_integral_operand(exponent, "bcpowmod", 2, "exponent")?;
    let modulus = parse_integral_operand(modulus, "bcpowmod", 3, "modulus")?;
    if exponent.negative {
        return Err(BcError::PowModNegativeExponent);
    }
    if modulus.is_zero() {
        return Err(BcError::DivisionByZero { func: "bcpowmod" });
    }

    let exponent_is_odd = exponent.digits.last().is_some_and(|digit| digit % 2 == 1);
    let mut exponent_digits = exponent.digits;
    let modulus_digits = modulus.digits;
    let mut factor = mod_digits(&base.digits, &modulus_digits);
    let mut result = mod_digits(&[1], &modulus_digits);
    while !is_zero_digits(&exponent_digits) {
        let (halved, bit) = div_two(&exponent_digits);
        if bit == 1 {
            result = mod_digits(&mul_digits(&result, &factor), &modulus_digits);
        }
        exponent_digits = halved;
        if !is_zero_digits(&exponent_digits) {
            factor = mod_digits(&mul_digits(&factor, &factor), &modulus_digits);
        }
    }
    format_bcmath_number(
        &BcNum::new(base.negative && exponent_is_odd, result, 0),
        result_scale,
    )
}

/// Computes a BCMath square root, truncating to the selected scale.
pub fn bc_sqrt(value: &str, scale: Option<i64>) -> Result<String, BcError> {
    let result_scale = resolve_scale(scale, "bcsqrt", 2)?;
    let value = parse_bcmath_number_for(value, "bcsqrt", 1, "num")?;
    if value.negative {
        return Err(BcError::SqrtNegative);
    }
    let power = i64::from(result_scale) * 2 - i64::from(value.scale);
    let radicand = if power >= 0 {
        append_zeros(value.digits, power as usize)
    } else {
        let drop_count = (-power) as usize;
        if drop_count >= value.digits.len() {
            vec![0]
        } else {
            value.digits[..value.digits.len() - drop_count].to_vec()
        }
    };
    let root = integer_sqrt(&radicand);
    format_bcmath_number(&BcNum::new(false, root, result_scale), result_scale)
}

/// Parses an operand that may contain fractional zeroes but must have an integral value.
pub(crate) fn parse_integral_operand(
    input: &str,
    func: &'static str,
    arg_pos: u32,
    arg_name: &'static str,
) -> Result<BcNum, BcError> {
    let mut number = parse_bcmath_number_for(input, func, arg_pos, arg_name)?;
    let scale = number.scale as usize;
    let fractional_start = number.digits.len().saturating_sub(scale);
    let has_fraction = number.digits[fractional_start..]
        .iter()
        .any(|digit| *digit != 0);
    if has_fraction {
        return Err(BcError::PowFractional {
            func,
            arg_pos,
            arg_name,
        });
    }
    if scale >= number.digits.len() {
        number.digits = vec![0];
    } else if scale > 0 {
        number.digits.truncate(number.digits.len() - scale);
    }
    number.scale = 0;
    if number.is_zero() {
        number.negative = false;
    }
    Ok(number)
}

/// Converts a parsed signed integral value to PHP's machine integer when it fits.
fn signed_i64(number: &BcNum) -> Option<i64> {
    let mut value = 0u64;
    for digit in &number.digits {
        value = value.checked_mul(10)?.checked_add(u64::from(*digit))?;
        if value > i64::MAX as u64 + u64::from(number.negative) {
            return None;
        }
    }
    if number.negative {
        if value == (1u64 << 63) { Some(i64::MIN) } else { Some(-(value as i64)) }
    } else {
        i64::try_from(value).ok()
    }
}

/// Raises an unsigned coefficient to a non-negative exponent by squaring.
fn pow_digits(base: &[u8], mut exponent: u64) -> Vec<u8> {
    let mut result = vec![1];
    let mut factor = base.to_vec();
    while exponent != 0 {
        if exponent & 1 == 1 {
            result = mul_digits(&result, &factor);
        }
        exponent >>= 1;
        if exponent != 0 {
            factor = mul_digits(&factor, &factor);
        }
    }
    result
}

/// Computes the floor of an unsigned coefficient's square root using decimal digit pairs.
fn integer_sqrt(digits: &[u8]) -> Vec<u8> {
    if is_zero_digits(digits) {
        return vec![0];
    }
    let mut root = vec![0];
    let mut remainder = vec![0];
    let mut index = 0usize;
    let first_pair_len = if digits.len() % 2 == 0 { 2 } else { 1 };
    while index < digits.len() {
        let pair_len = if index == 0 { first_pair_len } else { 2 };
        remainder = append_zeros(remainder, 2);
        let pair = if pair_len == 1 {
            digits[index]
        } else {
            digits[index] * 10 + digits[index + 1]
        };
        remainder = add_small(&remainder, pair);
        index += pair_len;

        let twenty_root = mul_small(&root, 20);
        let mut selected = 0u8;
        for candidate in 1..=9 {
            let trial = mul_small(&add_small(&twenty_root, candidate), candidate);
            if cmp_digits(&trial, &remainder).is_gt() {
                break;
            }
            selected = candidate;
        }
        let trial = mul_small(&add_small(&twenty_root, selected), selected);
        remainder = sub_digits(&remainder, &trial);
        root = add_small(&append_zeros(root, 1), selected);
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies positive and negative powers preserve requested output scale.
    #[test]
    fn power_matches_php_scale() {
        assert_eq!(bc_pow("4.2", "3", Some(2)).expect("pow"), "74.08");
        assert_eq!(bc_pow("5", "2", Some(2)).expect("pow"), "25.00");
        assert_eq!(bc_pow("2", "-3", Some(5)).expect("pow"), "0.12500");
    }

    /// Verifies integral zero fractions are accepted while nonzero fractions are rejected.
    #[test]
    fn power_exponent_integrality_matches_php() {
        assert_eq!(bc_pow("2", "2.0", Some(0)).expect("pow"), "4");
        assert!(matches!(
            bc_pow("2", "2.1", Some(0)),
            Err(BcError::PowFractional { .. })
        ));
        assert!(matches!(
            bc_pow("2", ".001", Some(0)),
            Err(BcError::PowFractional { .. })
        ));
    }

    /// Verifies modular exponentiation handles signs, scale padding, and zero modulus.
    #[test]
    fn powmod_matches_php_integral_rules() {
        assert_eq!(bc_powmod("2", "3", "5", Some(2)).expect("powmod"), "3.00");
        assert_eq!(
            bc_powmod("-2", "3", "5", Some(0)).expect("powmod"),
            "-3"
        );
        assert!(matches!(
            bc_powmod("2", "2", "0", Some(0)),
            Err(BcError::DivisionByZero { .. })
        ));
    }

    /// Verifies square root truncates the exact decimal result.
    #[test]
    fn square_root_truncates() {
        assert_eq!(bc_sqrt("2", Some(3)).expect("sqrt"), "1.414");
        assert_eq!(bc_sqrt("0.0004", Some(3)).expect("sqrt"), "0.020");
    }
}
