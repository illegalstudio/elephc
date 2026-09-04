//! Purpose:
//! Defines JSON constants exposed as PHP integer constants.
//! Keeps `ext/json` flag and error-code values in one source of truth.
//!
//! Called from:
//! - `crate::types::checker` when registering predefined constants.
//! - `crate::codegen::prescan` when materializing constant literal values.
//!
//! Key details:
//! - Values must match PHP's `ext/json` constants exactly for bitmask and error-code parity.

/// Tuple of `(name, value)` pairs for every `ext/json` integer constant.
///
/// Example entries: `("JSON_HEX_TAG", 1)`, `("JSON_ERROR_NONE", 0)`.
pub(crate) use elephc_builtin_contract::php_constants::JSON_INT_CONSTANTS;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies `JSON_PRETTY_PRINT` equals 128, matching PHP's ext/json value.
    #[test]
    fn json_pretty_print_is_128() {
        let entry = JSON_INT_CONSTANTS
            .iter()
            .find(|(name, _)| *name == "JSON_PRETTY_PRINT")
            .expect("JSON_PRETTY_PRINT defined");
        assert_eq!(entry.1, 128);
    }

    /// Asserts no duplicate names exist in `JSON_INT_CONSTANTS`.
    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = JSON_INT_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate JSON constant name");
    }
}
