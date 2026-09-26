//! Purpose:
//! Implements ISO-2022-JP-MS control syntax and its CP932 and private-use character modes.
//!
//! Called from:
//! - `super::Jis` for Microsoft-variant decoding and streaming cuts.
//!
//! Key details:
//! - ESC ( J selects ASCII, and SI/SO bytes remain ordinary controls.
//! - Invalid escape prefixes are consumed rather than replayed as punctuation.
//! - Shared CP51932 mappings cover the CP932 extension rows in the standard Kanji mode.

use crate::encoding::{Decoded, Substitute, BAD_INPUT};
use super::{encoder::Encoder, Jis, Mode};

/// Decodes complete bytes and emits one error for any unfinished escape or Kanji pair.
pub(super) fn decode(input: &[u8], codec: Jis) -> Decoded {
    decode_state(input, codec, &mut 0)
}

/// Continues the Microsoft shift mode while rejecting any incomplete unit at the word end.
pub(super) fn decode_state(input: &[u8], codec: Jis, state: &mut u32) -> Decoded {
    decode_next(input, codec, usize::MAX, state).0
}

/// Stops after complete input units, preserving the shift mode for the next buffer.
pub(super) fn decode_next(input: &[u8], codec: Jis, capacity: usize, state: &mut u32) -> (Decoded, usize) {
    let mut decoder = Decoder::new();
    decoder.mode = Mode::from_state(*state, codec.variant);
    let mut output = Decoded::default();
    let mut start = 0;
    let mut consumed = 0;
    for (offset, &byte) in input.iter().enumerate() {
        if output.points.len() == capacity { break; }
        consumed = offset + 1;
        if decoder.state == 0 { start = offset; }
        if let Some(code) = decoder.push(byte, codec) { output.push(code, start); }
    }
    if decoder.state != 0 { output.push(BAD_INPUT, start); }
    *state = decoder.mode.to_state(codec.variant);
    (output, consumed)
}

/// Cuts through the same streaming decoder with provisional default-substitution flushes.
pub(super) fn cut(input: &[u8], from: usize, length: usize, codec: Jis) -> Vec<u8> {
    let budget = length.min(input.len() - from);
    let mut decoder = Decoder::new();
    for &byte in &input[..from] { decoder.push(byte, codec); }
    let mut encoder = Encoder::new(codec, true);
    let mut output = Vec::new();
    for &byte in &input[from..] {
        let (previous, previous_length) = (encoder, output.len());
        if let Some(code) = decoder.push(byte, codec) { encoder.append(code, Substitute::default(), &mut output); }
        let mut finished = encoder;
        let mut tail = Vec::new();
        if decoder.state != 0 { finished.append(BAD_INPUT, Substitute::default(), &mut tail); }
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

/// Retains the active Microsoft character set and one incomplete input unit.
struct Decoder {
    mode: Mode,
    state: u8,
    first: u8,
}

impl Decoder {
    /// Starts an ASCII-mode stream with no pending escape or character byte.
    fn new() -> Self {
        Self { mode: Mode::Ascii, state: 0, first: 0 }
    }

    /// Consumes one byte and emits at most one character or invalid-unit marker.
    fn push(&mut self, byte: u8, codec: Jis) -> Option<u32> {
        match self.state {
            1 => { self.state = 0; Some(codec.pair(self.mode, self.first, byte)) }
            2 => match byte {
                b'$' => { self.state = 3; None }
                b'(' => { self.state = 5; None }
                _ => { self.state = 0; Some(BAD_INPUT) }
            },
            3 => match byte {
                b'@' | b'B' => self.select(Mode::Kanji),
                b'(' => { self.state = 4; None }
                _ => { self.state = 0; Some(BAD_INPUT) }
            },
            4 => match byte {
                b'@' | b'B' => self.select(Mode::Kanji),
                b'?' => self.select(Mode::User),
                _ => { self.state = 0; Some(BAD_INPUT) }
            },
            5 => match byte {
                b'B' | b'J' => self.select(Mode::Ascii),
                b'I' => self.select(Mode::Kana),
                _ => { self.state = 0; Some(BAD_INPUT) }
            },
            _ => match byte {
                0x1b => { self.state = 2; None }
                byte if self.mode == Mode::Kana && (0x21..=0x5f).contains(&byte) => Some(u32::from(byte) + 0xff40),
                byte if matches!(self.mode, Mode::Kanji | Mode::User) && (0x21..=0x7f).contains(&byte) => {
                    self.first = byte;
                    self.state = 1;
                    None
                }
                byte if byte < 128 => Some(u32::from(byte)),
                byte if (0xa1..=0xdf).contains(&byte) => Some(u32::from(byte) + 0xfec0),
                _ => Some(BAD_INPUT),
            },
        }
    }

    /// Completes an escape sequence without emitting a character.
    fn select(&mut self, mode: Mode) -> Option<u32> {
        self.mode = mode;
        self.state = 0;
        None
    }
}
