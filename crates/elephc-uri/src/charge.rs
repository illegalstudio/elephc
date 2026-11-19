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
