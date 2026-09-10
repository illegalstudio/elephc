//! Purpose:
//! Implements ISO-2022-JP-2004 shifts over shared JIS X 0208 and JIS X 0213 mappings.
//!
//! Called from:
//! - The canonical mbstring encoding catalog.
//!
//! Key details:
//! - Composite characters remain atomic within PHP's reserved-slot decoder batches.
//! - The legacy cut encoder shares one state for both planes and drops some pending tails.

use super::{doublebyte::DoubleByte, Decoded, Substitute, BAD_INPUT};

/// Shared Japanese character maps with the 2004 stateful transport rules.
#[derive(Clone, Copy, Debug)]
pub(super) struct Jis2004 {
    pub classic: DoubleByte,
    pub extended: DoubleByte,
}

/// Active transport plane, distinct from an incomplete byte pair or escape prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode { Ascii, Classic, Plane1, Plane2, Unknown(u32) }

impl Jis2004 {
    /// Decodes stateful bytes, preserving source offsets and 63-word batch reservations.
    pub fn decode(self, input: &[u8]) -> Decoded {
        self.decode_buffer(input, 64)
    }

    /// Retains composite boundaries using the requested output scratch-buffer capacity.
    pub fn decode_buffer(self, input: &[u8], capacity: usize) -> Decoded {
        self.decode_state(input, capacity, &mut 0)
    }

    /// Keeps the PHP plane number across MIME words without carrying incomplete byte units.
    pub fn decode_state(self, input: &[u8], capacity: usize, state: &mut u32) -> Decoded {
        self.decode_mode(input, capacity, state, false).0
    }

    /// Returns one bounded decoder call without consuming later input or resetting shift state.
    pub fn decode_next(self, input: &[u8], capacity: usize, state: &mut u32) -> (Decoded, usize) {
        self.decode_mode(input, capacity, state, true)
    }

    /// Keeps the full-stream and bounded readers on the same atomic decoder steps.
    fn decode_mode(self, input: &[u8], capacity: usize, state: &mut u32, single: bool) -> (Decoded, usize) {
        let mut output = Decoded::default();
        let mut decoder = Decoder::default();
        decoder.mode = match *state { 0 => Mode::Ascii, 1 => Mode::Classic, 2 => Mode::Plane1,
            3 => Mode::Plane2, value => Mode::Unknown(value) };
        let mut scratch = Vec::new();
        let (mut offset, mut start, mut batch) = (0, 0, 0);
        while offset < input.len() {
            if decoder.state == 0 {
                start = offset;
                if output.points.len() - batch >= capacity - 1 {
                    if single { break; }
                    output.case_batches.push(output.points.len());
                    batch = output.points.len();
                }
                if input[offset] == 0x1b && input.len() - offset < 3 {
                    output.push(BAD_INPUT, start);
                    offset = input.len();
                    continue;
                }
            }
            decoder.push(input[offset], self, &mut scratch);
            for code in scratch.drain(..) { output.push(code, start); }
            offset += 1;
        }
        if decoder.state != 0 { output.push(BAD_INPUT, start); }
        if !input.is_empty() { output.case_batches.push(output.points.len()); }
        *state = match decoder.mode { Mode::Ascii => 0, Mode::Classic => 1, Mode::Plane1 => 2,
            Mode::Plane2 => 3, Mode::Unknown(value) => value };
        (output, offset)
    }

    /// Encodes complete Unicode streams with shared 2004 scalar and composite mappings.
    pub fn encode(self, input: &[u32], substitute: Substitute) -> Vec<u8> {
        self.encode_prefix(input, substitute, true)
    }

    /// Encodes a candidate MIME line while optionally retaining unflushed output state.
    pub fn encode_prefix(self, input: &[u32], substitute: Substitute, finish: bool) -> Vec<u8> {
        let mut encoder = Encoder::new(self, false);
        let mut output = Vec::new();
        for &code in input { encoder.append(code, substitute, &mut output); }
        if finish { encoder.close(&mut output); }
        output
    }

    /// Retains the longest legacy streaming prefix fitting the clamped encoded-byte budget.
    pub fn cut(self, input: &[u8], from: usize, length: usize) -> Vec<u8> {
        let budget = length.min(input.len() - from);
        let mut decoder = Decoder::default();
        let mut scratch = Vec::new();
        for &byte in &input[..from] { decoder.push(byte, self, &mut scratch); scratch.clear(); }
        let mut encoder = Encoder::new(self, true);
        let mut output = Vec::new();
        for &byte in &input[from..] {
            let (previous, previous_length) = (encoder, output.len());
            decoder.push(byte, self, &mut scratch);
            for code in scratch.drain(..) { encoder.append(code, Substitute::default(), &mut output); }
            let mut finished = encoder;
            let mut tail = Vec::new();
            if decoder.state != 0 { finished.append(BAD_INPUT, Substitute::default(), &mut tail); }
            finished.close(&mut tail);
            if output.len() + tail.len() > budget {
                encoder = previous;
                output.truncate(previous_length);
                break;
            }
        }
        encoder.close(&mut output);
        output
    }

    /// Maps a validated pair through the selected shared Japanese plane.
    fn pair(self, mode: Mode, first: u8, second: u8, output: &mut Vec<u32>) {
        if !(0x21..=0x7e).contains(&second) { output.push(BAD_INPUT); return; }
        match mode {
            Mode::Classic | Mode::Unknown(_) => output.push(self.classic.decoded_pair(first | 0x80, second | 0x80)),
            Mode::Plane1 => output.extend(self.extended.decode(&[first | 0x80, second | 0x80]).points),
            Mode::Plane2 => {
                if !matches!(first, 0x21 | 0x23..=0x25 | 0x28 | 0x2c..=0x2f | 0x6e..=0x7e) {
                    output.push(BAD_INPUT);
                } else { output.extend(self.extended.decode(&[0x8f, first | 0x80, second | 0x80]).points); }
            }
            Mode::Ascii => unreachable!("ASCII has no pair state"),
        }
    }
}

/// Streaming pair/escape parser shared by modern decoding and legacy byte-budget cuts.
struct Decoder { mode: Mode, state: u8, first: u8 }

impl Default for Decoder {
    /// Starts an ASCII decoder with no pending bytes.
    fn default() -> Self { Self { mode: Mode::Ascii, state: 0, first: 0 } }
}

impl Decoder {
    /// Consumes a byte without retrying malformed escape bytes, as required by this transport.
    fn push(&mut self, byte: u8, codec: Jis2004, output: &mut Vec<u32>) {
        match self.state {
            1 => { self.state = 0; codec.pair(self.mode, self.first, byte, output); }
            2 => match byte {
                b'$' => self.state = 3,
                b'(' => self.state = 5,
                _ => { self.state = 0; output.push(BAD_INPUT); }
            },
            3 => match byte {
                b'B' => { self.state = 0; self.mode = Mode::Classic; }
                b'(' => self.state = 4,
                _ => { self.state = 0; output.push(BAD_INPUT); }
            },
            4 => {
                self.state = 0;
                match byte {
                    b'Q' => self.mode = Mode::Plane1,
                    b'P' => self.mode = Mode::Plane2,
                    _ => output.push(BAD_INPUT),
                }
            }
            5 => {
                self.state = 0;
                if byte == b'B' { self.mode = Mode::Ascii; } else { output.push(BAD_INPUT); }
            }
            _ => match byte {
                0x1b => self.state = 2,
                0x21..=0x7e if self.mode != Mode::Ascii => { self.state = 1; self.first = byte; }
                0..=0x7f => output.push(u32::from(byte)),
                _ => output.push(BAD_INPUT),
            },
        }
    }
}

/// Stateful output with one pending composable character and a legacy plane-state policy.
#[derive(Clone, Copy)]
struct Encoder { codec: Jis2004, mode: Mode, pending: Option<(u32, Substitute)>, legacy: bool }

impl Encoder {
    /// Creates an ASCII encoder using canonical or historical streaming flush rules.
    fn new(codec: Jis2004, legacy: bool) -> Self { Self { codec, mode: Mode::Ascii, pending: None, legacy } }

    /// Combines adjacent characters before mapping a scalar or applying replacement policy.
    fn append(&mut self, code: u32, substitute: Substitute, output: &mut Vec<u8>) {
        if let Some((previous, settings)) = self.pending.take() {
            if let Some((2, bytes)) = self.codec.extended.composite(&[previous, code]) {
                self.bytes(bytes, output);
                return;
            }
            self.scalar(previous, settings, output);
        }
        if self.codec.extended.composite_starts(code) { self.pending = Some((code, substitute)); }
        else { self.scalar(code, substitute, output); }
    }

    /// Encodes a standalone character while keeping substitution output in the same shift state.
    fn scalar(&mut self, code: u32, substitute: Substitute, output: &mut Vec<u8>) {
        if !self.push(code, output) { substitute.append(code, output, |code, output| self.push(code, output)); }
    }

    /// Rejects the EUC-JP-2004 kana-only form and emits any supported scalar mapping.
    fn push(&mut self, code: u32, output: &mut Vec<u8>) -> bool {
        let Some(bytes) = self.codec.extended.encoded(code) else { return false; };
        if bytes.first() == Some(&0x8e) { return false; }
        self.bytes(bytes, output);
        true
    }

    /// Reframes a shared EUC-JP-2004 character as the appropriate seven-bit transport plane.
    fn bytes(&mut self, bytes: &[u8], output: &mut Vec<u8>) {
        let (mode, payload) = match bytes {
            [_] => (Mode::Ascii, bytes),
            [_, _] => (Mode::Plane1, bytes),
            [0x8f, _, _] => (Mode::Plane2, &bytes[1..]),
            _ => unreachable!("canonical EUC-JP-2004 character"),
        };
        self.shift(mode, output);
        output.extend(payload.iter().map(|byte| byte & 0x7f));
    }

    /// Emits an escape when PHP's selected modern or legacy output state requires it.
    fn shift(&mut self, mode: Mode, output: &mut Vec<u8>) {
        if self.mode == mode || (self.legacy && self.mode != Mode::Ascii && mode != Mode::Ascii) { return; }
        output.extend_from_slice(match mode {
            Mode::Ascii => b"\x1b(B".as_slice(),
            Mode::Plane1 => b"\x1b$(Q",
            Mode::Plane2 => b"\x1b$(P",
            Mode::Classic => unreachable!("canonical 2004 output selects JIS X 0213"),
            Mode::Unknown(_) => unreachable!("encoder cannot select an unknown decoder state"),
        });
        self.mode = mode;
    }

    /// Flushes pending composites according to PHP's legacy status check, then closes ASCII.
    fn close(&mut self, output: &mut Vec<u8>) {
        if let Some((code, settings)) = self.pending.take() {
            if !self.legacy || self.mode == Mode::Ascii { self.scalar(code, settings, output); }
        }
        self.shift(Mode::Ascii, output);
    }
}
