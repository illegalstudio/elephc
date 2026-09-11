//! Purpose:
//! Implements KDDI mobile JIS decoding and the historical byte-budget cut adapter.
//!
//! Called from:
//! - The shared JIS codec descriptor.
//!
//! Key details:
//! - Emoji pair expansions remain atomic inside the reserved-slot decoder batches.
//! - Modern conversion composes flags; legacy cuts compose only telephone keycaps.

use crate::encoding::{mapping::word, Decoded, Substitute, BAD_INPUT};
use super::{encoder::Encoder, Jis, Mode};

/// Decodes a complete mobile JIS stream and retains the source decoder's 63-word limits.
pub(super) fn decode(input: &[u8]) -> Decoded {
    decode_buffer(input, 64)
}

/// Uses the requested scratch-buffer size while reserving room for a two-codepoint emoji.
pub(super) fn decode_buffer(input: &[u8], capacity: usize) -> Decoded {
    decode_state(input, capacity, &mut 0)
}

/// Keeps KDDI's numeric shift mode across separate MIME words, discarding incomplete units.
pub(super) fn decode_state(input: &[u8], capacity: usize, state: &mut u32) -> Decoded {
    decode_mode(input, capacity, state, false).0
}

/// Decodes one PHP scratch-buffer call and returns its consumed byte count and saved state.
pub(super) fn decode_next(input: &[u8], capacity: usize, state: &mut u32) -> (Decoded, usize) {
    decode_mode(input, capacity, state, true)
}

/// Shares complete-stream decoding with the bounded MIME source reader.
fn decode_mode(input: &[u8], capacity: usize, state: &mut u32, single: bool) -> (Decoded, usize) {
    let mut decoder = Decoder::default();
    decoder.mode = Mode::from_state(*state, super::Variant::Kddi);
    let mut output = Decoded::default();
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
        decoder.push(input[offset], &mut scratch);
        for code in scratch.drain(..) { output.push(code, start); }
        offset += 1;
    }
    if decoder.state != 0 { output.push(BAD_INPUT, start); }
    if !input.is_empty() { output.case_batches.push(output.points.len()); }
    *state = decoder.mode.to_state(super::Variant::Kddi);
    (output, offset)
}

/// Cuts using bytewise legacy decoding and fixed default replacement during provisional flushes.
pub(super) fn cut(input: &[u8], from: usize, length: usize, codec: Jis) -> Vec<u8> {
    let budget = length.min(input.len() - from);
    let mut decoder = Decoder::default();
    let mut scratch = Vec::new();
    for &byte in &input[..from] { decoder.push(byte, &mut scratch); scratch.clear(); }
    let mut encoder = Encoder::new(codec, true);
    let mut output = Vec::new();
    for &byte in &input[from..] {
        let (previous, previous_length) = (encoder, output.len());
        decoder.push(byte, &mut scratch);
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
    encoder.close_cut(&mut output);
    output
}

/// Retains the active mobile character mode and one incomplete pair or escape prefix.
struct Decoder { mode: Mode, state: u8, first: u8 }

impl Default for Decoder {
    /// Starts in ASCII mode without a pending pair or escape.
    fn default() -> Self { Self { mode: Mode::Ascii, state: 0, first: 0 } }
}

impl Decoder {
    /// Consumes one byte, preserving permissive GR kana and complete emoji expansions.
    fn push(&mut self, byte: u8, output: &mut Vec<u32>) {
        match self.state {
            1 => {
                self.state = 0;
                if !(0x21..=0x7e).contains(&byte) { output.push(BAD_INPUT); return; }
                let table = include_bytes!("../data/jis-kddi-plane.bin");
                let index = (usize::from(self.first - 0x21) * 94 + usize::from(byte - 0x21)) * 8;
                output.push(word(table, index));
                let second = word(table, index + 4);
                if second != 0 { output.push(second); }
            }
            2 => match byte {
                b'$' => self.state = 3,
                b'(' => self.state = 5,
                _ => { self.state = 0; output.push(BAD_INPUT); }
            },
            3 => match byte {
                b'@' | b'B' => { self.state = 0; self.mode = Mode::Kanji; }
                b'(' => self.state = 4,
                _ => { self.state = 0; output.push(BAD_INPUT); }
            },
            4 => {
                self.state = 0;
                if matches!(byte, b'@' | b'B') { self.mode = Mode::Kanji; }
                else { output.push(BAD_INPUT); }
            }
            5 => {
                self.state = 0;
                match byte {
                    b'B' | b'J' => self.mode = Mode::Ascii,
                    b'I' => self.mode = Mode::Kana,
                    _ => output.push(BAD_INPUT),
                }
            }
            _ => match byte {
                0x1b => self.state = 2,
                0x21..=0x5f if self.mode == Mode::Kana => output.push(u32::from(byte) + 0xff40),
                0x21..=0x7f if self.mode == Mode::Kanji => { self.state = 1; self.first = byte; }
                0..=0x7f => output.push(u32::from(byte)),
                0xa1..=0xdf => output.push(u32::from(byte) + 0xfec0),
                _ => output.push(BAD_INPUT),
            },
        }
    }
}
