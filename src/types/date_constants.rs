//! Purpose:
//! Defines the `ext/date` integer constants exposed as PHP predefined constants.
//! Currently the `SUNFUNCS_RET_*` return-format selectors for `date_sunrise()`/`date_sunset()`.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when registering predefined constant types.
//! - `crate::codegen::prescan` when materializing constant literal values.
//!
//! Key details:
//! - Values must match PHP's `ext/date` constants exactly so user code comparing or passing
//!   these selectors behaves identically.

/// Tuple of `(name, value)` pairs for every `ext/date` integer constant.
///
/// The `SUNFUNCS_RET_*` constants are the `$returnFormat` selector passed to
/// `date_sunrise()` / `date_sunset()`: `TIMESTAMP` (0) yields a Unix timestamp,
/// `STRING` (1, the default) an `"HH:MM"` string, and `DOUBLE` (2) the hour of the
/// day as a float. They are deprecated in PHP 8.1 alongside the functions themselves.
pub(crate) use elephc_builtin_contract::php_constants::DATE_INT_CONSTANTS;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the `SUNFUNCS_RET_*` selectors carry PHP's 0/1/2 values.
    #[test]
    fn sunfuncs_ret_values_match_php() {
        let find = |name: &str| {
            DATE_INT_CONSTANTS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| *v)
                .expect("constant defined")
        };
        assert_eq!(find("SUNFUNCS_RET_TIMESTAMP"), 0);
        assert_eq!(find("SUNFUNCS_RET_STRING"), 1);
        assert_eq!(find("SUNFUNCS_RET_DOUBLE"), 2);
    }

    /// Asserts no duplicate names exist in `DATE_INT_CONSTANTS`.
    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = DATE_INT_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate date constant name");
    }
}
