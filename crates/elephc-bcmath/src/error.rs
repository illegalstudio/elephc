//! Purpose:
//! Defines typed BCMath failures, stable C ABI status codes, and PHP-compatible messages.
//!
//! Called from:
//! - Decimal operations before errors cross either the Rust or C ABI boundary.
//! - `crate` C ABI wrappers when publishing the last operation error.
//!
//! Key details:
//! - Status codes are stable and shared by AOT codegen and Magician.
//! - Messages retain the function and argument context needed by PHP throwables.

/// Successful C ABI operation.
pub const BCMATH_OK: i32 = 0;
/// A numeric operand was not well formed.
pub const BCMATH_ERR_MALFORMED: i32 = 1;
/// A scale was outside PHP's accepted range.
pub const BCMATH_ERR_SCALE_RANGE: i32 = 2;
/// Division, modulo, or a negative power of zero failed.
pub const BCMATH_ERR_DIV_ZERO: i32 = 3;
/// A square root received a negative number.
pub const BCMATH_ERR_SQRT_NEGATIVE: i32 = 4;
/// A power operand that must be integral had a fractional part.
pub const BCMATH_ERR_POW_FRACTIONAL: i32 = 5;
/// An exponent or precision could not be represented safely.
pub const BCMATH_ERR_POW_RANGE: i32 = 6;
/// Modular exponentiation received an invalid integral operand.
pub const BCMATH_ERR_POWMOD: i32 = 7;
/// A rounding mode was outside the supported PHP enumeration.
pub const BCMATH_ERR_ROUND_MODE: i32 = 8;

/// A typed BCMath failure with enough context to reproduce PHP's message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BcError {
    /// A class operation with PHP's method-specific diagnostic and stable ABI status.
    Number {
        /// Existing decimal-domain status, shared with procedural ABI consumers.
        status: i32,
        /// Fully contextualized PHP diagnostic.
        message: String,
    },
    /// A numeric string failed BCMath's decimal grammar.
    Malformed {
        /// PHP function name.
        func: &'static str,
        /// One-based PHP argument position.
        arg_pos: u32,
        /// PHP argument name without `$`.
        arg_name: &'static str,
    },
    /// A scale was outside `0..=2147483647`.
    ScaleRange {
        /// PHP function name.
        func: &'static str,
        /// One-based PHP argument position.
        arg_pos: u32,
    },
    /// A division-like operation used a zero divisor.
    DivisionByZero {
        /// PHP function name, used to select php-src's exact wording.
        func: &'static str,
    },
    /// `bcsqrt()` received a negative operand.
    SqrtNegative,
    /// A power operand that PHP requires to be integral had a fractional part.
    PowFractional {
        /// PHP function name.
        func: &'static str,
        /// One-based PHP argument position.
        arg_pos: u32,
        /// PHP argument name without `$`.
        arg_name: &'static str,
    },
    /// `bcpow()` received an exponent outside the supported integer range.
    PowRange,
    /// `bcpowmod()` received a negative exponent.
    PowModNegativeExponent,
    /// `bcround()` received a precision outside the internal signed range.
    PrecisionRange,
    /// `bcround()` received an unsupported mode.
    RoundMode,
}

impl BcError {
    /// Returns the stable integer status used by the C ABI.
    pub fn status_code(&self) -> i32 {
        match self {
            Self::Number { status, .. } => *status,
            Self::Malformed { .. } => BCMATH_ERR_MALFORMED,
            Self::ScaleRange { .. } => BCMATH_ERR_SCALE_RANGE,
            Self::DivisionByZero { .. } => BCMATH_ERR_DIV_ZERO,
            Self::SqrtNegative => BCMATH_ERR_SQRT_NEGATIVE,
            Self::PowFractional { .. } => BCMATH_ERR_POW_FRACTIONAL,
            Self::PowRange | Self::PrecisionRange => BCMATH_ERR_POW_RANGE,
            Self::PowModNegativeExponent => BCMATH_ERR_POWMOD,
            Self::RoundMode => BCMATH_ERR_ROUND_MODE,
        }
    }

    /// Formats php-src-compatible exception text for the failure.
    pub fn php_message(&self) -> String {
        match self {
            Self::Number { message, .. } => message.clone(),
            Self::Malformed {
                func,
                arg_pos,
                arg_name,
            } => format!(
                "{func}(): Argument #{arg_pos} (${arg_name}) is not well-formed"
            ),
            Self::ScaleRange { func, arg_pos } => format!(
                "{func}(): Argument #{arg_pos} ($scale) must be between 0 and 2147483647"
            ),
            Self::DivisionByZero { func: "bcmod" | "bcpowmod" } => {
                "Modulo by zero".to_string()
            }
            Self::DivisionByZero { func: "bcpow" } => "Negative power of zero".to_string(),
            Self::DivisionByZero { .. } => "Division by zero".to_string(),
            Self::SqrtNegative => {
                "bcsqrt(): Argument #1 ($num) must be greater than or equal to 0".to_string()
            }
            Self::PowFractional {
                func,
                arg_pos,
                arg_name,
            } => format!(
                "{func}(): Argument #{arg_pos} (${arg_name}) cannot have a fractional part"
            ),
            Self::PowRange => "bcpow(): Argument #2 ($exponent) is too large".to_string(),
            Self::PowModNegativeExponent => {
                "bcpowmod(): Argument #2 ($exponent) must be greater than or equal to 0"
                    .to_string()
            }
            Self::PrecisionRange => {
                "bcround(): Argument #2 ($precision) must be between -9223372036854775808 and 2147483647".to_string()
            }
            Self::RoundMode => {
                "bcround(): Argument #3 ($mode) must be a valid rounding mode (RoundingMode::*)"
                    .to_string()
            }
        }
    }
}
