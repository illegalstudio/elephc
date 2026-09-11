//! Purpose:
//! Decodes permissive JIS control syntax while preserving stricter validation and batch limits.
//!
//! Called from:
//! - `super::Jis::decode`.
//!
//! Key details:
//! - Invalid escape prefixes can emit several codepoints before the final byte is retried.
//! - Decoder batches never split those prefix emissions.
//! - Validation distinguishes SO-invoked kana from escape-invoked kana.

use crate::encoding::{Decoded, BAD_INPUT};
use super::{Jis, Mode, Variant};

/// Decodes complete token steps and records PHP's 64-word output boundaries for casing.
pub(super) fn decode(input: &[u8], codec: Jis) -> Decoded {
    decode_buffer(input, codec, 64)
}

/// Uses the caller's scratch-buffer capacity for atomic malformed escape expansions.
pub(super) fn decode_buffer(input: &[u8], codec: Jis, capacity: usize) -> Decoded {
    decode_state(input, codec, capacity, &mut 0)
}

/// Retains only the selected character plane across complete MIME word boundaries.
pub(super) fn decode_state(input: &[u8], codec: Jis, capacity: usize, state: &mut u32) -> Decoded {
    decode_mode(input, codec, capacity, state, false).0
}

/// Decodes one PHP scratch-buffer call and returns its consumed byte count and saved state.
pub(super) fn decode_next(input: &[u8], codec: Jis, capacity: usize, state: &mut u32) -> (Decoded, usize) {
    decode_mode(input, codec, capacity, state, true)
}

/// Shares complete-stream decoding with the bounded MIME source reader.
fn decode_mode(input: &[u8], codec: Jis, capacity: usize, state: &mut u32, single: bool) -> (Decoded, usize) {
    let mut output = Decoded::default();
    let (mut offset, mut batch, mut mode) = (0, 0, Mode::from_state(*state, codec.variant));
    while offset < input.len() {
        let step = Step::read(&input[offset..], mode, codec);
        if output.points.len() - batch == capacity || output.points.len() - batch + step.count > capacity {
            if single { break; }
            output.case_batches.push(output.points.len());
            batch = output.points.len();
        }
        for &code in &step.points[..step.count] { output.push(code, offset); }
        output.validation_error |= step.invalid && !codec.variant.is_cp5022x();
        offset += step.bytes;
        mode = step.mode;
    }
    output.validation_error |= offset == input.len() && mode != Mode::Ascii && !codec.variant.is_cp5022x();
    if !input.is_empty() { output.case_batches.push(output.points.len()); }
    *state = mode.to_state(codec.variant);
    (output, offset)
}

/// One complete input unit, including an atomic malformed escape-prefix expansion.
struct Step {
    bytes: usize,
    points: [u32; 3],
    count: usize,
    mode: Mode,
    invalid: bool,
}

impl Step {
    /// Reads one unit without mutating the caller's state until its output fits the batch.
    fn read(input: &[u8], mode: Mode, codec: Jis) -> Self {
        let byte = input[0];
        let mut step = Self { bytes: 1, points: [0; 3], count: 0, mode, invalid: false };
        if byte == 0x1b {
            step.escape(input, codec.variant == Variant::Iso2022);
        } else if byte == 0x0e {
            step.mode = Mode::KanaSo;
            step.invalid = codec.variant == Variant::Iso2022 || mode != Mode::Ascii;
        } else if byte == 0x0f {
            step.mode = Mode::Ascii;
            step.invalid = codec.variant == Variant::Iso2022 || mode != Mode::KanaSo;
        } else if mode == Mode::Roman && byte == b'\\' {
            step.emit(&[0xa5]);
        } else if mode == Mode::Roman && byte == b'~' {
            step.emit(&[0x203e]);
        } else if matches!(mode, Mode::Kana | Mode::KanaSo) && (0x21..=0x5f).contains(&byte) {
            step.emit(&[u32::from(byte) + 0xff40]);
        } else if (matches!(mode, Mode::Kanji | Mode::Plane212) || matches!(mode, Mode::Unknown(value) if value >= 3))
            && (0x21..=if codec.variant.is_cp5022x() { 0x97 } else { 0x7e }).contains(&byte) {
            if let Some(&second) = input.get(1) {
                step.bytes = 2;
                step.emit(&[codec.pair(mode, byte, second)]);
            } else { step.emit(&[BAD_INPUT]); }
        } else if byte < 128 {
            step.emit(&[u32::from(byte)]);
        } else if (0xa1..=0xdf).contains(&byte) {
            step.emit(&[u32::from(byte) + 0xfec0]);
            step.invalid = codec.variant == Variant::Iso2022;
        } else {
            step.emit(&[BAD_INPUT]);
        }
        step
    }

    /// Selects a recognized escape or emits its invalid prefix while leaving the final byte unread.
    fn escape(&mut self, input: &[u8], iso_only: bool) {
        self.invalid = self.mode == Mode::KanaSo;
        let (bytes, mode) = if input.len() >= 3 {
            match (input[1], input[2]) {
                (b'$', b'@' | b'B') => (3, Some(Mode::Kanji)),
                (b'(', b'B' | b'H') => (3, Some(Mode::Ascii)),
                (b'(', b'J') => (3, Some(Mode::Roman)),
                (b'(', b'I') => (3, Some(Mode::Kana)),
                (b'$', b'(') if input.len() >= 4 => match input[3] {
                    b'@' | b'B' => (4, Some(Mode::Kanji)),
                    b'D' => (4, Some(Mode::Plane212)),
                    _ => (3, None),
                },
                (b'$', b'(') => { self.bytes = 3; self.emit(&[BAD_INPUT]); return; }
                (b'$', _) | (b'(', _) => (2, None),
                _ => (1, None),
            }
        } else {
            self.bytes = if input.get(1).is_some_and(|byte| matches!(byte, b'$' | b'(')) { 2 } else { 1 };
            self.emit(&[BAD_INPUT]);
            return;
        };
        self.bytes = bytes;
        if let Some(mode) = mode {
            self.mode = mode;
            self.invalid |= iso_only && !(bytes == 3
                && ((input[1] == b'$' && matches!(input[2], b'@' | b'B'))
                    || (input[1] == b'(' && matches!(input[2], b'B' | b'J'))));
        } else {
            self.invalid = true;
            match bytes {
                3 => self.emit(&[BAD_INPUT, u32::from(b'$'), u32::from(b'(')]),
                2 if input[1] == b'$' => self.emit(&[BAD_INPUT, u32::from(b'$')]),
                2 => self.emit(&[BAD_INPUT, u32::from(b'(')]),
                _ => self.emit(&[BAD_INPUT]),
            }
        }
    }

    /// Assigns the bounded codepoint sequence emitted atomically by this decoder step.
    fn emit(&mut self, points: &[u32]) {
        self.points[..points.len()].copy_from_slice(points);
        self.count = points.len();
    }
}
