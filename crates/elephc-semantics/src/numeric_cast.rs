//! Purpose:
//! Defines checked explicit numeric casts independently of implicit value conversion.
//!
//! Called from:
//! - Source validation, scalar folding and eval/native conversion adapters.
//!
//! Key details:
//! - Integer strings retain exact decimal truncation without a binary64 round trip.
//! - Floating results retain IEEE infinity, NaN and signed zero.

use crate::{numeric::{checked_float_to_int, checked_string_to_int, NumericFailure},
    numeric_string::scan_numeric_prefix, numeric_unary::NumericValue};

/// Explicit numeric result contract, shared with typed runtime call metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericCastTarget { Int, Float }

/// Original scalar input, before any PHP normalization or representation change.
#[derive(Clone, Copy, Debug)]
pub enum NumericCastInput<'a> {
    Int(i64), Float(f64), Bool(bool), Null, String(&'a str), Unsupported,
}

/// A cast's invalid value domain or unsupported original PHP kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericCastFailure { Range, InvalidString, InvalidBase, UnsupportedKind }

impl NumericCastFailure {
    /// Supplies identical diagnostics for compile-time and runtime checked casts.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Range => "--strict-casts: integer conversion requires a finite value in range",
            Self::InvalidString => "--strict-casts: numeric conversion requires a complete numeric string",
            Self::InvalidBase => "--strict-casts: integer base must be 0 or between 2 and 36",
            Self::UnsupportedKind => "--strict-casts: numeric conversion requires a scalar or an explicit object cast contract",
        }
    }
}

/// Converts a numeric parse failure without conflating type and value errors.
pub fn numeric_cast_failure(failure: NumericFailure) -> NumericCastFailure {
    match failure {
        NumericFailure::InvalidString => NumericCastFailure::InvalidString,
        _ => NumericCastFailure::Range,
    }
}

/// Evaluates an explicit numeric cast after selecting the checked conversion policy.
pub fn checked_numeric_cast(target: NumericCastTarget, input: NumericCastInput<'_>)
    -> Result<NumericValue, NumericCastFailure> {
    use NumericCastInput as Input;
    if target == NumericCastTarget::Int {
        let value = match input {
            Input::Int(value) => value,
            Input::Float(value) => checked_float_to_int(value).map_err(numeric_cast_failure)?,
            Input::Bool(value) => i64::from(value), Input::Null => 0,
            Input::String(value) => checked_string_to_int(value).map_err(numeric_cast_failure)?,
            Input::Unsupported => return Err(NumericCastFailure::UnsupportedKind),
        };
        return Ok(NumericValue::Int(value));
    }
    Ok(NumericValue::Float(match input {
        Input::Int(value) => value as f64, Input::Float(value) => value,
        Input::Bool(value) => f64::from(u8::from(value)), Input::Null => 0.0,
        Input::String(value) => {
            let scan = scan_numeric_prefix(value).filter(|scan| scan.is_fully_numeric())
                .ok_or(NumericCastFailure::InvalidString)?;
            scan.text.parse().map_err(|_| NumericCastFailure::InvalidString)?
        }
        Input::Unsupported => return Err(NumericCastFailure::UnsupportedKind),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact decimal truncation accepts endpoints, exponents and every PHP whitespace byte.
    #[test]
    fn strict_numeric_cast_exact_strings_preserve_integer_endpoints() {
        for (text, expected) in [("9223372036854775807", i64::MAX),
            ("9223372036854775807.9", i64::MAX), ("-9223372036854775808.9", i64::MIN),
            ("92233720368547758070e-1", i64::MAX), (".9", 0), ("-7.9", -7),
            ("\u{b}+42\u{c}", 42), ("0e999999999999999999999999", 0)] {
            assert_eq!(checked_numeric_cast(NumericCastTarget::Int, NumericCastInput::String(text)),
                Ok(NumericValue::Int(expected)), "{text:?}");
        }
        for text in ["9223372036854775808", "-9223372036854775809", "1e99"] {
            assert_eq!(checked_numeric_cast(NumericCastTarget::Int, NumericCastInput::String(text)),
                Err(NumericCastFailure::Range), "{text}");
        }
    }

    /// Full-string grammar excludes prefixes and embedded NUL while float results keep IEEE bits.
    #[test]
    fn strict_numeric_cast_domains_preserve_ieee_and_reject_prefixes() {
        for target in [NumericCastTarget::Int, NumericCastTarget::Float] {
            for text in ["", "12tail", "0x10", "1e", "INF", "NAN", "1\0", "\u{a0}1"] {
                assert_eq!(checked_numeric_cast(target, NumericCastInput::String(text)),
                    Err(NumericCastFailure::InvalidString), "{target:?} {text:?}");
            }
            assert_eq!(checked_numeric_cast(target, NumericCastInput::Unsupported), Err(NumericCastFailure::UnsupportedKind));
        }
        for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 9_223_372_036_854_775_808.0] {
            assert_eq!(checked_numeric_cast(NumericCastTarget::Int, NumericCastInput::Float(value)), Err(NumericCastFailure::Range));
        }
        for bits in [0u64, 1 << 63, f64::INFINITY.to_bits(), 0x7ff8_1234_5678_9abc] {
            let NumericValue::Float(value) = checked_numeric_cast(NumericCastTarget::Float,
                NumericCastInput::Float(f64::from_bits(bits))).unwrap() else { panic!("float result"); };
            assert_eq!(value.to_bits(), bits);
        }
        assert!(matches!(checked_numeric_cast(NumericCastTarget::Float, NumericCastInput::String("1e9999")),
            Ok(NumericValue::Float(value)) if value.is_infinite()));
    }
}
