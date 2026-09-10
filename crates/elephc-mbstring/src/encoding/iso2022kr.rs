//! Purpose:
//! Implements ISO-2022-KR shifts, designation escapes, and PHP's canonical encoder behavior.
//!
//! Called from:
//! - The mbstring encoding catalog and the legacy mb_strcut adapter.
//!
//! Key details:
//! - Character mappings reuse UHC with the ISO-2022-KR subset and raw-code fallback.
//! - Conversion and legacy cutting consume malformed escape sequences differently.
//! - Every nonempty encoder input emits the designation, even when substitution removes it.

use super::{doublebyte::DoubleByte, Decoded, Substitute, SubstituteMode, BAD_INPUT};

/// ISO-2022-KR control syntax around the shared UHC mapping tables.
#[derive(Clone, Copy, Debug)]
pub(super) struct Iso2022Kr(pub DoubleByte);

impl Iso2022Kr {
    /// Decodes designation escapes and SI/SO state with PHP's modern malformed-input rules.
    pub fn decode(self, input: &[u8]) -> Decoded {
        self.decode_state(input, &mut 0)
    }

    /// Retains the numeric character-set state while completing each MIME word separately.
    pub fn decode_state(self, input: &[u8], state: &mut u32) -> Decoded {
        self.decode_next(input, usize::MAX, state).0
    }

    /// Stops at a complete character or escape boundary and preserves the selected plane.
    pub fn decode_next(self, input: &[u8], capacity: usize, state: &mut u32) -> (Decoded, usize) {
        let mut output = Decoded::default();
        let (mut offset, mut mode) = (0, *state);
        while offset < input.len() && output.points.len() < capacity {
            let start = offset;
            let byte = input[offset];
            offset += 1;
            match byte {
                0x1b => {
                    if input.len() - offset < 3 {
                        if let Some(&next) = input.get(offset) {
                            offset += 1;
                            if next == b'$' && offset < input.len() { offset += 1; }
                        }
                        output.push(BAD_INPUT, start);
                    } else {
                        let (second, third, fourth) = (input[offset], input[offset + 1], input[offset + 2]);
                        offset += 3;
                        if second == b'$' && third == b')' && fourth == b'C' { mode = 0; }
                        else {
                            if third != b')' {
                                offset -= 1;
                                if second != b'$' { offset -= 1; }
                            }
                            output.push(BAD_INPUT, start);
                        }
                    }
                }
                0x0f => mode = 0,
                0x0e => mode = 1,
                first if mode == 1 && (0x21..=0x7e).contains(&first) => {
                    let code = if let Some(&second) = input.get(offset) {
                        offset += 1;
                        self.pair(first, second)
                    } else { BAD_INPUT };
                    output.push(code, start);
                }
                byte => output.push(if mode == 0 && byte < 128 { u32::from(byte) } else { BAD_INPUT }, start),
            }
        }
        *state = mode;
        (output, offset)
    }

    /// Encodes all codepoints with one initial designation and a final return to ASCII.
    pub fn encode(self, input: &[u32], substitute: Substitute) -> Vec<u8> {
        self.encode_prefix(input, substitute, true)
    }

    /// Encodes a candidate MIME line while optionally retaining unflushed output state.
    pub fn encode_prefix(self, input: &[u32], substitute: Substitute, finish: bool) -> Vec<u8> {
        let mut encoder = Encoder { codec: self, designated: false, korean: false };
        let mut output = Vec::new();
        for &code in input { encoder.append(code, substitute, &mut output); }
        if finish { encoder.close(&mut output); }
        output
    }

    /// Encodes a single character, distinguishing an unmappable input from its designation bytes.
    pub fn encode_scalar(self, code: u32) -> Option<Vec<u8>> {
        let mut encoder = Encoder { codec: self, designated: true, korean: false };
        let mut output = b"\x1b$)C".to_vec();
        if !encoder.push(code, &mut output) { return None; }
        encoder.close(&mut output);
        Some(output)
    }

    /// Runs legacy streaming cuts with default replacement and provisional-flush budgets.
    pub fn cut(self, input: &[u8], from: usize, length: usize) -> Vec<u8> {
        let budget = length.min(input.len() - from);
        let mut decoder = Decoder::default();
        for &byte in &input[..from] { decoder.push(byte, self); }
        let mut encoder = Encoder { codec: self, designated: false, korean: false };
        let mut output = Vec::new();
        for &byte in &input[from..] {
            let (previous, previous_length, previous_decoder) = (encoder, output.len(), decoder);
            if let Some(code) = decoder.push(byte, self) { encoder.append(code, Substitute::default(), &mut output); }
            let mut finished = encoder;
            let mut tail = Vec::new();
            if decoder.state != 0 { finished.append(BAD_INPUT, Substitute::default(), &mut tail); }
            finished.close(&mut tail);
            if output.len() + tail.len() > budget {
                output.truncate(previous_length);
                encoder = previous;
                decoder = previous_decoder;
                break;
            }
        }
        if decoder.state != 0 {
            // PHP's final flush suppresses replacement bytes but can still emit the designation.
            encoder.append(BAD_INPUT, Substitute { mode: SubstituteMode::None, ..Substitute::default() }, &mut output);
        }
        encoder.close(&mut output);
        output
    }

    /// Applies the ISO-2022-KR decoder's row restrictions before consulting shared UHC mappings.
    fn pair(self, first: u8, second: u8) -> u32 {
        if !(0x21..=0x7e).contains(&second) || first == 0x49 || first > 0x7d
            || (first == 0x22 && second > 0x65)
        { BAD_INPUT } else { self.0.decoded_pair(first | 0x80, second | 0x80) }
    }
}

/// Tracks partial byte pairs and each prefix of the legacy designation escape.
#[derive(Clone, Copy, Default)]
struct Decoder {
    korean: bool,
    state: u8,
    first: u8,
}

impl Decoder {
    /// Consumes one byte according to the legacy filter's earlier malformed-escape rejection.
    fn push(&mut self, byte: u8, codec: Iso2022Kr) -> Option<u32> {
        match self.state {
            1 => { self.state = 0; Some(codec.pair(self.first, byte)) }
            2 => {
                if byte == b'$' { self.state = 3; None }
                else { self.state = 0; Some(BAD_INPUT) }
            }
            3 => {
                if byte == b')' { self.state = 4; None }
                else { self.state = 0; Some(BAD_INPUT) }
            }
            4 => {
                self.state = 0;
                self.korean = false;
                if byte == b'C' { None } else { Some(BAD_INPUT) }
            }
            _ => match byte {
                0x1b => { self.state = 2; None }
                0x0f => { self.korean = false; None }
                0x0e => { self.korean = true; None }
                byte if self.korean && (0x21..=0x7e).contains(&byte) => {
                    self.state = 1;
                    self.first = byte;
                    None
                }
                byte => Some(if !self.korean && byte < 128 { u32::from(byte) } else { BAD_INPUT }),
            },
        }
    }
}

/// Retains designation and SI/SO state across scalar output and replacement text.
#[derive(Clone, Copy)]
struct Encoder {
    codec: Iso2022Kr,
    designated: bool,
    korean: bool,
}

impl Encoder {
    /// Emits the designation before processing the first input, including an invalid codepoint.
    fn append(&mut self, code: u32, substitute: Substitute, output: &mut Vec<u8>) {
        if !self.designated {
            output.extend_from_slice(b"\x1b$)C");
            self.designated = true;
        }
        if !self.push(code, output) {
            substitute.append(code, output, |code, output| self.push(code, output));
        }
    }

    /// Encodes a mapped character or PHP's accepted raw-code fallback without implicit validation.
    fn push(&mut self, code: u32, output: &mut Vec<u8>) -> bool {
        let mapped = match self.codec.0.encoded(code) {
            Some([first, second]) if *first >= 0xa1 && *second >= 0xa1 =>
                (u32::from(*first - 0x80) << 8) | u32::from(*second - 0x80),
            _ => code,
        };
        if (0x80..0x2121).contains(&mapped) || mapped > 0x8080 { return false; }
        if mapped < 128 {
            self.close(output);
            output.push(mapped as u8);
        } else {
            if !self.korean { output.push(0x0e); self.korean = true; }
            output.extend_from_slice(&[(mapped >> 8) as u8, mapped as u8]);
        }
        true
    }

    /// Emits SI only when a Korean output run remains open.
    fn close(&mut self, output: &mut Vec<u8>) {
        if self.korean { output.push(0x0f); self.korean = false; }
    }
}
