//! Purpose:
//! Converts native URI component storage into the PHP array-cell units the prelude charges
//! against `memory_limit`.
//!
//! Called from:
//! - `crate::rfc3986::RfcUri::charge_units` and `crate::whatwg::WhatWgUri::charge_units`.
//!
//! Key details:
//! - One unit is one packed PHP array cell (eight bytes). A retained Rust string costs one
//!   header unit plus `ceil(bytes / 8)` payload units; the estimate mirrors the prelude's
//!   path-node accounting rather than the allocator's exact block size.

/// Returns the PHP array-cell charge for one retained native string.
pub(crate) fn string_charge_units(text: &str) -> i64 {
    let bytes = i64::try_from(text.len()).unwrap_or(i64::MAX);
    // Four units cover the string header (32 bytes); the rest covers its payload.
    4 + bytes.saturating_add(7) / 8
}

/// Returns the charge for an optional retained native string.
pub(crate) fn optional_string_charge_units(text: Option<&str>) -> i64 {
    text.map_or(0, string_charge_units)
}

/// PHP's recomposed-URI length ceiling. uriparser/`ext/uri` store serialized
/// lengths in a signed `int`, so a recomposition at or above `INT_MAX`
/// characters must fail instead of overflowing (GHSA-mg3p-mxj9-g2mm).
pub(crate) const PHP_URI_MAX_RECOMPOSED_LEN: usize = i32::MAX as usize;

/// Returns whether a recomposed URI length fits PHP's `int`-sized string limit.
pub(crate) fn within_php_uri_len(len: usize) -> bool {
    len < PHP_URI_MAX_RECOMPOSED_LEN
}

#[cfg(test)]
mod tests {
    //! Focused coverage for the PHP string-length ceiling used by recomposition.

    use super::*;

    /// Verifies the `INT_MAX` boundary is exclusive: exactly `INT_MAX`
    /// characters must fail, matching `ext/uri`'s post-fix behavior.
    #[test]
    fn php_uri_recomposition_limit_is_exclusive() {
        assert!(within_php_uri_len(0));
        assert!(within_php_uri_len(PHP_URI_MAX_RECOMPOSED_LEN - 1));
        assert!(!within_php_uri_len(PHP_URI_MAX_RECOMPOSED_LEN));
        assert!(!within_php_uri_len(PHP_URI_MAX_RECOMPOSED_LEN + 1));
    }
}
