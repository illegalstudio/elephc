//! Purpose:
//! Represents owned scalar constants independently of compiler AST and runtime cells.
//!
//! Called from:
//! - Compiler constant folding and shared constant-expression analysis.
//!
//! Key details:
//! - PHP value truthiness differs from bit-exact identity used for propagation.
//! - Floating signed zero and NaN payloads retain their original bit patterns.

/// Represents a scalar value extracted from a literal expression.
/// Shared value facts preserve PHP kinds without depending on AST or runtime storage.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl ScalarValue {
    /// Returns whether this scalar value is truthy per PHP rules.
    pub fn truthy(&self) -> bool {
        match self {
            ScalarValue::Null => false,
            ScalarValue::Bool(value) => *value,
            ScalarValue::Int(value) => *value != 0,
            ScalarValue::Float(value) => *value != 0.0,
            ScalarValue::String(value) => !value.is_empty() && value != "0",
        }
    }

    /// Returns whether two scalar values denote the *same constant*, i.e. whether one can be
    /// substituted for the other without changing any observable byte of the program.
    ///
    /// This is deliberately stricter than `PartialEq`: floats are compared by bit pattern, so
    /// `0.0` and `-0.0` stay distinct (`echo -0.0` prints `-0`) and two NANs with the same
    /// payload merge. Use it for constant identity — merging ternary/match arms into one
    /// propagated fact — never for PHP's `==` or `===`, which are value comparisons.
    pub fn same_constant(&self, other: &Self) -> bool {
        match (self, other) {
            (ScalarValue::Float(left), ScalarValue::Float(right)) => {
                left.to_bits() == right.to_bits()
            }
            (left, right) => left == right,
        }
    }

    /// Returns whether this scalar value is a floating-point NAN.
    ///
    /// Folding a NAN to bool is value-correct (`truthy()` already answers `true`, since
    /// `NAN != 0.0`) but WARNING-incorrect under PHP 8.5, which reports every NAN-to-bool
    /// coercion. Cast consumers keep that diagnostic on the runtime path.
    pub fn is_nan_float(&self) -> bool {
        matches!(self, ScalarValue::Float(value) if value.is_nan())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Propagation identity distinguishes signed zero while PHP truthiness keeps byte-string rules.
    #[test]
    fn constant_scalar_identity_keeps_zero_nan_and_truthiness() {
        assert!(!ScalarValue::Float(0.0).same_constant(&ScalarValue::Float(-0.0)));
        let value = ScalarValue::Float(f64::from_bits(0x7ff8_1234_5678_9abc));
        assert!(value.same_constant(&value));
        assert!(value.is_nan_float() && value.truthy());
        for (text, truthy) in [("", false), ("0", false), ("00", true), ("0\0", true)] {
            assert_eq!(ScalarValue::String(text.into()).truthy(), truthy);
        }
    }
}
