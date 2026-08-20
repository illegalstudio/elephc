//! Purpose:
//! Implements PHP's `iconv()` byte-string transcoding.
//!
//! Called from:
//! - `crate::abi::elephc_iconv_convert` for the AOT runtime.
//! - Magician's `iconv` eval binding.
//!
//! Key details:
//! - An empty `$from_encoding` / `$to_encoding` resolves to PHP's `default_charset`
//!   rather than to `iconv.internal_encoding`, matching php-src's generic engine.
//! - `//TRANSLIT` is handed to libc untouched, but a `//IGNORE` target additionally
//!   enables php-src's own skip-the-rejected-byte loop, because glibc still reports
//!   `EILSEQ` for such a conversion.
//! - Diagnostics name the charsets exactly as the caller spelled them, which is what
//!   php-src passes to its error formatter.

use crate::encoding_state::effective_charset;
use crate::error::{IconvError, IconvResult};
use crate::ffi::Converter;

/// Converts `input` from one charset into another, PHP `iconv()` style.
///
/// Returns the transcoded bytes, or the failure php-src would report as a warning
/// (unknown charset pair) or a notice (truncated or rejected input).
pub fn convert(from: &[u8], to: &[u8], input: &[u8]) -> IconvResult<Vec<u8>> {
    let resolved_from = effective_charset(Some(from));
    let resolved_to = effective_charset(Some(to));
    let reported = |error: IconvError| {
        error.with_reported_charsets(
            &String::from_utf8_lossy(from),
            &String::from_utf8_lossy(to),
        )
    };
    let mut converter = Converter::open(&resolved_from, &resolved_to).map_err(reported)?;
    converter.convert_all_ignoring(input, ignores_illegal_sequences(&resolved_to))
}

/// Reports whether a target charset asks libc to drop bytes it cannot represent.
///
/// php-src recognizes exactly the `//IGNORE` and `//IGNORE//TRANSLIT` suffixes, and it
/// matches them case-sensitively.
fn ignores_illegal_sequences(charset: &[u8]) -> bool {
    charset.ends_with(b"//IGNORE") || charset.ends_with(b"//IGNORE//TRANSLIT")
}
