//! Purpose:
//! Provides the fixed-width UCS-4LE view php-src's character-oriented iconv functions
//! operate on, plus the conversion back into the caller's charset.
//!
//! Called from:
//! - `crate::search` for `iconv_strlen`, `iconv_substr`, `iconv_strpos`, `iconv_strrpos`.
//!
//! Key details:
//! - `UCS-4LE` is php-src's `GENERIC_SUPERSET_NAME`; every code point is exactly four bytes,
//!   so character indices are byte indices divided by four.
//! - Charset failures are reported with `UCS-4LE` as the destination, which is the spelling
//!   php-src puts in `iconv_strlen(): Wrong encoding, conversion from "X" to "UCS-4LE" ...`.

use crate::error::{IconvError, IconvResult};
use crate::ffi::Converter;

/// php-src's generic superset charset: a fixed four-byte encoding of every code point.
pub const SUPERSET: &[u8] = b"UCS-4LE";

/// Bytes one character occupies in the superset encoding.
pub const SUPERSET_WIDTH: usize = 4;

/// Decodes `input` into the fixed-width superset used for character indexing.
pub fn to_superset(input: &[u8], charset: &[u8]) -> IconvResult<Vec<u8>> {
    let mut converter = open_to_superset(charset)?;
    converter.convert_all(input)
}

/// Opens a decoder from `charset` into the superset, naming both sides on failure.
pub fn open_to_superset(charset: &[u8]) -> IconvResult<Converter> {
    Converter::open(charset, SUPERSET).map_err(|error| {
        error.with_reported_charsets(
            &String::from_utf8_lossy(charset),
            &String::from_utf8_lossy(SUPERSET),
        )
    })
}

/// Re-encodes superset bytes back into the caller's charset.
pub fn from_superset(units: &[u8], charset: &[u8]) -> IconvResult<Vec<u8>> {
    let mut converter = Converter::open(SUPERSET, charset).map_err(|error| {
        error.with_reported_charsets(
            &String::from_utf8_lossy(charset),
            &String::from_utf8_lossy(SUPERSET),
        )
    })?;
    converter.convert_all(units)
}

/// Returns how many characters a superset buffer holds.
pub fn superset_len(units: &[u8]) -> IconvResult<usize> {
    if units.len() % SUPERSET_WIDTH != 0 {
        return Err(IconvError::IncompleteChar);
    }
    Ok(units.len() / SUPERSET_WIDTH)
}

/// Returns the superset byte range covering characters `[start, start + count)`.
pub fn superset_slice(units: &[u8], start: usize, count: usize) -> &[u8] {
    let begin = (start * SUPERSET_WIDTH).min(units.len());
    let end = (begin + count * SUPERSET_WIDTH).min(units.len());
    &units[begin..end]
}
