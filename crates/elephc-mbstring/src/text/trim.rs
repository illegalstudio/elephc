//! Purpose:
//! Implements mbstring's character-list trimming and display-width trimming.
//!
//! Called from:
//! - mb_trim/ltrim/rtrim and mb_strimwidth adapters.
//!
//! Key details:
//! - A character list is a set of decoded characters, without byte-trim range syntax.
//! - Trim markers are appended as original bytes, even when malformed for the encoding.

use crate::encoding::{Encoding, Substitute, BAD_INPUT};
use crate::error::{MbError, MbResult};
use crate::unicode::character_width;

/// Side or sides of a string from which matching characters are removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimSide { Left, Right, Both }

/// Removes PHP's default Unicode whitespace, or explicitly selected decoded characters.
pub fn trim(input: &[u8], characters: Option<&[u8]>, side: TrimSide, encoding: Encoding, substitution: Substitute) -> MbResult<Vec<u8>> {
    let mut characters = match characters {
        Some(chars) => encoding.decode(chars).points,
        None => vec![0x20, 0x0c, 0x0a, 0x0d, 0x09, 0x0b, 0x00, 0xa0, 0x1680,
            0x2000, 0x2001, 0x2002, 0x2003, 0x2004, 0x2005, 0x2006, 0x2007,
            0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000, 0x85, 0x180e],
    };
    if characters.is_empty() { return Ok(input.to_vec()); }
    characters.sort_unstable();
    characters.dedup();
    let decoded = encoding.decode(input);
    let (mut left, mut right) = (0, decoded.points.len());
    if side != TrimSide::Right {
        while left < right && characters.binary_search(&decoded.points[left]).is_ok() { left += 1; }
    }
    if side != TrimSide::Left {
        while right > left && characters.binary_search(&decoded.points[right - 1]).is_ok() { right -= 1; }
    }
    if left == 0 && right == decoded.points.len() { return Ok(input.to_vec()); }
    super::slicing::slice(input, left, right - left, encoding, substitution)
}

/// Trims to terminal width from a signed character offset, appending a marker when needed.
///
/// Adapters also emit PHP's deprecation when the original width is negative.
pub fn strimwidth(
    input: &[u8], start: i64, width: i64, marker: &[u8], encoding: Encoding, substitution: Substitute,
) -> MbResult<Vec<u8>> {
    let total = encoding.strlen(input);
    let from = if start < 0 { total as i128 + start as i128 } else { start as i128 };
    if from < 0 || from > total as i128 {
        return Err(MbError::argument("mb_strimwidth", 2, "start", "is out of range"));
    }
    let from = from as usize;
    let decoded = encoding.decode(input);
    let width = if width < 0 {
        let prefix = super::slicing::slice(input, 0, from, encoding, substitution)?;
        width as i128 + super::strwidth(input, encoding) as i128 - super::strwidth(&prefix, encoding) as i128
    } else { width as i128 };
    if width < 0 {
        return Err(MbError::argument("mb_strimwidth", 3, "width", "is out of range"));
    }
    let suffix = &decoded.points[from.min(decoded.points.len())..];
    let suffix_width: usize = suffix.iter().copied().map(character_width).sum();
    if suffix_width as i128 <= width {
        if suffix.contains(&BAD_INPUT) { return Ok(encoding.encode(suffix, substitution)); }
        if from == 0 { return Ok(input.to_vec()); }
        return super::slicing::slice(input, from, usize::MAX, encoding, substitution);
    }
    let marker_width = super::strwidth(marker, encoding);
    if width <= marker_width as i128 { return Ok(marker.to_vec()); }
    let mut budget = width as usize - marker_width;
    let mut count = 0;
    for &point in suffix {
        let char_width = character_width(point);
        if char_width > budget { break; }
        budget -= char_width;
        count += 1;
    }
    let mut output = encoding.encode(&suffix[..count], substitution);
    output.extend_from_slice(marker);
    Ok(output)
}
