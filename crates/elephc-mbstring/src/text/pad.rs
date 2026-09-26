//! Purpose:
//! Implements multibyte padding with PHP's byte-preserving full-pad copies.
//!
//! Called from:
//! - The mb_str_pad backend adapter.
//!
//! Key details:
//! - Invalid padding arguments are ignored when no padding is needed, as in PHP.
//! - Only a partial pad copy uses character slicing; complete copies preserve raw bytes.

use crate::encoding::{Encoding, Substitute};
use crate::error::{MbError, MbResult};

/// Pads to a character length using left/right/both placement (PHP constants 0/1/2).
pub fn str_pad(input: &[u8], length: i64, pad: &[u8], side: i64, encoding: Encoding, substitution: Substitute) -> MbResult<Vec<u8>> {
    let input_len = encoding.strlen(input);
    if length < 0 || length as usize <= input_len { return Ok(input.to_vec()); }
    if pad.is_empty() { return Err(MbError::empty("mb_str_pad", 3, "pad_string")); }
    if !(0..=2).contains(&side) {
        return Err(MbError::argument("mb_str_pad", 4, "pad_type", "must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH"));
    }
    let pad_len = encoding.strlen(pad);
    if pad_len == 0 { return Err(MbError::empty("mb_str_pad", 3, "pad_string")); }
    let needed = length as usize - input_len;
    let left = match side { 0 => needed, 2 => needed / 2, _ => 0 };
    let right = needed - left;
    let left_tail = super::slicing::slice(pad, 0, left % pad_len, encoding, substitution)?;
    let right_tail = super::slicing::slice(pad, 0, right % pad_len, encoding, substitution)?;
    let full_left = left / pad_len;
    let full_right = right / pad_len;
    let bytes = full_left.checked_add(full_right).and_then(|copies| copies.checked_mul(pad.len()))
        .and_then(|bytes| bytes.checked_add(left_tail.len()))
        .and_then(|bytes| bytes.checked_add(right_tail.len()))
        .and_then(|bytes| bytes.checked_add(input.len()))
        .filter(|&bytes| bytes <= isize::MAX as usize - 32)
        .ok_or_else(|| MbError::Runtime("String size overflow".into()))?;
    let mut output = Vec::new();
    output.try_reserve_exact(bytes).map_err(|_| MbError::Runtime("String size overflow".into()))?;
    for _ in 0..full_left { output.extend_from_slice(pad); }
    output.extend_from_slice(&left_tail);
    output.extend_from_slice(input);
    for _ in 0..full_right { output.extend_from_slice(pad); }
    output.extend_from_slice(&right_tail);
    Ok(output)
}
