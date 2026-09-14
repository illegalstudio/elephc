//! Purpose:
//! Implements EUC-TW using the older CNS-11643 planes supported by PHP's mbstring baseline.
//!
//! Called from:
//! - The canonical mbstring encoding catalog.
//!
//! Key details:
//! - Only planes 1, 2, and historical plane 14 are accepted.
//! - Invalid plane and row bytes stop consumption at PHP's observable byte boundary.

use super::{mapping::word, Decoded, Substitute, BAD_INPUT};

/// Exact older-CNS decoder planes and independent canonical BMP reverse mappings.
#[derive(Clone, Copy, Debug)]
pub(super) struct EucTw {
    pub planes: &'static [u8],
    pub encode: &'static [u8],
}

impl EucTw {
    /// Decodes direct ASCII, compact plane-one units, and the three accepted extended planes.
    pub fn decode(self, input: &[u8]) -> Decoded {
        let mut output = Decoded::default();
        let mut offset = 0;
        while offset < input.len() {
            let start = offset;
            let lead = input[offset];
            offset += 1;
            let code = if lead < 128 {
                u32::from(lead)
            } else if valid_row(0, lead) && offset < input.len() {
                let cell = input[offset];
                offset += 1;
                self.character(0, lead, cell)
            } else if lead == 0x8e && offset < input.len() {
                let plane = input[offset];
                offset += 1;
                if let Some(plane) = [0xa1, 0xa2, 0xae].iter().position(|&value| value == plane) {
                    if let Some(&row) = input.get(offset) {
                        offset += 1;
                        if valid_row(plane, row) && offset < input.len() {
                            let cell = input[offset];
                            offset += 1;
                            self.character(plane, row, cell)
                        } else { BAD_INPUT }
                    } else { BAD_INPUT }
                } else { BAD_INPUT }
            } else { BAD_INPUT };
            output.push(code, start);
        }
        output
    }

    /// Encodes representable BMP values and applies the selected substitution to all others.
    pub fn encode(self, input: &[u32], substitute: Substitute) -> Vec<u8> {
        let mut output = Vec::new();
        for &code in input {
            if !self.append(code, &mut output) {
                substitute.append(code, &mut output, |code, output| self.append(code, output));
            }
        }
        output
    }

    /// Looks up one assigned row/cell or returns the invalid-unit sentinel.
    fn character(self, plane: usize, row: u8, cell: u8) -> u32 {
        if !(0xa1..=0xfe).contains(&cell) { return BAD_INPUT; }
        word(self.planes, (plane * 94 * 94 + usize::from(row - 0xa1) * 94 + usize::from(cell - 0xa1)) * 4)
    }

    /// Appends a canonical one-, two-, or four-byte mapping without output on failure.
    fn append(self, code: u32, output: &mut Vec<u8>) -> bool {
        if code > 0xffff { return false; }
        let mapped = word(self.encode, code as usize * 4);
        if mapped == BAD_INPUT { return false; }
        let bytes = mapped.to_be_bytes();
        let width = if mapped < 0x100 { 1 } else if mapped < 0x10000 { 2 } else { 4 };
        output.extend_from_slice(&bytes[4 - width..]);
        true
    }
}

/// Checks the row ranges supported by the specific historical CNS plane.
fn valid_row(plane: usize, row: u8) -> bool {
    match plane {
        0 => ((0xa1..=0xa6).contains(&row) || (0xc2..=0xfd).contains(&row)) && row != 0xc3,
        1 => (0xa1..=0xf2).contains(&row),
        2 => (0xa1..=0xe7).contains(&row),
        _ => false,
    }
}
