//! Purpose:
//! Defines PHP sort-flag integer constants exposed by elephc.
//! Keeps the `SORT_*` flag values in one source of truth for type checking and codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when registering predefined constants.
//!
//! Key details:
//! - Values must match PHP's sort flag constants exactly. The sort builtins currently honor the
//!   flag only when it matches the array's element family (the common case); `SORT_REGULAR` and a
//!   flag matching the element type sort correctly, while a mismatched flag (e.g. `SORT_STRING` on
//!   a numeric array) is not yet specialized.

/// Tuple of `(name, value)` pairs for PHP sort-flag integer constants.
///
/// Used as the optional `$flags` argument to `sort()`, `rsort()`, `asort()`, `arsort()`,
/// `ksort()`, and `krsort()`.
pub(crate) const SORT_INT_CONSTANTS: &[(&str, i64)] = &[
    ("SORT_REGULAR", 0),
    ("SORT_NUMERIC", 1),
    ("SORT_STRING", 2),
    ("SORT_DESC", 3),
    ("SORT_ASC", 4),
    ("SORT_LOCALE_STRING", 5),
    ("SORT_NATURAL", 6),
    ("SORT_FLAG_CASE", 8),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the default `SORT_REGULAR` flag value is zero, matching PHP.
    #[test]
    fn sort_regular_is_zero() {
        let entry = SORT_INT_CONSTANTS
            .iter()
            .find(|(name, _)| *name == "SORT_REGULAR")
            .expect("SORT_REGULAR defined");
        assert_eq!(entry.1, 0);
    }

    /// Asserts the well-known PHP sort flag values to guard against accidental drift.
    #[test]
    fn known_sort_flag_values_match_php() {
        for (name, expected) in [
            ("SORT_NUMERIC", 1),
            ("SORT_STRING", 2),
            ("SORT_NATURAL", 6),
            ("SORT_FLAG_CASE", 8),
        ] {
            let entry = SORT_INT_CONSTANTS
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .unwrap_or_else(|| panic!("{} defined", name));
            assert_eq!(entry.1, expected, "{} value", name);
        }
    }

    /// Asserts no duplicate names exist in `SORT_INT_CONSTANTS`.
    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = SORT_INT_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate sort constant name");
    }
}
