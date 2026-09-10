//! Purpose:
//! Implements character ordinals, single-character construction, and first-character casing.
//!
//! Called from:
//! - mb_ord, mb_chr, mb_ucfirst, and mb_lcfirst backend adapters.
//!
//! Key details:
//! - mb_ucfirst uses title case, including one-to-many mappings.
//! - PHP deliberately rejects several stateful and transfer encodings for ord/chr.

use crate::encoding::{Encoding, Substitute, BAD_INPUT};
use crate::error::{MbError, MbResult};
use crate::unicode::CaseMode;

/// Returns the first decoded codepoint, or false for an invalid/empty decoded sequence.
pub fn ord(input: &[u8], encoding: Encoding) -> MbResult<Option<u32>> {
    if input.is_empty() { return Err(MbError::empty("mb_ord", 1, "string")); }
    check_supported("mb_ord", encoding)?;
    Ok(encoding.decode(input).points.first().copied().filter(|&point| point != BAD_INPUT))
}

/// Constructs an encoded codepoint, returning false when it cannot be represented.
pub fn chr(code: i64, encoding: Encoding) -> MbResult<Option<Vec<u8>>> {
    check_supported("mb_chr", encoding)?;
    if !(0..=0x10ffff).contains(&code) {
        return Ok(None);
    }
    if encoding.name().starts_with("UTF-8") && (0xd800..=0xdfff).contains(&code) {
        return Ok(None);
    }
    Ok(encoding.encode_scalar(code as u32))
}

/// Changes the first character and preserves the original bytes when casing has no effect.
pub fn first_case(input: &[u8], upper: bool, encoding: Encoding, substitution: Substitute) -> MbResult<Vec<u8>> {
    let first = super::slicing::slice(input, 0, 1, encoding, substitution)?;
    let mode = if upper { CaseMode::Title } else { CaseMode::Lower };
    let mut changed = super::convert_case(&first, mode, encoding, substitution);
    if changed == first { return Ok(input.to_vec()); }
    changed.extend_from_slice(&super::slicing::slice(input, 1, usize::MAX, encoding, substitution)?);
    Ok(changed)
}

/// Rejects encodings excluded by PHP's ord/chr contract before decoding or codepoint checks.
fn check_supported(function: &str, encoding: Encoding) -> MbResult<()> {
    if !encoding.supports_ord_chr() {
        return Err(MbError::Value(format!("{function}() does not support the \"{}\" encoding", encoding.name())));
    }
    Ok(())
}
