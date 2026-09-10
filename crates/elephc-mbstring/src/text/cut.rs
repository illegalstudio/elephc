//! Purpose:
//! Implements byte-budget mb_strcut using the encoding catalog's raw slicing paths.
//!
//! Called from:
//! - The mb_strcut backend adapter.
//!
//! Key details:
//! - Start and length are byte quantities, unlike mb_substr character positions.
//! - UTF-16 has PHP-specific BOM and trailing-surrogate handling.

use crate::encoding::{Encoding, Slicing, Substitute, UnicodeEncoding};
use crate::error::MbResult;

/// Cuts within a byte budget, rounding only at the boundaries required by the encoding.
pub fn strcut(input: &[u8], start: i64, length: Option<i64>, encoding: Encoding, _substitution: Substitute) -> MbResult<Vec<u8>> {
    let from = if start < 0 { (input.len() as i128 + start as i128).max(0) } else { start as i128 };
    let length = match length {
        None => input.len() as i128,
        Some(length) if length < 0 => (input.len() as i128 - from + length as i128).max(0),
        Some(length) => length as i128,
    };
    if from > input.len() as i128 || length == 0 { return Ok(Vec::new()); }
    let (from, length) = (from as usize, length as usize);
    if let Some(output) = encoding.cut(input, from, length) { return Ok(output); }
    if encoding.name().starts_with("UTF-8") {
        let mut from = from;
        while from > 0 && input.get(from).is_some_and(|byte| byte & 0xc0 == 0x80) { from -= 1; }
        let mut end = from.saturating_add(length).min(input.len());
        if end < input.len() {
            while end > from && input[end] & 0xc0 == 0x80 { end -= 1; }
        }
        return Ok(input[from..end].to_vec());
    }
    if matches!(encoding.unicode(), Some(UnicodeEncoding::Utf16 | UnicodeEncoding::Utf16Be | UnicodeEncoding::Utf16Le)) {
        return Ok(cut_utf16(input, from, length, encoding.unicode().unwrap()));
    }
    match encoding.slicing() {
        Slicing::Fixed(width) => {
            let from = from / width * width;
            let length = length.min(input.len() - from) / width * width;
            Ok(input[from..from + length].to_vec())
        }
        Slicing::LeadingByte(widths) => {
            let from = boundary(input, 0, from, widths);
            let end = if length >= input.len() - from { input.len() }
                else { boundary(input, from, from + length, widths) };
            Ok(input[from..end].to_vec())
        }
        // Every converted encoding supplies a legacy filter above, including UTF-16.
        Slicing::Converted => unreachable!("{} requires a dedicated byte cut", encoding.name()),
    }
}

/// Rounds a requested byte position down by walking PHP's leading-byte widths.
fn boundary(input: &[u8], mut offset: usize, requested: usize, widths: &[u8]) -> usize {
    while offset < requested {
        let next = offset + usize::from(widths[usize::from(input[offset])]);
        if next > requested { break; }
        offset = next;
    }
    offset
}

/// Applies PHP's UTF-16 cut rules, including BOM removal for automatic byte order.
fn cut_utf16(input: &[u8], mut from: usize, length: usize, encoding: UnicodeEncoding) -> Vec<u8> {
    let mut little = encoding == UnicodeEncoding::Utf16Le;
    if encoding == UnicodeEncoding::Utf16 {
        if input.len() < 2 || length < 2 { return Vec::new(); }
        little = input.starts_with(&[0xff, 0xfe]);
        if little || input.starts_with(&[0xfe, 0xff]) { from = from.max(2); }
    }
    let length = length.min(input.len().saturating_sub(from)) & !1;
    let from = from & !1;
    if length < 2 || input.len().saturating_sub(from) < 2 { return Vec::new(); }
    let mut end = (from + length).min(input.len());
    let last = if little { u16::from_le_bytes([input[end - 2], input[end - 1]]) }
        else { u16::from_be_bytes([input[end - 2], input[end - 1]]) };
    if (0xd800..=0xdbff).contains(&last) { end -= 2; }
    input[from..end].to_vec()
}
