//! Purpose:
//! Implements the character-oriented iconv functions: `iconv_strlen`, `iconv_substr`,
//! `iconv_strpos`, and `iconv_strrpos`.
//!
//! Called from:
//! - `crate::abi::dispatch` for the AOT runtime and Magician's matching eval bindings.
//!
//! Key details:
//! - Every operation works on the fixed-width UCS-4LE superset, so offsets and lengths
//!   are character counts exactly like php-src.
//! - `strlen`/`substr` convert the whole subject up front and fail on the first bad byte;
//!   the search pair instead walks the subject one character at a time and stops at the
//!   first match, so a match before a malformed tail is still reported.
//! - An out-of-range `$offset` is a `ValueError` in PHP 8, which is a distinct outcome
//!   from a diagnostic-plus-`false` failure; `SearchFailure` keeps the two apart.

use crate::encoding_state::effective_charset;
use crate::error::{IconvError, IconvResult};
use crate::text::{
    from_superset, open_to_superset, superset_len, superset_slice, to_superset, SUPERSET_WIDTH,
};

/// Why a search-family call did not produce an integer position.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SearchFailure {
    /// The conversion itself failed and PHP emits a diagnostic before returning `false`.
    Conversion(IconvError),
    /// PHP 8 throws `ValueError` because `$offset` is outside `$haystack`.
    OffsetOutOfRange,
}

impl From<IconvError> for SearchFailure {
    /// Wraps a conversion failure so `?` works inside the search helpers.
    fn from(error: IconvError) -> Self {
        SearchFailure::Conversion(error)
    }
}

/// Message php-src attaches to the out-of-range `$offset` `ValueError`.
pub fn offset_value_error_message(function: &str) -> String {
    format!("{function}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)")
}

/// Counts the characters of `input` when read as `charset`.
pub fn strlen(input: &[u8], charset: Option<&[u8]>) -> IconvResult<usize> {
    let charset = effective_charset(charset);
    superset_len(&to_superset(input, &charset)?)
}

/// Extracts a character-indexed slice of `input`, PHP `iconv_substr()` style.
///
/// `offset` and `length` follow PHP's `substr()` conventions: negative values count
/// from the end and an omitted length runs to the end of the string.
pub fn substr(
    input: &[u8],
    offset: i64,
    length: Option<i64>,
    charset: Option<&[u8]>,
) -> IconvResult<Vec<u8>> {
    let charset = effective_charset(charset);
    let units = to_superset(input, &charset)?;
    let total = superset_len(&units)?;
    let Some((start, count)) = resolve_range(total, offset, length) else {
        return Ok(Vec::new());
    };
    from_superset(superset_slice(&units, start, count), &charset)
}

/// Finds the first occurrence of `needle` at or after `offset`.
pub fn strpos(
    haystack: &[u8],
    needle: &[u8],
    offset: i64,
    charset: Option<&[u8]>,
) -> Result<Option<usize>, SearchFailure> {
    // php-src answers an empty needle before it converts anything.
    if needle.is_empty() && offset >= 0 {
        return Ok(None);
    }
    let charset = effective_charset(charset);
    let offset = if offset < 0 {
        // A negative offset needs the full character count, which php-src measures with
        // `iconv_strlen()` semantics, so a malformed subject fails here first.
        let total = superset_len(&to_superset(haystack, &charset)?)? as i64;
        let resolved = offset + total;
        if resolved < 0 {
            return Err(SearchFailure::OffsetOutOfRange);
        }
        resolved
    } else {
        offset
    };
    if needle.is_empty() {
        return Ok(None);
    }
    let needle_units = to_superset(needle, &charset)?;
    scan(haystack, &needle_units, offset as usize, false, &charset)
}

/// Finds the last occurrence of `needle` anywhere in `haystack`.
pub fn strrpos(
    haystack: &[u8],
    needle: &[u8],
    charset: Option<&[u8]>,
) -> Result<Option<usize>, SearchFailure> {
    if needle.is_empty() {
        return Ok(None);
    }
    let charset = effective_charset(charset);
    let needle_units = to_superset(needle, &charset)?;
    scan(haystack, &needle_units, 0, true, &charset)
}

/// Walks `haystack` one character at a time looking for `needle`.
///
/// This mirrors php-src's `_php_iconv_strpos`: a step that produces nothing ends the
/// scan without recording an error, a step that produces a character records any failure
/// but keeps the character, and a forward search stops at its first match. The
/// out-of-range `$offset` check runs only when no failure was recorded.
fn scan(
    haystack: &[u8],
    needle_units: &[u8],
    offset: usize,
    reverse: bool,
    charset: &[u8],
) -> Result<Option<usize>, SearchFailure> {
    let needle_chars = needle_units.len() / SUPERSET_WIDTH;
    let mut converter = open_to_superset(charset)?;
    let mut units: Vec<u8> = Vec::with_capacity(haystack.len() * SUPERSET_WIDTH);
    let mut input = haystack;
    let mut failure: Option<IconvError> = None;
    let mut found: Option<usize> = None;
    let mut count = 0usize;
    let mut more = !haystack.is_empty();
    while more {
        let mut buf = [0u8; SUPERSET_WIDTH];
        more = !input.is_empty();
        let (produced, error) = converter.step(&mut input, &mut buf, more);
        if produced == 0 {
            break;
        }
        if let Some(error) = error {
            failure = Some(error);
        }
        units.extend_from_slice(&buf[..produced]);
        if count >= offset && count + 1 >= needle_chars {
            let start = count + 1 - needle_chars;
            if start >= offset && ends_at(&units, needle_units, count) {
                found = Some(start);
                if !reverse {
                    break;
                }
            }
        }
        count += 1;
    }
    if let Some(error) = failure {
        return Err(SearchFailure::Conversion(error));
    }
    if offset > count {
        return Err(SearchFailure::OffsetOutOfRange);
    }
    Ok(found)
}

/// Reports whether `needle` occupies the characters ending at index `last`.
fn ends_at(units: &[u8], needle: &[u8], last: usize) -> bool {
    let end = (last + 1) * SUPERSET_WIDTH;
    if end > units.len() || needle.len() > end {
        return false;
    }
    units[end - needle.len()..end] == *needle
}

/// Normalizes PHP's `$offset`/`$length` pair into a character range, or `None` when empty.
fn resolve_range(total: usize, offset: i64, length: Option<i64>) -> Option<(usize, usize)> {
    let total_signed = total as i64;
    let start = if offset < 0 {
        (total_signed + offset).max(0)
    } else {
        offset.min(total_signed)
    };
    let available = total_signed - start;
    let count = match length {
        None => available,
        Some(length) if length < 0 => available + length,
        Some(length) => length.min(available),
    };
    if count <= 0 {
        return None;
    }
    Some((start as usize, count as usize))
}
