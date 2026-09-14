//! Purpose:
//! Implements mbstring forward/reverse searches, substring extraction, and counting.
//!
//! Called from:
//! - mb_strpos/stripos/strrpos/strripos, strstr variants, and mb_substr_count adapters.
//!
//! Key details:
//! - Insensitive search uses simple case folding, preserving codepoint offsets.
//! - PHP searches raw UTF-8 for sensitive position queries, including malformed bytes.
//! - Converted invalid units use 0xFF, never a valid replacement character like '?'.

use crate::encoding::{Encoding, Substitute, UnicodeEncoding};
use crate::error::{MbError, MbResult};
use crate::unicode::{self, CaseMode};

/// Direction and case policy for PHP's four position-search variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchMode {
    pub reverse: bool,
    pub insensitive: bool,
}

impl SearchMode {
    /// Returns the PHP function name used to format an offset failure.
    fn function(self) -> &'static str {
        match (self.reverse, self.insensitive) {
            (false, false) => "mb_strpos",
            (true, false) => "mb_strrpos",
            (false, true) => "mb_stripos",
            (true, true) => "mb_strripos",
        }
    }
}

/// Finds a character offset with PHP's signed offset bounds and reverse-search semantics.
pub fn strpos(haystack: &[u8], needle: &[u8], offset: i64, encoding: Encoding, mode: SearchMode) -> MbResult<Option<usize>> {
    let raw_utf8 = encoding.name().starts_with("UTF-8") && !mode.insensitive;
    let haystack = if raw_utf8 { haystack.to_vec() } else { normalized(haystack, encoding, mode.insensitive)? };
    let needle = if raw_utf8 { needle.to_vec() } else { normalized(needle, encoding, mode.insensitive)? };
    let position = offset_pointer(&haystack, offset).ok_or_else(|| MbError::argument(
        mode.function(), 3, "offset", "must be contained in argument #1 ($haystack)"))?;
    let (start, end) = if mode.reverse && offset < 0 {
        let needle_chars = count_starts(&needle);
        let end = advance(&haystack, position, needle_chars).unwrap_or(haystack.len());
        (0, end.min(haystack.len()))
    } else {
        (position, haystack.len())
    };
    let found = if needle.is_empty() {
        Some(if mode.reverse { end } else { start })
    } else if start > end || needle.len() > end.saturating_sub(start) {
        None
    } else if mode.reverse {
        haystack[start..end].windows(needle.len()).rposition(|part| part == needle).map(|index| start + index)
    } else {
        haystack[start..end].windows(needle.len()).position(|part| part == needle).map(|index| start + index)
    };
    // PHP permits a final UTF-8 width-table step to pass a truncated final sequence.
    // Count that virtual terminator region without reading outside the Rust slice.
    Ok(found.map(|position| count_starts(&haystack[..position.min(haystack.len())])
        + position.saturating_sub(haystack.len())))
}

/// Returns the prefix or suffix surrounding the first/last matched needle.
pub fn strstr(haystack: &[u8], needle: &[u8], before: bool, encoding: Encoding, mode: SearchMode, substitution: Substitute) -> MbResult<Option<Vec<u8>>> {
    let Some(position) = strpos(haystack, needle, 0, encoding, mode)? else { return Ok(None); };
    let (start, length) = if before { (0, position) } else { (position, usize::MAX) };
    Ok(Some(super::slicing::slice(haystack, start, length, encoding, substitution)?))
}

/// Counts non-overlapping occurrences after PHP's invalid-unit normalization.
pub fn substr_count(haystack: &[u8], needle: &[u8], encoding: Encoding) -> MbResult<usize> {
    if needle.is_empty() { return Err(MbError::empty("mb_substr_count", 2, "needle")); }
    let haystack = normalized(haystack, encoding, false)?;
    let needle = normalized(needle, encoding, false)?;
    if needle.is_empty() { return Err(MbError::empty("mb_substr_count", 2, "needle")); }
    let (mut offset, mut count) = (0, 0);
    while offset + needle.len() <= haystack.len() {
        let Some(index) = haystack[offset..].windows(needle.len()).position(|part| part == needle) else { break; };
        offset += index + needle.len();
        count += 1;
    }
    Ok(count)
}

/// Converts to UTF-8 with PHP's reserved invalid-byte marker and optional simple folding.
fn normalized(input: &[u8], encoding: Encoding, fold: bool) -> MbResult<Vec<u8>> {
    let decoded = encoding.decode(input);
    let points = if fold {
        unicode::convert_case(&decoded.points, CaseMode::FoldSimple, encoding.uses_turkish_case())
    } else { decoded.points };
    if !fold && encoding.raw_conversion_destination() {
        return Ok(points.into_iter().map(|code| if code <= 0xff { code as u8 } else { 0xff }).collect());
    }
    let mut output = Vec::new();
    for code in points {
        if code > 0x10ffff {
            output.push(0xff);
        } else {
            output.extend_from_slice(&UnicodeEncoding::Utf8.encode(&[code], Substitute::default()));
        }
    }
    Ok(output)
}

/// Counts bytes other than UTF-8 continuation bytes, including raw invalid lead bytes.
fn count_starts(input: &[u8]) -> usize {
    input.iter().filter(|&&byte| !(0x80..=0xbf).contains(&byte)).count()
}

/// Resolves a signed character offset with PHP's distinct forward/backward byte scans.
fn offset_pointer(input: &[u8], offset: i64) -> Option<usize> {
    if offset >= 0 { return advance(input, 0, offset as usize); }
    let mut remaining = offset.unsigned_abs();
    let mut position = input.len();
    while remaining != 0 {
        position = position.checked_sub(1)?;
        if !(0x80..=0xbf).contains(&input[position]) { remaining -= 1; }
    }
    Some(position)
}

/// Advances by the UTF-8 leading-byte width table, allowing a final truncated sequence.
fn advance(input: &[u8], mut position: usize, count: usize) -> Option<usize> {
    for _ in 0..count {
        let &byte = input.get(position)?;
        position += match byte { 0xc2..=0xdf => 2, 0xe0..=0xef => 3, 0xf0..=0xf4 => 4, _ => 1 };
    }
    Some(position)
}
