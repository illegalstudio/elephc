//! Purpose:
//! Computes the target-dependent values of PHP's `ICONV_IMPL` and `ICONV_VERSION` constants.
//! The names themselves (and the fixed `ICONV_MIME_DECODE_*` values) live in the shared
//! constant catalog; this module only supplies what the catalog marks `TargetDependent`.
//!
//! Called from:
//! - `crate::codegen_support::prescan` when materializing constant literal values.
//!
//! Key details:
//! - `ICONV_IMPL` is derived from the compilation target, because Apple platforms ship GNU
//!   libiconv while elephc's Linux support targets glibc.
//! - `ICONV_VERSION` is the `unknown` spelling php-src itself uses when it cannot identify
//!   its iconv provider: elephc compiles ahead of time, so the runtime libc version that
//!   will serve the conversions is not knowable while compiling.

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
