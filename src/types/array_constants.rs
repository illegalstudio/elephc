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
/// `array_filter()` uses the `ARRAY_FILTER_*` constants to select which callback arguments
/// are passed; `count()` uses the `COUNT_*` constants to select flat or recursive counting.
pub(crate) use elephc_builtin_contract::php_constants::ARRAY_INT_CONSTANTS;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies `ARRAY_FILTER_USE_VALUE` is NOT declared, because php does not define it.
    ///
    /// Measured on `php -n` 8.5.6: `defined("ARRAY_FILTER_USE_VALUE")` is false, while
    /// `ARRAY_FILTER_USE_KEY` and `ARRAY_FILTER_USE_BOTH` are both true. Mode 0 is still the
    /// value mode, and php's own `ValueError` message spells the name — but spelling it in php
    /// source is a fatal there, so declaring it here would let such a program compile.
    #[test]
    fn array_filter_use_value_is_not_a_php_constant() {
        assert!(
            !ARRAY_INT_CONSTANTS
                .iter()
                .any(|(name, _)| *name == "ARRAY_FILTER_USE_VALUE"),
            "php does not define ARRAY_FILTER_USE_VALUE"
        );
        for name in ["ARRAY_FILTER_USE_KEY", "ARRAY_FILTER_USE_BOTH"] {
            assert!(
                ARRAY_INT_CONSTANTS.iter().any(|(declared, _)| *declared == name),
                "{name} is a php constant and must stay declared"
            );
        }
    }

    /// Verifies `count()`'s mode constants carry php-src's exact values.
    ///
    /// `count()`'s omitted-`$mode` default and its `ValueError` range check both assume
    /// `COUNT_NORMAL == 0` and `COUNT_RECURSIVE == 1`.
    #[test]
    fn count_modes_match_php() {
        let normal = ARRAY_INT_CONSTANTS
            .iter()
            .find(|(name, _)| *name == "COUNT_NORMAL")
            .expect("COUNT_NORMAL defined");
        let recursive = ARRAY_INT_CONSTANTS
            .iter()
            .find(|(name, _)| *name == "COUNT_RECURSIVE")
            .expect("COUNT_RECURSIVE defined");
        assert_eq!((normal.1, recursive.1), (0, 1));
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
