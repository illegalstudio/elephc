//! Purpose:
//! Case-maps decoded code points exactly the way php-src's `php_unicode_convert_case()`
//! does for `PHP_UNICODE_CASE_UPPER` and `PHP_UNICODE_CASE_LOWER`.
//!
//! Called from:
//! - `crate::case::convert_case()` once the encoding is resolved.
//!
//! Key details:
//! - Mappings are full (SpecialCasing) mappings: `ß` uppercases to `SS` and `İ` lowercases
//!   to `i` + U+0307. They come from Rust's `char::to_uppercase` / `to_lowercase`, whose
//!   per-code-point results equal mbstring's for every Unicode scalar under php 8.5 (both
//!   ship Unicode 17); `tests.rs` pins the version-sensitive cases.
//! - ISO-8859-9 applies mbstring's Turkish special case to the ASCII `i` / `I` pair.
//! - Capital sigma lowercases to final `ς` when a cased letter precedes it and none
//!   follows, skipping Case_Ignorable code points on both sides. Like php-src, the look
//!   back only sees the current chunk plus what is left of the previous chunk's converted
//!   output, and the look ahead checks Cased before Case_Ignorable once it leaves the
//!   chunk; both quirks are reproduced because they change results at chunk boundaries.
//! - Values above U+FFFFFF (error markers, and raw UCS-4 values) pass through untouched.

use super::codec::{Codec, Decoded};
use super::props::{is_case_ignorable, is_cased};
use super::CaseMode;
use crate::error::IconvError;

/// GREEK CAPITAL LETTER SIGMA.
const CAPITAL_SIGMA: u32 = 0x3A3;
/// GREEK SMALL LETTER FINAL SIGMA.
const FINAL_SIGMA: u32 = 0x3C2;

/// Case-maps `input`, decoding and re-encoding it with `codec`.
///
/// `turkish` selects ISO-8859-9's dotted/dotless `i` rules. Fails only when a libc
/// `iconv` fallback charset cannot be opened.
pub(super) fn convert(
    mode: CaseMode,
    codec: Codec,
    turkish: bool,
    input: &[u8],
) -> Result<Vec<u8>, IconvError> {
    if matches!(codec, Codec::Utf8) && input.is_ascii() {
        // Pure ASCII maps within ASCII and never meets the sigma rule.
        return Ok(match mode {
            CaseMode::Upper => input.to_ascii_uppercase(),
            CaseMode::Lower => input.to_ascii_lowercase(),
        });
    }
    let decoded = codec.decode(input)?;
    let mut encoder = codec.encoder()?;
    let mut out = Vec::with_capacity(input.len() + 1);
    let mut converted = Converted::default();
    let mut start = 0;
    for &end in &decoded.chunk_ends {
        converted.begin_chunk();
        let chunk = &decoded.wchars[start..end];
        for (index, &w) in chunk.iter().enumerate() {
            if w > 0xFF_FFFF {
                converted.push(w);
                continue;
            }
            match mode {
                CaseMode::Upper => push_upper(w, turkish, &mut converted),
                CaseMode::Lower => {
                    if w == CAPITAL_SIGMA && ends_word(chunk, index, &converted, &decoded, end) {
                        converted.push(FINAL_SIGMA);
                    } else {
                        push_lower(w, turkish, &mut converted);
                    }
                }
            }
        }
        encoder.encode(converted.current(), &mut out);
        converted.end_chunk();
        start = end;
    }
    encoder.finish(&mut out);
    Ok(out)
}

/// php-src's reusable `converted_buf`: each chunk overwrites it from the start, so the
/// tail of the previous chunk's output survives only past the current write position.
#[derive(Default)]
struct Converted {
    /// Backing storage shared by every chunk.
    slots: Vec<u32>,
    /// Write position within the current chunk (php-src's `p`).
    len: usize,
    /// Length of the previous chunk's output (php-src's `converted_end`), if any.
    previous_end: Option<usize>,
}

impl Converted {
    /// Starts overwriting the buffer for a new chunk.
    fn begin_chunk(&mut self) {
        self.len = 0;
    }

    /// Records the finished chunk's length as the next chunk's look-back limit.
    fn end_chunk(&mut self) {
        self.previous_end = Some(self.len);
    }

    /// Appends one converted code point.
    fn push(&mut self, w: u32) {
        if self.len < self.slots.len() {
            self.slots[self.len] = w;
        } else {
            self.slots.push(w);
        }
        self.len += 1;
    }

    /// Returns the current chunk's converted code points.
    fn current(&self) -> &[u32] {
        &self.slots[..self.len]
    }

    /// Ports `scan_back_for_cased_letter(p, converted_end)`.
    ///
    /// Only the previous chunk's output that the current chunk has not yet overwritten is
    /// visible, and it is the converted (not the original) code points that are tested.
    fn previous_chunk_has_cased_letter(&self) -> bool {
        let Some(end) = self.previous_end else {
            return false;
        };
        for &w in self.slots[self.len.min(end)..end].iter().rev() {
            if is_cased(w) {
                return true;
            }
            if !is_case_ignorable(w) {
                return false;
            }
        }
        false
    }
}

/// Decides whether the capital sigma at `chunk[index]` is word-final.
fn ends_word(
    chunk: &[u32],
    index: usize,
    converted: &Converted,
    decoded: &Decoded,
    chunk_end: usize,
) -> bool {
    let mut back = index;
    while back > 0 && is_case_ignorable(chunk[back - 1]) {
        back -= 1;
    }
    let preceded = if back > 0 {
        is_cased(chunk[back - 1])
    } else {
        converted.previous_chunk_has_cased_letter()
    };
    if !preceded {
        return false;
    }
    let mut ahead = index + 1;
    while ahead < chunk.len() && is_case_ignorable(chunk[ahead]) {
        ahead += 1;
    }
    let followed = if ahead < chunk.len() {
        is_cased(chunk[ahead])
    } else {
        rest_has_cased_letter(&decoded.wchars[chunk_end..])
    };
    !followed
}

/// Ports `scan_ahead_for_cased_letter()`, which tests Cased before Case_Ignorable.
fn rest_has_cased_letter(rest: &[u32]) -> bool {
    for &w in rest {
        if is_cased(w) {
            return true;
        }
        if !is_case_ignorable(w) {
            return false;
        }
    }
    false
}

/// Appends the full uppercase mapping of one code point.
fn push_upper(w: u32, turkish: bool, converted: &mut Converted) {
    if w < 0xB5 {
        // php-src's ASCII fast path: nothing below MICRO SIGN except a-z has an uppercase.
        let mapped = match w {
            0x69 if turkish => 0x130,
            0x61..=0x7A => w - 0x20,
            _ => w,
        };
        converted.push(mapped);
        return;
    }
    match char::from_u32(w) {
        Some(c) => c.to_uppercase().for_each(|u| converted.push(u32::from(u))),
        None => converted.push(w),
    }
}

/// Appends the full lowercase mapping of one code point (sigma context handled by the caller).
fn push_lower(w: u32, turkish: bool, converted: &mut Converted) {
    if w < 0xC0 {
        // php-src's ASCII fast path: nothing below U+00C0 except A-Z has a lowercase.
        let mapped = match w {
            0x49 if turkish => 0x131,
            0x41..=0x5A => w + 0x20,
            _ => w,
        };
        converted.push(mapped);
        return;
    }
    if turkish && w == 0x130 {
        converted.push(0x69);
        return;
    }
    match char::from_u32(w) {
        Some(c) => c.to_lowercase().for_each(|l| converted.push(u32::from(l))),
        None => converted.push(w),
    }
}
