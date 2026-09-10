//! Purpose:
//! Implements character substrings and splitting with PHP's encoding-specific fast paths.
//!
//! Called from:
//! - Direct mbstring substring/split operations and trimming, padding, and search helpers.
//!
//! Key details:
//! - Fixed-width substrings preserve raw bytes, including BOMs and incomplete units.
//! - Negative arguments use PHP's optimized length, which can differ from decoded length.

use crate::encoding::{Encoding, Slicing, Substitute};
use crate::error::{MbError, MbResult};

/// Selects a character substring using PHP's signed start/length normalization.
pub fn substr(
    input: &[u8], start: i64, length: Option<i64>, encoding: Encoding, substitution: Substitute,
) -> MbResult<Vec<u8>> {
    validate_substr_bounds(start, length)?;
    let total = if start < 0 || length.is_some_and(|length| length < 0) {
        encoding.strlen(input)
    } else {
        0
    };
    let start = if start < 0 { total.saturating_sub(start.unsigned_abs() as usize) } else { start as usize };
    let length = match length {
        None => usize::MAX,
        Some(length) if length >= 0 => length as usize,
        Some(length) => total.saturating_sub(start).saturating_sub(length.unsigned_abs() as usize),
    };
    slice(input, start, length, encoding, substitution)
}

/// Rejects PHP's unrepresentable signed substring bounds before encoding resolution.
pub fn validate_substr_bounds(start: i64, length: Option<i64>) -> MbResult<()> {
    for (number, name, value) in [(2, "start", Some(start)), (3, "length", length)] {
        if value == Some(i64::MIN) {
            return Err(MbError::argument("mb_substr", number, name,
                "must be between -9223372036854775807 and 9223372036854775807"));
        }
    }
    Ok(())
}

/// Selects already-normalized character positions for all text-operation consumers.
pub(super) fn slice(
    input: &[u8], start: usize, length: usize, encoding: Encoding, substitution: Substitute,
) -> MbResult<Vec<u8>> {
    if length == 0 {
        return Ok(Vec::new());
    }
    if let Slicing::Fixed(width) = encoding.slicing() {
        let from = start.saturating_mul(width).min(input.len());
        let end = from.saturating_add(length.saturating_mul(width)).min(input.len());
        return Ok(input[from..end].to_vec());
    }
    let decoded = encoding.decode(input);
    let from = start.min(decoded.points.len());
    let end = from.saturating_add(length).min(decoded.points.len());
    Ok(encoding.encode(&decoded.points[from..end], substitution))
}

/// Splits an encoded string, preserving PHP's raw-byte chunking where available.
pub fn str_split(input: &[u8], length: i64, encoding: Encoding, substitution: Substitute) -> MbResult<Vec<Vec<u8>>> {
    validate_split_length(length)?;
    let length = length as usize;
    match encoding.slicing() {
        Slicing::Fixed(width) => Ok(input.chunks(width * length).map(<[u8]>::to_vec).collect()),
        Slicing::LeadingByte(widths) => {
            let mut output = Vec::new();
            let mut offset = 0;
            while offset < input.len() {
                let start = offset;
                for _ in 0..length {
                    if offset >= input.len() { break; }
                    offset += usize::from(widths[usize::from(input[offset])]);
                }
                offset = offset.min(input.len());
                output.push(input[start..offset].to_vec());
            }
            Ok(output)
        }
        Slicing::Converted => {
            let decoded = encoding.decode(input);
            decoded.points.chunks(length).map(|part| Ok(encoding.encode(part, substitution))).collect()
        }
    }
}

/// Rejects split lengths before encoding resolution with PHP's exact integer limits.
pub fn validate_split_length(length: i64) -> MbResult<()> {
    if length <= 0 {
        return Err(MbError::argument("mb_str_split", 2, "length", "must be greater than 0"));
    }
    if length > (u32::MAX / 4) as i64 {
        return Err(MbError::argument("mb_str_split", 2, "length", "is too large"));
    }
    Ok(())
}
