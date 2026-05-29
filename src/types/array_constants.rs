//! Purpose:
//! Defines PHP array-related integer constants exposed by elephc.
//! Keeps callback-mode constants in one source of truth for type checking and codegen.
//!
//! Called from:
//! - `crate::types::checker` when registering predefined constants.
//! - `crate::codegen::prescan` when materializing constant literal values.
//!
//! Key details:
//! - Values must match PHP's array extension constants exactly for callback-mode parity.

/// Tuple of `(name, value)` pairs for PHP array integer constants.
///
/// `array_filter()` uses these constants to select which callback arguments are passed.
pub(crate) const ARRAY_INT_CONSTANTS: &[(&str, i64)] = &[
    ("ARRAY_FILTER_USE_VALUE", 0),
    ("ARRAY_FILTER_USE_BOTH", 1),
    ("ARRAY_FILTER_USE_KEY", 2),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies PHP 8.6's value-mode constant is the default mode value.
    #[test]
    fn array_filter_use_value_is_zero() {
        let entry = ARRAY_INT_CONSTANTS
            .iter()
            .find(|(name, _)| *name == "ARRAY_FILTER_USE_VALUE")
            .expect("ARRAY_FILTER_USE_VALUE defined");
        assert_eq!(entry.1, 0);
    }

    /// Asserts no duplicate names exist in `ARRAY_INT_CONSTANTS`.
    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = ARRAY_INT_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate array constant name");
    }
}
