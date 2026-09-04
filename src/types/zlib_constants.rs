//! Purpose:
//! Defines the `ZLIB_ENCODING_*` integer constants ext-zlib exposes.
//! Single source of truth for the three framings `gzencode()` / `zlib_encode()` select between.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when registering predefined constants.
//! - `crate::codegen::prescan` when materializing constant literal values.
//! - `crate::name_resolver::names` when recognizing builtin constant names.
//!
//! Key details:
//! - The values are zlib's `windowBits` arguments, not opaque tags: -15 asks `deflateInit2_` for a
//!   RAW stream with a 32 KiB window, 15 for the same window with a zlib header, and 31 for the
//!   same again with the gzip flag added. MEASURED on `php -n` 8.5.6: `-15`, `15`, `31`.
//! - A domain table of its own rather than an entry in `stream_constants`: these name a
//!   compression FRAMING for string functions, not a stream capability, and the compression
//!   wrappers select their framing from the URL scheme instead.
//! - php refuses anything else with a ValueError naming all three, which the `gz*` prelude raises.

/// Tuple of `(name, value)` pairs for ext-zlib's encoding constants.
pub(crate) use elephc_builtin_contract::php_constants::ZLIB_INT_CONSTANTS;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the three encodings carry php's own values.
    ///
    /// They are `windowBits` arguments, so a wrong value would not be rejected — it would select a
    /// different framing and produce bytes no reader expects.
    #[test]
    fn zlib_encodings_match_php() {
        assert_eq!(
            ZLIB_INT_CONSTANTS,
            &[
                ("ZLIB_ENCODING_RAW", -15),
                ("ZLIB_ENCODING_DEFLATE", 15),
                ("ZLIB_ENCODING_GZIP", 31),
            ]
        );
    }
}
