//! Purpose:
//! Implements JIS byte-budget cuts using PHP's legacy streaming decoder and encoder policies.
//!
//! Called from:
//! - `super::Jis::cut`.
//!
//! Key details:
//! - Prefix bytes establish state while discarding emitted characters.
//! - Failed escape prefixes emit their preserved punctuation before retrying the current byte.
//! - Provisional flush checks include pending errors; final flush suppresses those errors.

use crate::encoding::{Substitute, BAD_INPUT};
use super::{encoder::Encoder, Jis, Mode};

/// Retains the longest accepted streaming state under the clamped encoded-byte budget.
pub(super) fn cut(input: &[u8], from: usize, length: usize, codec: Jis) -> Vec<u8> {
    let budget = length.min(input.len() - from);
    let mut decoder = Decoder { mode: Mode::Ascii, state: 0, first: 0 };
    let mut scratch = Vec::new();
    for &byte in &input[..from] {
        decoder.push(byte, codec, &mut scratch);
        scratch.clear();
    }
    let mut encoder = Encoder::new(codec, true);
    let mut output = Vec::new();
    for &byte in &input[from..] {
        let (previous, previous_length) = (encoder, output.len());
        decoder.push(byte, codec, &mut scratch);
        for code in scratch.drain(..) { encoder.append(code, Substitute::default(), &mut output); }
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
    encoder.close_cut(&mut output);
    output
}

/// Retains one incomplete pair or escape prefix together with the active character mode.
struct Decoder {
    mode: Mode,
    state: u8,
    first: u8,
}

impl Decoder {
    /// Consumes one byte, retrying it after a malformed escape prefix when PHP does so.
    fn push(&mut self, byte: u8, codec: Jis, output: &mut Vec<u32>) {
        loop {
            match self.state {
                1 => {
                    self.state = 0;
                    output.push(codec.pair(self.mode, self.first, byte));
                }
                2 => match byte {
                    b'$' => self.state = 3,
                    b'(' => self.state = 5,
                    _ => { self.state = 0; output.push(BAD_INPUT); continue; }
                },
                3 => match byte {
                    b'@' | b'B' => { self.state = 0; self.mode = Mode::Kanji; }
                    b'(' => self.state = 4,
                    _ => { self.state = 0; output.extend_from_slice(&[BAD_INPUT, u32::from(b'$')]); continue; }
                },
                4 => match byte {
                    b'@' | b'B' => { self.state = 0; self.mode = Mode::Kanji; }
                    b'D' => { self.state = 0; self.mode = Mode::Plane212; }
                    _ => {
                        self.state = 0;
                        output.extend_from_slice(&[BAD_INPUT, u32::from(b'$'), u32::from(b'(')]);
                        continue;
                    }
                },
                5 => match byte {
                    b'B' | b'H' => { self.state = 0; self.mode = Mode::Ascii; }
                    b'J' => { self.state = 0; self.mode = Mode::Roman; }
                    b'I' => { self.state = 0; self.mode = Mode::Kana; }
                    _ => { self.state = 0; output.extend_from_slice(&[BAD_INPUT, u32::from(b'(')]); continue; }
                },
                _ => match byte {
                    0x1b => self.state = 2,
                    0x0e => self.mode = Mode::Kana,
                    0x0f => self.mode = Mode::Ascii,
                    b'\\' if self.mode == Mode::Roman => output.push(0xa5),
                    b'~' if self.mode == Mode::Roman => output.push(0x203e),
                    byte if self.mode == Mode::Kana && (0x21..=0x5f).contains(&byte) => output.push(u32::from(byte) + 0xff40),
                    byte if matches!(self.mode, Mode::Kanji | Mode::Plane212)
                        && (0x21..=if codec.variant.is_cp5022x() { 0x97 } else { 0x7e }).contains(&byte) => {
                        self.first = byte;
                        self.state = 1;
                    }
                    byte if byte < 128 => output.push(u32::from(byte)),
                    byte if (0xa1..=0xdf).contains(&byte) => output.push(u32::from(byte) + 0xfec0),
                    _ => output.push(BAD_INPUT),
                },
            }
            break;
        }
    }
}
