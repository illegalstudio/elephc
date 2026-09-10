//! Purpose:
//! Implements PHP's legacy one-byte codecs from compact exact mapping tables.
//!
//! Called from:
//! - `super::catalog::Encoding` for non-Unicode single-byte encodings.
//!
//! Key details:
//! - Decoder holes retain BAD_INPUT; reverse maps preserve PHP's canonical choices.
//! - Encoder tables cover every Unicode scalar, including compatibility mappings.

use super::{Decoded, Substitute};

/// Exact decoder and encoder tables for one legacy one-byte encoding.
#[derive(Clone, Copy, Debug)]
pub(super) struct SingleByte {
    /// 256 little-endian decoded codepoints, indexed by source byte.
    pub decode: &'static [u8],
    /// Sorted little-endian pairs of codepoint and canonical output byte.
    pub encode: &'static [u8],
}

impl SingleByte {
    /// Decodes bytes with one output unit and one source offset per byte.
    pub fn decode(self, input: &[u8]) -> Decoded {
        Decoded {
            points: input.iter().map(|&byte| word(self.decode, usize::from(byte) * 4)).collect(),
            offsets: (0..input.len()).collect(),
            ..Decoded::default()
        }
    }

    /// Encodes codepoints with the caller's substitution settings for missing mappings.
    pub fn encode(self, points: &[u32], substitution: Substitute) -> Vec<u8> {
        let mut output = Vec::with_capacity(points.len());
        for &code in points {
            if !self.append(code, &mut output) {
                substitution.append(code, &mut output, |point, output| self.append(point, output));
            }
        }
        output
    }

    /// Appends a mapped byte or leaves output untouched if the codepoint is absent.
    fn append(self, code: u32, output: &mut Vec<u8>) -> bool {
        let (mut low, mut high) = (0, self.encode.len() / 8);
        while low < high {
            let mid = low + (high - low) / 2;
            match word(self.encode, mid * 8).cmp(&code) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    output.push(word(self.encode, mid * 8 + 4) as u8);
                    return true;
                }
            }
        }
        false
    }
}

/// Reads a generated little-endian integer without assuming byte-slice alignment.
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("generated codec word"))
}
