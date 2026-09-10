//! Purpose:
//! Implements mbstring's named and numeric HTML transfer encoding and legacy entity cuts.
//!
//! Called from:
//! - The transfer encoding dispatcher and byte-budget cut driver.
//!
//! Key details:
//! - ASCII is preserved on output, including ampersands and angle brackets.
//! - Numeric input uses wrapping 32-bit arithmetic followed by PHP's Unicode-range check.
//! - Legacy cuts retain a bounded entity buffer and a distinct decimal-overflow guard.

mod data;

use crate::encoding::Decoded;
use super::cut::{Decode, Encode};

/// Looks up one case-sensitive HTML entity name without accepting HTML5-only additions.
fn named(name: &[u8]) -> Option<u32> {
    data::DECODE.binary_search_by(|(candidate, _)| candidate.cmp(&name)).ok().map(|index| data::DECODE[index].1)
}

/// Resolves a complete entity body using modern or legacy numeric and NUL handling.
fn entity(body: &[u8], legacy: bool) -> Option<u32> {
    if let Some(number) = body.strip_prefix(b"#") {
        let (digits, radix) = if number.first().is_some_and(|byte| matches!(byte, b'x' | b'X')) { (&number[1..], 16) }
            else { (number, 10) };
        if digits.is_empty() { return None; }
        let mut code = 0u32;
        for &byte in digits {
            if legacy && radix == 10 && code > 0x19999999 { return None; }
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'A'..=b'F' if radix == 16 => u32::from(byte - b'A' + 10),
                b'a'..=b'f' if radix == 16 => u32::from(byte - b'a' + 10),
                _ => return None,
            };
            code = code.wrapping_mul(radix).wrapping_add(digit);
        }
        (code <= 0x10ffff).then_some(code)
    } else {
        let end = if legacy { body.iter().position(|&byte| byte == 0).unwrap_or(body.len()) } else { body.len() };
        named(&body[..end])
    }
}

/// Decodes complete HTML entities and preserves malformed or unknown input as literal bytes.
pub(super) fn decode(input: &[u8], capacity: usize) -> Decoded {
    decode_mode(input, capacity, false).0
}

/// Returns one transfer-decoder buffer and the exact consumed input byte count.
pub(super) fn decode_next(input: &[u8], capacity: usize) -> (Decoded, usize) {
    decode_mode(input, capacity, true)
}

/// Keeps bounded source reads and complete-stream conversion on the same token parser.
fn decode_mode(input: &[u8], capacity: usize, single: bool) -> (Decoded, usize) {
    let mut output = Decoded::default();
    let mut offset = 0;
    while offset < input.len() && (!single || output.points.len() < capacity) {
        let start = offset;
        let byte = input[offset];
        offset += 1;
        if byte == b'&' {
            let count = input[offset..].iter().take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'#').count();
            let end = offset + count;
            if input.get(end) == Some(&b';') {
                if let Some(code) = entity(&input[offset..end], false) { output.push(code, start); offset = end + 1; continue; }
            }
        }
        output.push(u32::from(byte), start);
    }
    output.case_batches.extend((capacity..output.points.len()).step_by(capacity));
    if !output.points.is_empty() { output.case_batches.push(output.points.len()); }
    (output, offset)
}

/// Emits ASCII literally, otherwise selecting PHP's preferred name or decimal numeric entity.
fn emit(code: u32, output: &mut Vec<u8>) {
    if code < 128 { output.push(code as u8); return; }
    if let Ok(index) = data::ENCODE.binary_search_by_key(&code, |&(code, _)| code) {
        output.extend_from_slice(data::ENCODE[index].1);
    } else { output.extend_from_slice(format!("&#{code};").as_bytes()); }
}

/// Encodes every codepoint, including raw UCS values and invalid markers, without substitution.
pub(super) fn encode(input: &[u32]) -> Vec<u8> {
    let mut output = Vec::new();
    for &code in input { emit(code, &mut output); }
    output
}

/// PHP's historical bounded entity prefix used by mb_strcut.
#[derive(Clone, Default)]
pub(super) struct Decoder { pending: Vec<u8> }

impl Decode for Decoder {
    /// Accumulates an entity or flushes a malformed prefix, retaining a new ampersand when needed.
    fn push(&mut self, byte: u8, output: &mut Vec<u32>) {
        if self.pending.is_empty() {
            if byte == b'&' { self.pending.push(byte); } else { output.push(u32::from(byte)); }
            return;
        }
        if byte == b';' {
            if let Some(code) = entity(&self.pending[1..], true) { output.push(code); self.pending.clear(); }
            else { self.pending.push(byte); self.finish(output); }
            return;
        }
        self.pending.push(byte);
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'#' | 0)) || self.pending.len() == 15
            || (byte == b'#' && self.pending.len() > 2)
        {
            if byte == b'&' { self.pending.pop(); }
            self.finish(output);
            if byte == b'&' { self.pending.push(byte); }
        }
    }

    /// Emits every pending prefix byte literally at the end of a cut candidate.
    fn finish(&mut self, output: &mut Vec<u32>) { output.extend(self.pending.drain(..).map(u32::from)); }
}

/// Stateless HTML output adapter for the shared legacy cut driver.
#[derive(Clone)]
pub(super) struct Encoder;

impl Encode for Encoder {
    /// Emits one legacy-decoded codepoint using the canonical HTML representation.
    fn push(&mut self, code: u32, output: &mut Vec<u8>) { emit(code, output); }
    /// Completes the stateless HTML output filter without adding bytes.
    fn finish(&mut self, _output: &mut Vec<u8>) {}
}
