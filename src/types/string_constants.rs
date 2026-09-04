//! Purpose:
//! Defines PHP string-related integer constants exposed by elephc.
//! Keeps `str_pad()`'s padding-mode constants in one source of truth for type checking and codegen.
//!
//! Called from:
//! - `crate::types::checker` when registering predefined constants.
//! - `crate::codegen_support::prescan` when materializing constant literal values.
//!
//! Key details:
//! - Values must match PHP's string extension constants exactly: `str_pad()` validates its
//!   `$pad_type` against this 0..=2 range and raises `ValueError` for anything else, so a
//!   mismatch here would turn a valid PHP call into a runtime exception.

/// Tuple of `(name, value)` pairs for PHP string integer constants.
///
/// `str_pad()` uses these constants to select which side of the input is padded.
pub(crate) use elephc_builtin_contract::php_constants::STRING_INT_CONSTANTS;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the padding-mode constants carry php-src's exact values.
    ///
    /// `str_pad()`'s omitted-`$pad_type` default and its `ValueError` range check both assume
    /// `STR_PAD_RIGHT == 1` and a contiguous `0..=2` range.
    #[test]
    fn str_pad_modes_match_php() {
        assert_eq!(
            STRING_INT_CONSTANTS,
            &[
                ("STR_PAD_LEFT", 0),
                ("STR_PAD_RIGHT", 1),
                ("STR_PAD_BOTH", 2),
            ]
        );
    }

    /// Asserts no duplicate names exist in `STRING_INT_CONSTANTS`.
    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = STRING_INT_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate string constant name");
    }
}
