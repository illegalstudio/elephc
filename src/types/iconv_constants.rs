//! Purpose:
//! Defines the constants PHP's iconv extension publishes.
//! Single source of truth for `ICONV_MIME_DECODE_*`, `ICONV_IMPL`, and `ICONV_VERSION`.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when registering predefined constants.
//! - `crate::codegen_support::prescan` when materializing constant literal values.
//! - `crate::name_resolver::names` when recognizing builtin constant names.
//!
//! Key details:
//! - The two integer values must match PHP 8.x exactly (`php -r 'echo ICONV_MIME_DECODE_STRICT;'`).
//! - `ICONV_IMPL` is derived from the compilation target, because Apple platforms ship GNU
//!   libiconv while elephc's Linux support targets glibc.
//! - `ICONV_VERSION` is the `unknown` spelling php-src itself uses when it cannot identify
//!   its iconv provider: elephc compiles ahead of time, so the runtime libc version that
//!   will serve the conversions is not knowable while compiling.

/// Integer constants the iconv extension registers.
pub(crate) const ICONV_INT_CONSTANTS: &[(&str, i64)] = &[
    // Reject encoded-words that RFC 2047 would not allow at that position.
    ("ICONV_MIME_DECODE_STRICT", 1),
    // Keep undecodable text verbatim instead of failing the whole call.
    ("ICONV_MIME_DECODE_CONTINUE_ON_ERROR", 2),
];

/// Returns the `ICONV_IMPL` value for one compilation target.
pub(crate) fn iconv_impl(is_macos: bool) -> &'static str {
    if is_macos {
        "libiconv"
    } else {
        "glibc"
    }
}

/// Value reported by `ICONV_VERSION`.
pub(crate) const ICONV_VERSION: &str = "unknown";
