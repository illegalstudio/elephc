//! Purpose:
//! Shares Unicode kana transformations between text operations and Japanese encoders.
//!
//! Called from:
//! - The CP50220 encoder for fullwidth katakana with adjacent mark composition.
//! - `crate::text::convert_kana` for PHP's validated conversion modes.
//!
//! Key details:
//! - Tables capture PHP's KV behavior independently of standard Unicode normalization.
//! - A consumed second codepoint must not be emitted again by the caller.

use crate::error::{MbError, MbResult};

/// PHP's flag bit order, also defining which inverse combination is diagnosed first.
const FLAGS: &[u8; 17] = b"ARNSKHMCarnskhmcV";

/// Scalar transforms in PHP's first-match order, with uppercase kana handled contextually.
const TRANSFORMS: &[(u32, &[u8])] = &[
    (1 << 0, include_bytes!("data/kana-upper-a.bin")),
    (1 << 1, include_bytes!("data/kana-upper-r.bin")),
    (1 << 2, include_bytes!("data/kana-upper-n.bin")),
    (1 << 3, include_bytes!("data/kana-upper-s.bin")),
    (1 << 4, include_bytes!("data/kana-upper-k.bin")),
    (1 << 5, include_bytes!("data/kana-upper-h.bin")),
    (1 << 6, include_bytes!("data/kana-upper-m.bin")),
    (1 << 8, include_bytes!("data/kana-lower-a.bin")),
    (1 << 9, include_bytes!("data/kana-lower-r.bin")),
    (1 << 10, include_bytes!("data/kana-lower-n.bin")),
    (1 << 11, include_bytes!("data/kana-lower-s.bin")),
    (1 << 12, include_bytes!("data/kana-lower-k.bin")),
    (1 << 13, include_bytes!("data/kana-lower-h.bin")),
    (1 << 7, include_bytes!("data/kana-upper-c.bin")),
    (1 << 15, include_bytes!("data/kana-lower-c.bin")),
    (1 << 14, include_bytes!("data/kana-lower-m.bin")),
];

/// Validated kana options, preserving PHP's transform order and composition rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KanaMode(u32);

impl Default for KanaMode {
    /// Selects PHP's default KV conversion: halfwidth to fullwidth katakana with mark fusion.
    fn default() -> Self { Self((1 << 4) | (1 << 16)) }
}

impl KanaMode {
    /// Validates every flag byte and rejects inverse or competing transforms in PHP's order.
    pub fn parse(input: &[u8]) -> MbResult<Self> {
        let mut mode = 0;
        for &byte in input {
            let Some(index) = FLAGS.iter().position(|&flag| flag == byte) else {
                let mut message = b"mb_convert_kana(): Argument #2 ($mode) contains invalid flag: '".to_vec();
                message.push(byte);
                message.push(b'\'');
                if let Some(nul) = message.iter().position(|&byte| byte == 0) { message.truncate(nul); }
                return Err(match String::from_utf8(message) {
                    Ok(message) => MbError::Value(message),
                    Err(error) => MbError::ValueBytes(error.into_bytes()),
                });
            };
            mode |= 1 << index;
            if byte == b'A' { mode |= (1 << 1) | (1 << 2); }
            if byte == b'a' { mode |= (1 << 9) | (1 << 10); }
        }
        let inverse = ((mode >> 8) & mode) & 0xffu32;
        if inverse != 0 {
            let index = inverse.trailing_zeros() as usize;
            let mut first = FLAGS[index];
            let mut second = FLAGS[index + 8];
            if matches!(first, b'R' | b'N') && mode & 1 != 0 { first = b'A'; }
            if matches!(second, b'r' | b'n') && mode & (1 << 8) != 0 { second = b'a'; }
            return Err(Self::incompatible(first, second));
        }
        for (first, second) in [(b'H', b'K'), (b'h', b'C'), (b'h', b'c'), (b'k', b'C'), (b'k', b'c')] {
            let selected = |flag| mode & (1 << FLAGS.iter().position(|&byte| byte == flag).unwrap()) != 0;
            if selected(first) && selected(second) { return Err(Self::incompatible(first, second)); }
        }
        Ok(Self(mode))
    }

    /// Formats the exact conflicting-option diagnostic for a validated pair of ASCII flags.
    fn incompatible(first: u8, second: u8) -> MbError {
        MbError::argument("mb_convert_kana", 2, "mode",
            &format!("must not combine '{}' and '{}' flags", char::from(first), char::from(second)))
    }

    /// Transforms a codepoint stream, retaining invalid markers and adjacent composition state.
    pub fn convert(self, input: &[u32]) -> Vec<u32> {
        let mut output = Vec::with_capacity(input.len());
        let mut offset = 0;
        while let Some(&code) = input.get(offset) {
            let (points, consumed) = self.point(code, input.get(offset + 1).copied().unwrap_or(0));
            output.push(points[0]);
            if points[1] != 0 { output.push(points[1]); }
            offset += 1 + usize::from(consumed);
        }
        output
    }

    /// Applies the first matching scalar rule, checking a neighboring kana mark when enabled.
    pub(crate) fn point(self, code: u32, next: u32) -> ([u32; 2], bool) {
        for &(flag, table) in TRANSFORMS {
            if self.0 & flag == 0 { continue; }
            if self.0 & (1 << 16) != 0 && matches!(flag, 16 | 32) && (0xff61..=0xff9f).contains(&code) {
                let (code, consumed) = fullwidth_kana(code, next, flag == 32);
                return ([code, 0], consumed);
            }
            if let Some(points) = lookup(table, code) { return (points, false); }
        }
        ([code, 0], false)
    }
}

/// Finds a sparse scalar transform in sorted little-endian source/output/output records.
fn lookup(table: &[u8], code: u32) -> Option<[u32; 2]> {
    let (mut low, mut high) = (0, table.len() / 12);
    while low < high {
        let mid = low + (high - low) / 2;
        let offset = mid * 12;
        match word(table, offset).cmp(&code) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
            std::cmp::Ordering::Equal => return Some([word(table, offset + 4), word(table, offset + 8)]),
        }
    }
    None
}

/// Reads a generated Unicode mapping word without native-endian or alignment assumptions.
fn word(table: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(table[offset..offset + 4].try_into().expect("generated kana word"))
}

/// Converts halfwidth kana and optionally merges a following voiced or semi-voiced mark.
pub(crate) fn fullwidth_katakana(code: u32, next: u32) -> (u32, bool) {
    fullwidth_kana(code, next, false)
}

/// Selects PHP's fullwidth hiragana or katakana mapping and optional adjacent composition.
fn fullwidth_kana(code: u32, next: u32, hiragana: bool) -> (u32, bool) {
    if !(0xff61..=0xff9f).contains(&code) { return (code, false); }
    let table = if hiragana { include_bytes!("data/kana-hv.bin") } else { include_bytes!("data/kana-kv.bin") };
    let offset = (code - 0xff61) as usize * 12;
    let base = word(table, offset);
    if matches!(next, 0xff9e | 0xff9f) {
        let index = offset + (next - 0xff9d) as usize * 4;
        let combined = word(table, index);
        if combined != u32::MAX { return (combined, true); }
    }
    (base, false)
}
