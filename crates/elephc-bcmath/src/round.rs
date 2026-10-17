//! Purpose:
//! Implements BCMath ceiling, floor, and the eight PHP rounding modes.
//!
//! Called from:
//! - Public Rust operations used by Magician.
//! - C ABI wrappers used by AOT-generated programs.
//!
//! Key details:
//! - Rounding operates on decimal digits and never converts through binary floating point.
//! - Positive and negative precision are both supported.

use crate::error::BcError;
use crate::format::format_bcmath_number;
use crate::num::{add_small, append_zeros, is_zero_digits, normalize_digits, BcNum};
use crate::parse::parse_bcmath_number_for;

/// Returns the least integer greater than or equal to a BCMath numeric string.
pub fn bc_ceil(value: &str) -> Result<String, BcError> {
    integer_boundary(value, true)
}

/// Returns the greatest integer less than or equal to a BCMath numeric string.
pub fn bc_floor(value: &str) -> Result<String, BcError> {
    integer_boundary(value, false)
}

/// Rounds a BCMath numeric string at a signed precision using mode `1..=8`.
pub fn bc_round(value: &str, precision: i64, mode: i64) -> Result<String, BcError> {
    if !(1..=8).contains(&mode) {
        return Err(BcError::RoundMode);
    }
    if precision > i64::from(i32::MAX) { return Err(BcError::PrecisionRange); }
    let number = parse_bcmath_number_for(value, "bcround", 1, "num")?;
    let drop_count = i128::from(number.scale) - i128::from(precision);
    if drop_count <= 0 {
        return format_bcmath_number(&number, precision.max(0) as i32);
    }

    let drop_count = drop_count as usize;
    let split = number.digits.len().saturating_sub(drop_count);
    let mut retained = if split == 0 {
        vec![0]
    } else {
        normalize_digits(number.digits[..split].to_vec())
    };
    let (cmp_half, discarded_nonzero) = discarded_relation(&number.digits, drop_count);
    let increment = should_increment(
        mode,
        number.negative,
        cmp_half,
        discarded_nonzero,
        retained.last().copied().unwrap_or(0),
    );
    if increment {
        retained = add_small(&retained, 1);
    }

    if precision >= 0 {
        format_bcmath_number(
            &BcNum::new(number.negative, retained, precision as i32),
            precision as i32,
        )
    } else {
        if is_zero_digits(&retained) { return Ok("0".to_string()); }
        let digits = append_zeros(retained, precision.unsigned_abs() as usize);
        format_bcmath_number(&BcNum::new(number.negative, digits, 0), 0)
    }
}

/// Implements `bcceil` and `bcfloor` with sign-aware fractional adjustment.
fn integer_boundary(value: &str, ceil: bool) -> Result<String, BcError> {
    let func = if ceil { "bcceil" } else { "bcfloor" };
    let number = parse_bcmath_number_for(value, func, 1, "num")?;
    let scale = number.scale as usize;
    let split = number.digits.len().saturating_sub(scale);
    let integer_digits = if split == 0 {
        vec![0]
    } else {
        normalize_digits(number.digits[..split].to_vec())
    };
    let fraction_nonzero = if scale >= number.digits.len() {
        !number.is_zero()
    } else {
        number.digits[split..].iter().any(|digit| *digit != 0)
    };
    let away_from_zero = fraction_nonzero
        && ((ceil && !number.negative) || (!ceil && number.negative));
    let digits = if away_from_zero {
        add_small(&integer_digits, 1)
    } else {
        integer_digits
    };
    format_bcmath_number(&BcNum::new(number.negative, digits, 0), 0)
}

/// Compares discarded digits with one half and reports whether any are nonzero.
fn discarded_relation(digits: &[u8], drop_count: usize) -> (i8, bool) {
    if drop_count > digits.len() {
        return (-1, !is_zero_digits(digits));
    }
    let discarded = &digits[digits.len() - drop_count..];
    let any_nonzero = discarded.iter().any(|digit| *digit != 0);
    let relation = match discarded[0].cmp(&5) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => {
            if discarded[1..].iter().any(|digit| *digit != 0) {
                1
            } else {
                0
            }
        }
    };
    (relation, any_nonzero)
}

/// Decides whether the retained magnitude advances for one PHP rounding mode.
fn should_increment(
    mode: i64,
    negative: bool,
    cmp_half: i8,
    discarded_nonzero: bool,
    retained_last: u8,
) -> bool {
    match mode {
        1 => cmp_half >= 0,
        2 => cmp_half > 0,
        3 => cmp_half > 0 || (cmp_half == 0 && retained_last % 2 == 1),
        4 => cmp_half > 0 || (cmp_half == 0 && retained_last % 2 == 0),
        5 => !negative && discarded_nonzero,
        6 => negative && discarded_nonzero,
        7 => false,
        8 => discarded_nonzero,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies ceiling and floor move only when the sign/direction requires it.
    #[test]
    fn integer_boundaries_match_php() {
        assert_eq!(bc_ceil("1.1").expect("ceil"), "2");
        assert_eq!(bc_ceil("-1.1").expect("ceil"), "-1");
        assert_eq!(bc_floor("1.9").expect("floor"), "1");
        assert_eq!(bc_floor("-1.1").expect("floor"), "-2");
    }

    /// Verifies half-up and negative precision fixtures from PHP 8.4.
    #[test]
    fn round_half_up_and_negative_precision() {
        assert_eq!(bc_round("3.5", 0, 1).expect("round"), "4");
        assert_eq!(bc_round("5.045", 2, 1).expect("round"), "5.05");
        assert_eq!(bc_round("345", -2, 1).expect("round"), "300");
    }

    /// Verifies all eight mode decisions around positive and negative half values.
    #[test]
    fn all_rounding_modes_match_php_enumeration() {
        let expected = [
            (1, "10", "-10"),
            (2, "9", "-9"),
            (3, "10", "-10"),
            (4, "9", "-9"),
            (5, "10", "-9"),
            (6, "9", "-10"),
            (7, "9", "-9"),
            (8, "10", "-10"),
        ];
        for (mode, positive, negative) in expected {
            assert_eq!(bc_round("9.5", 0, mode).expect("round"), positive);
            assert_eq!(bc_round("-9.5", 0, mode).expect("round"), negative);
        }
    }
}
