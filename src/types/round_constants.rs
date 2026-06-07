//! Purpose:
//! Defines PHP `PHP_ROUND_HALF_*` rounding-mode integer constants exposed by elephc.
//! Keeps the round-mode values in one source of truth for type checking and codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when registering predefined constants.
//! - `crate::codegen::prescan` when materializing constant literal values.
//!
//! Key details:
//! - Values must match PHP's `round()` mode constants exactly. `round()` currently implements
//!   `PHP_ROUND_HALF_UP` (the default, round half away from zero) and `PHP_ROUND_HALF_EVEN`
//!   (banker's rounding); `PHP_ROUND_HALF_DOWN` and `PHP_ROUND_HALF_ODD` are recognized constants
//!   but rejected with a diagnostic until specialized.

/// Tuple of `(name, value)` pairs for PHP `round()` rounding-mode integer constants.
pub(crate) const ROUND_INT_CONSTANTS: &[(&str, i64)] = &[
    ("PHP_ROUND_HALF_UP", 1),
    ("PHP_ROUND_HALF_DOWN", 2),
    ("PHP_ROUND_HALF_EVEN", 3),
    ("PHP_ROUND_HALF_ODD", 4),
];

/// Returns the rounding-mode value for a `PHP_ROUND_HALF_*` constant name, if recognized.
pub(crate) fn round_mode_value(name: &str) -> Option<i64> {
    ROUND_INT_CONSTANTS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, value)| *value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the default `PHP_ROUND_HALF_UP` mode value is 1, matching PHP.
    #[test]
    fn half_up_is_one() {
        assert_eq!(round_mode_value("PHP_ROUND_HALF_UP"), Some(1));
    }

    /// Asserts the four PHP round-mode values to guard against accidental drift.
    #[test]
    fn known_round_mode_values_match_php() {
        assert_eq!(round_mode_value("PHP_ROUND_HALF_DOWN"), Some(2));
        assert_eq!(round_mode_value("PHP_ROUND_HALF_EVEN"), Some(3));
        assert_eq!(round_mode_value("PHP_ROUND_HALF_ODD"), Some(4));
        assert_eq!(round_mode_value("NOT_A_MODE"), None);
    }
}
