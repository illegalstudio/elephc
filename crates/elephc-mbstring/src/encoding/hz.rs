//! Purpose:
//! Implements HZ's ASCII/GB2312 shifts, line continuations, and streaming byte cuts.
//!
//! Called from:
//! - The canonical mbstring encoding catalog.
//!
//! Key details:
//! - HZ shares EUC-CN mappings with one decoder override and one encoder exclusion.
//! - A dangling tilde is ignored, while an incomplete Chinese pair is invalid.
//! - Cuts use fixed default replacement and retain only states fitting a provisional flush.

use super::{doublebyte::DoubleByte, Decoded, Substitute, BAD_INPUT};

/// HZ control syntax around the shared EUC-CN character mapping tables.
#[derive(Clone, Copy, Debug)]
pub(super) struct Hz(pub DoubleByte);

impl Hz {
    /// Decodes bytes while keeping escape and pair starts for subsequent string operations.
    pub fn decode(self, input: &[u8]) -> Decoded {
        self.decode_state(input, &mut 0)
    }

    /// Retains PHP's raw shift state across independently terminated MIME words.
    pub fn decode_state(self, input: &[u8], state: &mut u32) -> Decoded {
        self.decode_next(input, usize::MAX, state).0
    }

    /// Stops after complete input units, preserving the shift mode for the next buffer.
    pub fn decode_next(self, input: &[u8], capacity: usize, state: &mut u32) -> (Decoded, usize) {
        let mut decoder = Decoder::default();
        decoder.mode = *state;
        let mut output = Decoded::default();
        let mut start = 0;
        let mut consumed = 0;
        for (offset, &byte) in input.iter().enumerate() {
            if output.points.len() == capacity { break; }
            consumed = offset + 1;
            if !decoder.escaped && decoder.pending.is_none() { start = offset; }
            if let Some(code) = decoder.push(byte, self) { output.push(code, start); }
        }
        if decoder.pending.is_some() { output.push(BAD_INPUT, start); }
        *state = decoder.mode;
        (output, consumed)
    }

    /// Encodes maximal GB2312 runs and closes the final shift after applying substitutions.
    pub fn encode(self, input: &[u32], substitute: Substitute) -> Vec<u8> {
        self.encode_prefix(input, substitute, true)
    }

    /// Encodes a candidate MIME line while optionally retaining unflushed output state.
    pub fn encode_prefix(self, input: &[u32], substitute: Substitute, finish: bool) -> Vec<u8> {
        let mut encoder = Encoder { codec: self, chinese: false };
        let mut output = Vec::new();
        for &code in input { encoder.append(code, substitute, &mut output); }
        if finish { encoder.close(&mut output); }
        output
    }

    /// Streams past discarded prefix bytes and enforces the cut budget after each flush.
    pub fn cut(self, input: &[u8], from: usize, length: usize) -> Vec<u8> {
        let budget = length.min(input.len() - from);
        let mut decoder = Decoder::default();
        for &byte in &input[..from] { decoder.push(byte, self); }
        let mut encoder = Encoder { codec: self, chinese: false };
        let mut output = Vec::new();
        for &byte in &input[from..] {
            let (previous, previous_length) = (encoder, output.len());
            if let Some(code) = decoder.push(byte, self) { encoder.append(code, Substitute::default(), &mut output); }
            let mut finished = encoder;
            let mut tail = Vec::new();
            if decoder.pending.is_some() { finished.append(BAD_INPUT, Substitute::default(), &mut tail); }
            finished.close(&mut tail);
            if output.len() + tail.len() > budget {
                output.truncate(previous_length);
                encoder = previous;
                break;
            }
        }
        encoder.close(&mut output);
        output
    }
}

/// Tracks the selected character set and an incomplete tilde or Chinese byte pair.
#[derive(Default)]
struct Decoder {
    mode: u32,
    escaped: bool,
    pending: Option<u8>,
}

impl Decoder {
    /// Consumes one input byte, emitting at most one codepoint or invalid-unit sentinel.
    fn push(&mut self, byte: u8, codec: Hz) -> Option<u32> {
        if let Some(first) = self.pending.take() {
            return Some(if !(0x21..=0x7e).contains(&byte) { BAD_INPUT }
                else if first == 0x21 && byte == 0x2c { 0x2225 }
                else { codec.0.decoded_pair(first | 0x80, byte | 0x80) });
        }
        if self.escaped {
            self.escaped = false;
            match byte {
                b'}' if self.mode == 1 => self.mode = 0,
                b'{' if self.mode == 0 => self.mode = 1,
                b'~' if self.mode == 0 => return Some(u32::from(b'~')),
                b'\n' => {},
                _ => return Some(BAD_INPUT),
            }
        } else if byte == b'~' {
            self.escaped = true;
        } else if self.mode == 1 && ((0x21..=0x29).contains(&byte) || (0x30..=0x77).contains(&byte)) {
            self.pending = Some(byte);
        } else {
            return Some(if self.mode == 0 && byte < 128 { u32::from(byte) } else { BAD_INPUT });
        }
        None
    }
}

/// Retains the output character-set shift between representable units and substitutions.
#[derive(Clone, Copy)]
struct Encoder {
    codec: Hz,
    chinese: bool,
}

impl Encoder {
    /// Appends a codepoint using the caller's replacement policy if no HZ mapping exists.
    fn append(&mut self, code: u32, substitute: Substitute, output: &mut Vec<u8>) {
        if !self.push(code, output) {
            substitute.append(code, output, |code, output| self.push(code, output));
        }
    }

    /// Emits one mapped scalar, preserving the state and bytes if the character is rejected.
    fn push(&mut self, code: u32, output: &mut Vec<u8>) -> bool {
        if code == 0x2016 { return false; }
        let Some(bytes) = self.codec.0.encoded(code) else { return false; };
        if bytes.len() == 1 && bytes[0] < 128 {
            self.close(output);
            output.push(bytes[0]);
            if bytes[0] == b'~' { output.push(b'~'); }
        } else if bytes.len() == 2 {
            if !self.chinese {
                output.extend_from_slice(b"~{");
                self.chinese = true;
            }
            output.extend_from_slice(&[bytes[0] & 0x7f, bytes[1] & 0x7f]);
        } else { return false; }
        true
    }

    /// Returns the output stream to ASCII when a GB2312 run remains open.
    fn close(&mut self, output: &mut Vec<u8>) {
        if self.chinese {
            output.extend_from_slice(b"~}");
            self.chinese = false;
        }
    }
}
