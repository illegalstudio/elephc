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
pub(crate) const DATE_INT_CONSTANTS: &[(&str, i64)] = &[
    ("SUNFUNCS_RET_TIMESTAMP", 0),
    ("SUNFUNCS_RET_STRING", 1),
    ("SUNFUNCS_RET_DOUBLE", 2),
    // ext/calendar: calendar selectors for cal_to_jd()/cal_from_jd()/cal_days_in_month()/cal_info().
    ("CAL_GREGORIAN", 0),
    ("CAL_JULIAN", 1),
    ("CAL_JEWISH", 2),
    ("CAL_FRENCH", 3),
    ("CAL_NUM_CALS", 4),
    // jddayofweek() $mode selectors.
    ("CAL_DOW_DAYNO", 0),
    ("CAL_DOW_LONG", 1),
    ("CAL_DOW_SHORT", 2),
    // jdmonthname() $mode selectors.
    ("CAL_MONTH_GREGORIAN_SHORT", 0),
    ("CAL_MONTH_GREGORIAN_LONG", 1),
    ("CAL_MONTH_JULIAN_SHORT", 2),
    ("CAL_MONTH_JULIAN_LONG", 3),
    ("CAL_MONTH_JEWISH", 4),
    ("CAL_MONTH_FRENCH", 5),
    // easter_date()/easter_days() $mode selectors.
    ("CAL_EASTER_DEFAULT", 0),
    ("CAL_EASTER_ROMAN", 1),
    ("CAL_EASTER_ALWAYS_GREGORIAN", 2),
    ("CAL_EASTER_ALWAYS_JULIAN", 3),
    // jdtojewish() Hebrew-formatting flags (accepted; Hebrew output is not produced).
    ("CAL_JEWISH_ADD_ALAFIM_GERESH", 2),
    ("CAL_JEWISH_ADD_ALAFIM", 4),
    ("CAL_JEWISH_ADD_GERESHAYIM", 8),
];

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
