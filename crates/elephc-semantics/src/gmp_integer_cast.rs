//! Purpose:
//! Proves checked GMP integer sources without substituting PHP intval string grammar.
//!
//! Called from:
//! - Original-source compiler validation before destructive constant folding.
//!
//! Key details:
//! - Canonical decimal magnitudes are exact; unfamiliar native spellings stay unknown.
//! - PHP strict_types domain errors keep priority over checked float range failures.

use crate::{numeric_cast::{checked_numeric_cast, NumericCastFailure, NumericCastInput, NumericCastTarget},
    numeric_unary::NumericValue};

/// Proves reviewed scalar inputs and defers native GMP grammar and strict PHP binding to their owner.
pub fn prove_gmp_integer_cast(input: NumericCastInput<'_>, strict_types: bool)
    -> Result<Option<i64>, NumericCastFailure> {
    match input {
        NumericCastInput::Float(_) | NumericCastInput::Bool(_) | NumericCastInput::Null if strict_types => return Ok(None),
        NumericCastInput::String(text) => {
            let text = text.trim_matches(|ch: char| ch.is_ascii_whitespace());
            let digits = text.strip_prefix('-').unwrap_or(text);
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit())
                || (digits.len() > 1 && digits.starts_with('0')) { return Ok(None); }
            return text.parse().map(Some).map_err(|_| NumericCastFailure::Range);
        }
        _ => {}
    }
    match checked_numeric_cast(NumericCastTarget::Int, input)? {
        NumericValue::Int(value) => Ok(Some(value)), NumericValue::Float(_) => unreachable!("integer target"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact decimal integer sources keep both endpoints and reject range loss without f64.
    #[test]
    fn strict_gmp_integer_proof_preserves_decimal_magnitudes() {
        for (text, expected) in [("9223372036854775807", i64::MAX), ("-9223372036854775808", i64::MIN),
            (" 42\n", 42), ("-0", 0)] {
            assert_eq!(prove_gmp_integer_cast(NumericCastInput::String(text), false), Ok(Some(expected)));
        }
        for text in ["9223372036854775808", "-9223372036854775809"] {
            assert_eq!(prove_gmp_integer_cast(NumericCastInput::String(text), false), Err(NumericCastFailure::Range));
        }
    }

    /// Prefixes, internal whitespace, signs and PHP strict binding are decided by the native contract.
    #[test]
    fn strict_gmp_integer_proof_defers_distinct_native_grammar() {
        for text in ["0xff", "0o17", "077", "1 2", "0 xff", "-0xff", "+42", "1e3", "12tail", "1\0"] {
            assert_eq!(prove_gmp_integer_cast(NumericCastInput::String(text), false), Ok(None), "{text:?}");
        }
        for input in [NumericCastInput::Float(f64::INFINITY), NumericCastInput::Bool(true), NumericCastInput::Null] {
            assert_eq!(prove_gmp_integer_cast(input, true), Ok(None));
        }
    }

    /// Weak numeric adaptation checks the original floating source before range information is lost.
    #[test]
    fn strict_gmp_integer_proof_checks_original_float_range() {
        for value in [f64::INFINITY, f64::NAN, 9223372036854775808.0, -9223372036854777856.0] {
            assert_eq!(prove_gmp_integer_cast(NumericCastInput::Float(value), false), Err(NumericCastFailure::Range));
        }
        assert_eq!(prove_gmp_integer_cast(NumericCastInput::Float(7.9), false), Ok(Some(7)));
        assert_eq!(prove_gmp_integer_cast(NumericCastInput::Unsupported, false), Err(NumericCastFailure::UnsupportedKind));
    }
}
