//! Purpose:
//! Implements stateless legacy multibyte codecs from exact PHP conversion mappings.
//!
//! Called from:
//! - `super::catalog::Encoding` for Shift-JIS, Chinese, and Korean codec families.
//!
//! Key details:
//! - Pair overrides preserve PHP's consumption of malformed trailing bytes.
//! - Decoder outputs may expand into several codepoints, including mobile emoji.
//! - Reverse composition records handle sequences represented by one encoded character.

use super::{mapping::{lookup, word}, Decoded, Substitute, BAD_INPUT};

/// Compact mapping tables for a stateless codec with up to three-byte input units.
#[derive(Clone, Copy, Debug)]
pub(super) struct DoubleByte {
    pub single: &'static [u8],
    pub pairs: &'static [u8],
    pub triples: &'static [u8],
    pub points: &'static [u8],
    pub encode: &'static [u8],
    pub bytes: &'static [u8],
    pub composites: &'static [u8],
    pub escapes: &'static [u8],
}

impl DoubleByte {
    /// Decodes the longest registered byte unit, retaining PHP's malformed-unit consumption.
    pub fn decode(self, input: &[u8]) -> Decoded {
        self.decode_state(input, &mut 0)
    }

    /// Retains SoftBank's emoji escape mode between MIME words without affecting other tables.
    pub fn decode_state(self, input: &[u8], state: &mut u32) -> Decoded {
        self.decode_buffer_state(input, 64, state)
    }

    /// Preserves SoftBank output reservations, including empty calls that consume shift bytes.
    pub fn decode_buffer_state(self, input: &[u8], capacity: usize, state: &mut u32) -> Decoded {
        self.decode_mode(input, capacity, state, false).0
    }

    /// Returns one bounded decoder call without consuming later input or resetting shift state.
    pub fn decode_next(self, input: &[u8], capacity: usize, state: &mut u32) -> (Decoded, usize) {
        self.decode_mode(input, capacity, state, true)
    }

    /// Keeps the full-stream and bounded readers on the same atomic decoder steps.
    fn decode_mode(self, input: &[u8], capacity: usize, state: &mut u32, single: bool) -> (Decoded, usize) {
        let mut output = Decoded::default();
        let (mut offset, mut batch) = (0, 0);
        let mut emoji_mode = if self.escapes.is_empty() { 0 } else { *state };
        while offset < input.len() {
            if !self.escapes.is_empty() && output.points.len() - batch >= capacity - 1 {
                if single { break; }
                output.case_batches.push(output.points.len());
                batch = output.points.len();
            }
            if emoji_mode != 0 {
                let byte = input[offset];
                if let Some(mode) = b"EFGOPQ".iter().position(|&value| u32::from(value) == emoji_mode) {
                    let record = word(self.escapes, (mode * 256 + usize::from(byte)) * 4) as usize;
                    self.append_points(record, offset, &mut output);
                } else if byte != 0x0f { output.push(BAD_INPUT, offset); }
                if byte == 0x0f || output.points.last() == Some(&BAD_INPUT) {
                    emoji_mode = 0;
                }
                offset += 1;
                continue;
            }
            if offset + 2 < input.len() && !self.triples.is_empty() {
                let key = (u32::from(input[offset]) << 16)
                    | (u32::from(input[offset + 1]) << 8) | u32::from(input[offset + 2]);
                if let Some(record) = lookup(self.triples, key) {
                    self.append_points(record as usize, offset, &mut output);
                    offset += 3;
                    continue;
                }
            }
            if !self.escapes.is_empty() && input[offset] == 0x1b {
                let start = offset;
                offset += 1;
                if offset < input.len() {
                    let marker = input[offset];
                    offset += 1;
                    if marker == b'$' && offset < input.len() {
                        let mode = input[offset];
                        offset += 1;
                        if b"EFGOPQ".contains(&mode) {
                            emoji_mode = u32::from(mode);
                            continue;
                        }
                    }
                }
                output.push(BAD_INPUT, start);
                continue;
            }
            let pair = input.get(offset + 1).map(|&next| {
                (u32::from(input[offset]) << 8) | u32::from(next)
            });
            let paired = pair.and_then(|key| lookup(self.pairs, key));
            let record = paired.unwrap_or_else(|| word(self.single, usize::from(input[offset]) * 4));
            self.append_points(record as usize, offset, &mut output);
            offset += if paired.is_some() { 2 } else { 1 };
        }
        if !self.escapes.is_empty() {
            *state = emoji_mode;
            if !input.is_empty() { output.case_batches.push(output.points.len()); }
        }
        (output, offset)
    }

    /// Appends a decoded sequence, assigning every expansion the source character's offset.
    fn append_points(self, record: usize, offset: usize, output: &mut Decoded) {
        let count = word(self.points, record) as usize;
        for index in 0..count {
            output.push(word(self.points, record + 4 + index * 4), offset);
        }
    }

    /// Encodes Unicode sequences, preferring registered composite mappings when present.
    pub fn encode(self, input: &[u32], substitution: Substitute) -> Vec<u8> {
        self.encode_prefix(input, substitution, true)
    }

    /// Leaves an incomplete composite pending until more input or the final encoder call.
    pub fn encode_prefix(self, input: &[u32], substitution: Substitute, finish: bool) -> Vec<u8> {
        let mut output = Vec::new();
        let mut offset = 0;
        while offset < input.len() {
            if !finish && self.incomplete_composite(&input[offset..]) { break; }
            if let Some((count, bytes)) = self.composite(&input[offset..]) {
                output.extend_from_slice(bytes);
                offset += count;
                continue;
            }
            let code = input[offset];
            if !self.append(code, &mut output) {
                substitution.append(code, &mut output, |point, bytes| self.append(point, bytes));
            }
            offset += 1;
        }
        output
    }

    /// Detects a proper prefix of a registered composition without flushing its first scalar.
    fn incomplete_composite(self, input: &[u32]) -> bool {
        let mut offset = 0;
        while offset < self.composites.len() {
            let count = word(self.composites, offset) as usize;
            if input.len() < count && input.iter().enumerate().all(|(index, &point)|
                point == word(self.composites, offset + 4 + index * 4))
            { return true; }
            let end = offset + 4 + count * 4;
            offset = end + 4 + word(self.composites, end) as usize;
        }
        false
    }

    /// Appends one canonical encoded character without changing output on a missing mapping.
    fn append(self, code: u32, output: &mut Vec<u8>) -> bool {
        let Some(bytes) = self.encoded(code) else { return false; };
        output.extend_from_slice(bytes);
        true
    }

    /// Borrows the canonical scalar mapping for stateful encodings sharing this character set.
    pub(super) fn encoded(self, code: u32) -> Option<&'static [u8]> {
        let Some(offset) = lookup(self.encode, code) else {
            return None;
        };
        let offset = offset as usize;
        let len = word(self.bytes, offset) as usize;
        Some(&self.bytes[offset + 4..offset + 4 + len])
    }

    /// Reads a one-codepoint pair mapping for a stateful wrapper's validated character bytes.
    pub(super) fn decoded_pair(self, first: u8, second: u8) -> u32 {
        let key = (u32::from(first) << 8) | u32::from(second);
        let Some(record) = lookup(self.pairs, key) else { return BAD_INPUT; };
        let record = record as usize;
        if word(self.points, record) != 1 { return BAD_INPUT; }
        word(self.points, record + 4)
    }

    /// Reads a one-codepoint triple mapping for a wrapper sharing the JIS X 0212 plane.
    pub(super) fn decoded_triple(self, first: u8, second: u8, third: u8) -> u32 {
        let key = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        let Some(record) = lookup(self.triples, key) else { return BAD_INPUT; };
        let record = record as usize;
        if word(self.points, record) != 1 { return BAD_INPUT; }
        word(self.points, record + 4)
    }

    /// Finds the longest encoded composition matching the remaining decoded input.
    pub(super) fn composite(self, input: &[u32]) -> Option<(usize, &'static [u8])> {
        let mut offset = 0;
        let mut found = None;
        while offset < self.composites.len() {
            let count = word(self.composites, offset) as usize;
            let end = offset + 4 + count * 4;
            let len = word(self.composites, end) as usize;
            if count <= input.len()
                && (0..count).all(|index| input[index] == word(self.composites, offset + 4 + index * 4))
                && found.is_none_or(|(previous, _)| count > previous)
            {
                found = Some((count, &self.composites[end + 4..end + 4 + len]));
            }
            offset = end + 4 + len;
        }
        found
    }

    /// Identifies characters requiring one-codepoint lookahead in a shared streaming encoder.
    pub(super) fn composite_starts(self, code: u32) -> bool {
        let mut offset = 0;
        while offset < self.composites.len() {
            let count = word(self.composites, offset) as usize;
            if count > 1 && word(self.composites, offset + 4) == code { return true; }
            let end = offset + 4 + count * 4;
            offset = end + 4 + word(self.composites, end) as usize;
        }
        false
    }
}
