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
pub(crate) const JSON_INT_CONSTANTS: &[(&str, i64)] = &[
    // Encoding flags (bitmask passed to json_encode).
    ("JSON_HEX_TAG", 1),
    ("JSON_HEX_AMP", 2),
    ("JSON_HEX_APOS", 4),
    ("JSON_HEX_QUOT", 8),
    ("JSON_FORCE_OBJECT", 16),
    ("JSON_NUMERIC_CHECK", 32),
    ("JSON_UNESCAPED_SLASHES", 64),
    ("JSON_PRETTY_PRINT", 128),
    ("JSON_UNESCAPED_UNICODE", 256),
    ("JSON_PARTIAL_OUTPUT_ON_ERROR", 512),
    ("JSON_PRESERVE_ZERO_FRACTION", 1024),
    ("JSON_INVALID_UTF8_IGNORE", 1_048_576),
    ("JSON_INVALID_UTF8_SUBSTITUTE", 2_097_152),
    ("JSON_THROW_ON_ERROR", 4_194_304),
    // Decoding flags (bitmask passed to json_decode / json_validate).
    ("JSON_OBJECT_AS_ARRAY", 1),
    ("JSON_BIGINT_AS_STRING", 2),
    // Error codes returned by json_last_error().
    ("JSON_ERROR_NONE", 0),
    ("JSON_ERROR_DEPTH", 1),
    ("JSON_ERROR_STATE_MISMATCH", 2),
    ("JSON_ERROR_CTRL_CHAR", 3),
    ("JSON_ERROR_SYNTAX", 4),
    ("JSON_ERROR_UTF8", 5),
    ("JSON_ERROR_RECURSION", 6),
    ("JSON_ERROR_INF_OR_NAN", 7),
    ("JSON_ERROR_UNSUPPORTED_TYPE", 8),
    ("JSON_ERROR_INVALID_PROPERTY_NAME", 9),
    ("JSON_ERROR_UTF16", 10),
];

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
