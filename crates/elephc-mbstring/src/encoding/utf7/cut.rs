//! Purpose:
//! Implements byte-budget UTF-7 cuts through PHP's legacy streaming conversion semantics.
//!
//! Called from:
//! - `super::Utf7::cut` for mb_strcut.
//!
//! Key details:
//! - Prefix bytes update decoder state without producing output.
//! - Each accepted byte must fit after a provisional flush; final flush errors are suppressed.
//! - Streaming IMAP preserves historical surrogate and literal-NUL behavior.

use crate::encoding::{Substitute, BAD_INPUT};
use super::encoder::Encoder;

/// Streams to the requested start and retains the last state fitting the clamped byte budget.
pub(super) fn cut(input: &[u8], from: usize, length: usize, imap: bool) -> Vec<u8> {
    let budget = length.min(input.len() - from);
    let mut decoder = Decoder { imap, state: 0, cache: 0 };
    let mut scratch = Vec::new();
    for &byte in &input[..from] {
        decoder.push(byte, &mut scratch);
        scratch.clear();
    }
    let mut encoder = Encoder::new(imap, true);
    let mut output = Vec::new();
    for &byte in &input[from..] {
        let (previous_encoder, previous_length) = (encoder, output.len());
        decoder.push(byte, &mut scratch);
        for code in scratch.drain(..) { append(&mut encoder, code, &mut output); }
        let mut finished = encoder;
        let mut tail = Vec::new();
        if decoder.flush_error() { append(&mut finished, BAD_INPUT, &mut tail); }
        finished.close(true, &mut tail);
        if output.len() + tail.len() > budget {
            output.truncate(previous_length);
            encoder = previous_encoder;
            break;
        }
    }
    encoder.close(true, &mut output);
    output
}

/// Encodes a streaming output unit using the filter's fixed default replacement policy.
fn append(encoder: &mut Encoder, code: u32, output: &mut Vec<u8>) {
    if !encoder.push(code, output) {
        Substitute::default().append(code, output, |code, output| encoder.push(code, output));
    }
}

/// Retains PHP's legacy filter status and combined partial-word/surrogate cache.
struct Decoder {
    imap: bool,
    state: u8,
    cache: u32,
}

impl Decoder {
    /// Consumes one byte and appends only codepoints emitted at that byte boundary.
    fn push(&mut self, byte: u8, output: &mut Vec<u32>) {
        if self.state == 0 {
            if byte == if self.imap { b'&' } else { b'+' } {
                self.state = 1;
            } else if if self.imap { (0x20..=0x7e).contains(&byte) } else { byte < 128 } {
                output.push(u32::from(byte));
            } else {
                output.push(BAD_INPUT);
            }
            return;
        }
        let Some(digit) = super::digit(byte, self.imap) else {
            if self.imap {
                if byte == b'-' && self.state == 1 { output.push(u32::from(b'&')); }
                else if byte != b'-' || self.cache != 0 { output.push(BAD_INPUT); }
            } else {
                if self.cache != 0 { output.push(BAD_INPUT); }
                if byte == b'-' {
                    if self.state == 1 { output.push(u32::from(b'+')); }
                } else {
                    output.push(if byte < 128 { u32::from(byte) } else { BAD_INPUT });
                }
            }
            self.state = 0;
            self.cache = 0;
            return;
        };
        let completed = match self.state {
            1 | 2 => { self.cache |= digit << 10; self.state = 3; None }
            3 => { self.cache |= digit << 4; self.state = 4; None }
            4 => { self.state = 5; Some(((digit >> 2) | (self.cache & 0xffff), (digit & 3) << 14)) }
            5 => { self.cache |= digit << 8; self.state = 6; None }
            6 => { self.cache |= digit << 2; self.state = 7; None }
            7 => { self.state = 8; Some(((digit >> 4) | (self.cache & 0xffff), (digit & 15) << 12)) }
            8 => { self.cache |= digit << 6; self.state = 9; None }
            9 => { self.state = 2; Some((digit | (self.cache & 0xffff), 0)) }
            _ => unreachable!("UTF-7 streaming state"),
        };
        if let Some((code, tail)) = completed { self.unit(code, tail, output); }
    }

    /// Updates pending UTF-16 state, preserving the legacy IMAP filter's malformed-pair rules.
    fn unit(&mut self, code: u32, tail: u32, output: &mut Vec<u32>) {
        let pending = self.cache & 0xfff0000;
        if (0xd800..=0xdbff).contains(&code) {
            if pending != 0 && !self.imap { output.push(BAD_INPUT); }
            self.cache = (((code & 0x3ff) << 16) + 0x400000) | tail;
        } else if (0xdc00..=0xdfff).contains(&code) {
            if pending != 0 {
                output.push((code & 0x3ff) | (pending >> 6));
                self.cache = tail;
            } else {
                output.push(BAD_INPUT);
                if !self.imap { self.cache = tail; }
            }
        } else {
            if pending != 0 && !self.imap { output.push(BAD_INPUT); }
            self.cache = tail;
            output.push(if self.imap && (0x20..=0x7e).contains(&code) && code != u32::from(b'&') { BAD_INPUT } else { code });
        }
    }

    /// Reports whether a provisional legacy decoder flush emits one invalid-unit marker.
    fn flush_error(&self) -> bool {
        if self.imap { self.state != 0 } else { self.cache != 0 }
    }
}
