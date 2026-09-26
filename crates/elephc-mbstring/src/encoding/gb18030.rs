//! Purpose:
//! Implements PHP's GB18030 and GB18030-2022 mappings and variable-length byte grammar.
//!
//! Called from:
//! - The shared mbstring encoding catalog and mb_strcut.
//!
//! Key details:
//! - BMP decoder and encoder maps remain independent across standard revisions.
//! - Supplementary characters use the standard linear four-byte address space.
//! - Byte cutting uses PHP's raw grammar, including invalid sequences and partial tails.

use super::{mapping::word, Decoded, Substitute, BAD_INPUT};

/// Revision-specific BMP mappings combined with the common GB18030 byte grammar.
#[derive(Clone, Copy, Debug)]
pub(super) struct Gb18030 {
    pub pairs: &'static [u8],
    pub bmp: &'static [u8],
    pub encode: &'static [u8],
}

impl Gb18030 {
    /// Decodes complete or malformed byte units with PHP's exact continuation consumption.
    pub fn decode(self, input: &[u8]) -> Decoded {
        let mut output = Decoded::default();
        let mut offset = 0;
        while offset < input.len() {
            let start = offset;
            let lead = input[offset];
            offset += 1;
            let code = if lead < 128 {
                u32::from(lead)
            } else if matches!(lead, 0x80 | 0xff) || offset == input.len() {
                BAD_INPUT
            } else {
                let second = input[offset];
                offset += 1;
                if ((0x81..=0x84).contains(&lead) || (0x90..=0xe3).contains(&lead))
                    && (0x30..=0x39).contains(&second)
                {
                    if let Some(&third) = input.get(offset) {
                        offset += 1;
                        if (0x81..=0xfe).contains(&third) && offset < input.len() {
                            let fourth = input[offset];
                            offset += 1;
                            if (0x30..=0x39).contains(&fourth) {
                                let pointer = (((u32::from(lead - 0x81) * 10 + u32::from(second - 0x30))
                                    * 126 + u32::from(third - 0x81)) * 10) + u32::from(fourth - 0x30);
                                if pointer <= 39419 {
                                    word(self.bmp, pointer as usize * 4)
                                } else if (189000..=1237575).contains(&pointer) {
                                    pointer - 189000 + 0x10000
                                } else { BAD_INPUT }
                            } else { BAD_INPUT }
                        } else { BAD_INPUT }
                    } else { BAD_INPUT }
                } else {
                    word(self.pairs, (usize::from(lead - 0x81) * 256 + usize::from(second)) * 4)
                }
            };
            output.push(code, start);
        }
        output
    }

    /// Encodes codepoints using the revision's canonical reverse mappings and substitution.
    pub fn encode(self, input: &[u32], substitute: Substitute) -> Vec<u8> {
        let mut output = Vec::new();
        for &code in input {
            if !self.append(code, &mut output) {
                substitute.append(code, &mut output, |code, output| self.append(code, output));
            }
        }
        output
    }

    /// Emits one representable codepoint without changing output on a rejected value.
    fn append(self, code: u32, output: &mut Vec<u8>) -> bool {
        if code < 0x10000 {
            let mapped = word(self.encode, code as usize * 4);
            if mapped == BAD_INPUT { return false; }
            let bytes = mapped.to_be_bytes();
            let width = if mapped < 0x100 { 1 } else if mapped < 0x10000 { 2 } else { 4 };
            output.extend_from_slice(&bytes[4 - width..]);
        } else if code <= 0x10ffff {
            let mut pointer = code - 0x10000 + 189000;
            let fourth = (pointer % 10) as u8 + 0x30;
            pointer /= 10;
            let third = (pointer % 126) as u8 + 0x81;
            pointer /= 126;
            output.extend_from_slice(&[(pointer / 10) as u8 + 0x81, (pointer % 10) as u8 + 0x30, third, fourth]);
        } else { return false; }
        true
    }

    /// Cuts raw byte boundaries after clamping the requested budget before start alignment.
    pub fn cut(self, input: &[u8], from: usize, length: usize) -> Vec<u8> {
        let length = length.min(input.len() - from);
        let start = boundary(input, 0, from);
        let end = if start + length >= input.len() { input.len() }
            else { boundary(input, start, start + length) };
        input[start..end].to_vec()
    }
}

/// Walks one-, two-, and four-byte raw units without validating their encoded character.
fn boundary(input: &[u8], mut offset: usize, limit: usize) -> usize {
    while offset < limit {
        let lead = input[offset];
        let width = if lead < 0x81 || lead == 0xff { 1 }
            else if limit - offset < 2 { break; }
            else if (0x30..=0x39).contains(&input[offset + 1]) { 4 } else { 2 };
        if limit - offset < width { break; }
        offset += width;
    }
    offset
}
