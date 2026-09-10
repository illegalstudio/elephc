//! Purpose:
//! Implements PHP's permissive Base64 transfer decoder and 76-column encoder.
//!
//! Called from:
//! - The transfer encoding dispatcher and legacy cut driver.
//!
//! Key details:
//! - Invalid bytes emit markers without discarding partially accumulated Base64 digits.
//! - Encoder inputs are reduced to their low byte, including invalid decoder markers.

use crate::encoding::{Decoded, BAD_INPUT};
use super::cut::{Decode, Encode};

/// Standard Base64 alphabet, independent of locale and target character set.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Pending six-bit digits shared by modern and historical Base64 decoding.
#[derive(Clone, Default)]
pub(super) struct Decoder { bits: u32, digits: u8 }

impl Decode for Decoder {
    /// Accepts a digit, ignores PHP's padding/whitespace set, or emits an invalid-unit marker.
    fn push(&mut self, byte: u8, output: &mut Vec<u32>) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'\r' | b'\n' | b' ' | b'\t' | b'=' => return,
            _ => { output.push(BAD_INPUT); return; }
        };
        self.bits = (self.bits << 6) | u32::from(value);
        self.digits += 1;
        if self.digits == 4 {
            output.extend_from_slice(&[(self.bits >> 16) & 255, (self.bits >> 8) & 255, self.bits & 255]);
            self.bits = 0;
            self.digits = 0;
        }
    }

    /// Emits complete bytes from two or three final digits and silently discards a lone digit.
    fn finish(&mut self, output: &mut Vec<u32>) {
        if self.digits == 2 { output.push((self.bits >> 4) & 255); }
        if self.digits == 3 { output.extend_from_slice(&[(self.bits >> 10) & 255, (self.bits >> 2) & 255]); }
        self.bits = 0;
        self.digits = 0;
    }
}

/// Decodes complete byte triples with PHP's three-word output-buffer reservation.
pub(super) fn decode(input: &[u8], capacity: usize) -> Decoded {
    decode_state(input, capacity, &mut 0)
}

/// Preserves PHP's packed cache only when a bounded call leaves unread input bytes.
pub(super) fn decode_state(input: &[u8], capacity: usize, state: &mut u32) -> Decoded {
    decode_mode(input, capacity, state, false).0
}

/// Stops before the next reserved triple and saves PHP's partial Base64 cache.
pub(super) fn decode_next(input: &[u8], capacity: usize, state: &mut u32) -> (Decoded, usize) {
    decode_mode(input, capacity, state, true)
}

/// Shares invalid-byte ordering and PHP's EOF state behavior with the bounded source reader.
fn decode_mode(input: &[u8], capacity: usize, state: &mut u32, single: bool) -> (Decoded, usize) {
    let mut output = Decoded::default();
    let (mut bits, mut cache, mut batch) = (*state & 255, *state >> 8, 0);
    let mut offset = 0;
    while offset < input.len() {
        if output.points.len() - batch > capacity - 3 {
            if single { *state = (cache << 8) | (bits & 255); break; }
            output.case_batches.push(output.points.len());
            batch = output.points.len();
            *state = (cache << 8) | (bits & 255);
        }
        let position = offset;
        let byte = input[offset];
        offset += 1;
        let value = match byte {
            b'A'..=b'Z' => byte - b'A', b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52, b'+' => 62, b'/' => 63,
            b'\r' | b'\n' | b' ' | b'\t' | b'=' => continue,
            _ => { output.push(BAD_INPUT, position); continue; }
        };
        bits = bits.wrapping_add(6);
        cache = (cache << 6) | u32::from(value);
        if bits == 24 {
            for code in [(cache >> 16) & 255, (cache >> 8) & 255, cache & 255] { output.push(code, position); }
            bits = 0;
            cache = 0;
        }
    }
    let position = input.len().saturating_sub(1);
    if offset == input.len() && bits == 18 { output.push((cache >> 10) & 255, position); output.push((cache >> 2) & 255, position); }
    else if offset == input.len() && bits == 12 { output.push((cache >> 4) & 255, position); }
    if output.points.len() != batch { output.case_batches.push(output.points.len()); }
    (output, offset)
}

/// Encodes raw bytes while optionally withholding the final incomplete Base64 quartet.
pub(super) fn encode_prefix(input: &[u32], finish: bool) -> Vec<u8> {
    let mut encoder = Encoder::default();
    let mut output = Vec::new();
    for &point in input { encoder.push(point, &mut output); }
    if finish { encoder.finish(&mut output); }
    output
}

/// Pending raw bytes and the current encoded line length.
#[derive(Clone, Default)]
pub(super) struct Encoder { bytes: [u8; 3], count: usize, column: usize }

impl Encoder {
    /// Emits one padded quartet, inserting CRLF only before output beyond column 76.
    fn quartet(&mut self, output: &mut Vec<u8>) {
        if self.column > 72 { output.extend_from_slice(b"\r\n"); self.column = 0; }
        let [a, b, c] = self.bytes;
        output.push(ALPHABET[usize::from(a >> 2)]);
        output.push(ALPHABET[usize::from(((a & 3) << 4) | (b >> 4))]);
        output.push(if self.count > 1 { ALPHABET[usize::from(((b & 15) << 2) | (c >> 6))] } else { b'=' });
        output.push(if self.count > 2 { ALPHABET[usize::from(c & 63)] } else { b'=' });
        self.column += 4;
        self.bytes = [0; 3];
        self.count = 0;
    }
}

impl Encode for Encoder {
    /// Buffers each low byte until a complete quartet can be emitted.
    fn push(&mut self, code: u32, output: &mut Vec<u8>) {
        self.bytes[self.count] = code as u8;
        self.count += 1;
        if self.count == 3 { self.quartet(output); }
    }

    /// Pads and emits an incomplete final quartet without a trailing line ending.
    fn finish(&mut self, output: &mut Vec<u8>) {
        if self.count > 0 { self.quartet(output); }
    }
}

/// Encodes a complete raw-byte stream with continuous line wrapping and final padding.
pub(super) fn encode(input: &[u32]) -> Vec<u8> {
    let mut encoder = Encoder::default();
    let mut output = Vec::new();
    for &code in input { encoder.push(code, &mut output); }
    encoder.finish(&mut output);
    output
}
