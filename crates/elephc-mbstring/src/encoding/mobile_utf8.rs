//! Purpose:
//! Implements PHP's four carrier-specific UTF-8 variants using sparse Unicode overrides.
//!
//! Called from:
//! - `super::catalog::Encoding` for mobile UTF-8 decoding and encoding.
//!
//! Key details:
//! - Emoji expansions share their original byte offset and compose during encoding.
//! - Empty encoder overrides represent rejected standalone regional indicators.

use super::{mapping::{lookup, word}, Decoded, Substitute, UnicodeEncoding};

/// Sparse carrier mappings layered over PHP's ordinary UTF-8 codec.
#[derive(Clone, Copy, Debug)]
pub(super) struct MobileUtf8 {
    pub decode: &'static [u8],
    pub points: &'static [u8],
    pub encode: &'static [u8],
    pub bytes: &'static [u8],
    pub composites: &'static [u8],
}

impl MobileUtf8 {
    /// Decodes UTF-8 and expands private-use carrier emoji into their Unicode sequence.
    pub fn decode(self, input: &[u8]) -> Decoded {
        let raw = UnicodeEncoding::Utf8.decode(input);
        let mut output = Decoded::default();
        for (code, offset) in raw.points.into_iter().zip(raw.offsets) {
            if let Some(record) = lookup(self.decode, code) {
                let record = record as usize;
                for index in 0..word(self.points, record) as usize {
                    output.push(word(self.points, record + 4 + index * 4), offset);
                }
            } else {
                output.push(code, offset);
            }
        }
        output
    }

    /// Encodes carrier emoji compositions before applying scalar overrides and UTF-8 fallback.
    pub fn encode(self, input: &[u32], substitution: Substitute) -> Vec<u8> {
        let mut output = Vec::new();
        let mut offset = 0;
        while offset < input.len() {
            if let Some((count, bytes)) = self.composite(&input[offset..]) {
                output.extend_from_slice(bytes);
                offset += count;
            } else {
                let code = input[offset];
                if !self.append(code, &mut output) {
                    substitution.append(code, &mut output, |code, output| self.append(code, output));
                }
                offset += 1;
            }
        }
        output
    }

    /// Appends one representable scalar, rejecting explicit empty mappings without output.
    fn append(self, code: u32, output: &mut Vec<u8>) -> bool {
        if let Some(record) = lookup(self.encode, code) {
            let record = record as usize;
            let length = word(self.bytes, record) as usize;
            output.extend_from_slice(&self.bytes[record + 4..record + 4 + length]);
            length != 0
        } else {
            super::unicode::append_utf8(code, output)
        }
    }

    /// Selects the longest captured carrier composition matching the remaining input.
    fn composite(self, input: &[u32]) -> Option<(usize, &'static [u8])> {
        let mut offset = 0;
        let mut found = None;
        while offset < self.composites.len() {
            let count = word(self.composites, offset) as usize;
            let end = offset + 4 + count * 4;
            let length = word(self.composites, end) as usize;
            if count <= input.len()
                && (0..count).all(|index| input[index] == word(self.composites, offset + 4 + index * 4))
                && found.is_none_or(|(previous, _)| count > previous)
            {
                found = Some((count, &self.composites[end + 4..end + 4 + length]));
            }
            offset = end + 4 + length;
        }
        found
    }
}
