//! Purpose:
//! Defines PHP string-related integer constants exposed by elephc.
//! Keeps `str_pad()` padding modes and `mb_convert_case()` `MB_CASE_*` modes in one
//! source of truth for type checking and codegen.
//!
//! Called from:
//! - `crate::types::checker` when registering predefined constants.
//! - `crate::codegen_support::prescan` when materializing constant literal values.
//!
//! Key details:
//! - Values must match PHP's string / mbstring extension constants exactly: `str_pad()`
//!   validates `$pad_type` against `0..=2`, and `mb_convert_case()` validates `$mode`
//!   against the contiguous `MB_CASE_*` range `0..=7`. A mismatch here would turn a
//!   valid PHP call into a runtime `ValueError`.

/// Tuple of `(name, value)` pairs for PHP string integer constants.
///
/// `str_pad()` uses the `STR_PAD_*` constants to select which side of the input is padded.
/// `mb_convert_case()` uses the `MB_CASE_*` constants to select Unicode case conversion.
pub(crate) const STRING_INT_CONSTANTS: &[(&str, i64)] = &[
    ("STR_PAD_LEFT", 0),
    ("STR_PAD_RIGHT", 1),
    ("STR_PAD_BOTH", 2),
    ("MB_CASE_UPPER", 0),
    ("MB_CASE_LOWER", 1),
    ("MB_CASE_TITLE", 2),
    ("MB_CASE_FOLD", 3),
    ("MB_CASE_UPPER_SIMPLE", 4),
    ("MB_CASE_LOWER_SIMPLE", 5),
    ("MB_CASE_TITLE_SIMPLE", 6),
    ("MB_CASE_FOLD_SIMPLE", 7),
];

/// Inclusive upper bound of PHP's `MB_CASE_*` mode integers (`MB_CASE_FOLD_SIMPLE`).
pub(crate) const MB_CASE_MODE_MAX: i64 = 7;

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
            &STRING_INT_CONSTANTS[..3],
            &[
                ("STR_PAD_LEFT", 0),
                ("STR_PAD_RIGHT", 1),
                ("STR_PAD_BOTH", 2),
            ]
        );
    }

    /// Verifies `MB_CASE_*` constants match php-src's `PHP_UNICODE_CASE_*` integers.
    #[test]
    fn mb_case_modes_match_php() {
        assert_eq!(
            &STRING_INT_CONSTANTS[3..],
            &[
                ("MB_CASE_UPPER", 0),
                ("MB_CASE_LOWER", 1),
                ("MB_CASE_TITLE", 2),
                ("MB_CASE_FOLD", 3),
                ("MB_CASE_UPPER_SIMPLE", 4),
                ("MB_CASE_LOWER_SIMPLE", 5),
                ("MB_CASE_TITLE_SIMPLE", 6),
                ("MB_CASE_FOLD_SIMPLE", 7),
            ]
        );
        assert_eq!(MB_CASE_MODE_MAX, 7);
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
