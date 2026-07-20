//! Purpose:
//! Defines PHP's `E_*` error-level integer constants (the `error_reporting`
//! bitmask levels). Keeps the `ext/standard` error-severity values in one
//! source of truth.
//!
//! Called from:
//! - `crate::name_resolver::names` when recognizing builtin global constants.
//! - `crate::types::checker::driver::init` when registering predefined constants.
//! - `crate::codegen::prescan` when materializing constant literal values.
//!
//! Key details:
//! - Values must match PHP's Zend error-level constants exactly so bitmask
//!   comparisons (`E_ALL & ~E_NOTICE`, `trigger_error($m, E_USER_WARNING)`)
//!   fold correctly with zero runtime `define()` calls.
//! - These constants are global (available in CLI and `--web` alike), matching
//!   PHP where the `E_*` levels are always defined.
//! - `E_ALL` is `32767` on PHP 8.x (`E_STRICT`, value `2048`, is deprecated but
//!   its bit remains set in `E_ALL`).

/// Tuple of `(name, value)` pairs for every PHP `E_*` error-level constant.
///
/// Values mirror Zend's `zend_errors.h` severity bitmask so programs can build
/// and compare `error_reporting()` masks and pass levels to `trigger_error()`.
pub(crate) const ERROR_LEVEL_CONSTANTS: &[(&str, i64)] = &[
    ("E_ERROR", 1),
    ("E_WARNING", 2),
    ("E_PARSE", 4),
    ("E_NOTICE", 8),
    ("E_CORE_ERROR", 16),
    ("E_CORE_WARNING", 32),
    ("E_COMPILE_ERROR", 64),
    ("E_COMPILE_WARNING", 128),
    ("E_USER_ERROR", 256),
    ("E_USER_WARNING", 512),
    ("E_USER_NOTICE", 1024),
    ("E_STRICT", 2048),
    ("E_RECOVERABLE_ERROR", 4096),
    ("E_DEPRECATED", 8192),
    ("E_USER_DEPRECATED", 16384),
    ("E_ALL", 32767),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies each `E_*` constant equals PHP's Zend error-level value.
    #[test]
    fn error_level_constants_match_php_values() {
        let expected: &[(&str, i64)] = &[
            ("E_ERROR", 1),
            ("E_WARNING", 2),
            ("E_PARSE", 4),
            ("E_NOTICE", 8),
            ("E_CORE_ERROR", 16),
            ("E_CORE_WARNING", 32),
            ("E_COMPILE_ERROR", 64),
            ("E_COMPILE_WARNING", 128),
            ("E_USER_ERROR", 256),
            ("E_USER_WARNING", 512),
            ("E_USER_NOTICE", 1024),
            ("E_STRICT", 2048),
            ("E_RECOVERABLE_ERROR", 4096),
            ("E_DEPRECATED", 8192),
            ("E_USER_DEPRECATED", 16384),
            ("E_ALL", 32767),
        ];
        for (name, value) in expected {
            let entry = ERROR_LEVEL_CONSTANTS
                .iter()
                .find(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("{name} defined"));
            assert_eq!(entry.1, *value, "{name} value mismatch");
        }
    }

    /// Asserts no duplicate names exist in `ERROR_LEVEL_CONSTANTS`.
    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = ERROR_LEVEL_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate error-level constant name");
    }
}
