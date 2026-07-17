//! Purpose:
//! Defines the `session_status()` PHP integer constants.
//! Keeps `ext/session` status-code values in one source of truth.
//!
//! Called from:
//! - `crate::name_resolver::names` when recognizing builtin global constants.
//! - `crate::types::checker::driver::init` when registering predefined constants.
//! - `crate::codegen::prescan` when materializing constant literal values.
//!
//! Key details:
//! - Values must match PHP's `ext/session` constants exactly (`PHP_SESSION_DISABLED`,
//!   `PHP_SESSION_NONE`, `PHP_SESSION_ACTIVE`) so `session_status()` comparisons and
//!   `defined()` checks fold correctly with zero runtime `define()` calls.

/// Tuple of `(name, value)` pairs for every `ext/session` integer constant.
///
/// Entries: `("PHP_SESSION_DISABLED", 0)`, `("PHP_SESSION_NONE", 1)`,
/// `("PHP_SESSION_ACTIVE", 2)`.
pub(crate) const SESSION_INT_CONSTANTS: &[(&str, i64)] = &[
    ("PHP_SESSION_DISABLED", 0),
    ("PHP_SESSION_NONE", 1),
    ("PHP_SESSION_ACTIVE", 2),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies each `PHP_SESSION_*` constant equals PHP's `ext/session` value.
    #[test]
    fn session_constants_match_php_values() {
        let expected: &[(&str, i64)] = &[
            ("PHP_SESSION_DISABLED", 0),
            ("PHP_SESSION_NONE", 1),
            ("PHP_SESSION_ACTIVE", 2),
        ];
        for (name, value) in expected {
            let entry = SESSION_INT_CONSTANTS
                .iter()
                .find(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("{name} defined"));
            assert_eq!(entry.1, *value, "{name} value mismatch");
        }
    }

    /// Asserts no duplicate names exist in `SESSION_INT_CONSTANTS`.
    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = SESSION_INT_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate session constant name");
    }
}
