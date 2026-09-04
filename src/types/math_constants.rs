//! Purpose:
//! Defines PHP math integer constants exposed by elephc.
//! Keeps `round()`'s rounding-mode constants in one source of truth for type checking and codegen.
//!
//! Called from:
//! - `crate::types::checker` when registering predefined constants.
//! - `crate::codegen_support::prescan` when materializing constant literal values.
//! - `crate::name_resolver::names` when deciding which names bypass symbol-table resolution.
//!
//! Key details:
//! - Values must match php-src's `PHP_ROUND_HALF_*` exactly: `round()` validates its `$mode`
//!   against this contiguous `1..=4` range and raises `ValueError` for anything else, so a
//!   mismatch here would turn a valid PHP call into a runtime exception (or, worse, silently
//!   pick the wrong tie-breaking rule).

/// Tuple of `(name, value)` pairs for PHP math integer constants.
///
/// `round()` uses these constants to select how exact `.5` ties are broken.
pub(crate) use elephc_builtin_contract::php_constants::MATH_INT_CONSTANTS;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the rounding-mode constants carry php-src's exact values.
    ///
    /// `round()`'s omitted-`$mode` default and its `ValueError` range check both assume
    /// `PHP_ROUND_HALF_UP == 1` and a contiguous `1..=4` range.
    #[test]
    fn round_modes_match_php() {
        assert_eq!(
            MATH_INT_CONSTANTS,
            &[
                ("PHP_ROUND_HALF_UP", 1),
                ("PHP_ROUND_HALF_DOWN", 2),
                ("PHP_ROUND_HALF_EVEN", 3),
                ("PHP_ROUND_HALF_ODD", 4),
            ]
        );
    }

    /// Asserts no duplicate names exist in `MATH_INT_CONSTANTS`.
    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = MATH_INT_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate math constant name");
    }
}
