//! Purpose:
//! Implements mbstring's permissive quoted-printable decoding and historical line rules.
//!
//! Called from:
//! - The transfer encoding dispatcher and legacy cut driver.
//!
//! Key details:
//! - Malformed hex escapes remain literal and may expand atomically to three words.
//! - Modern encoding drops CR; legacy cuts normalize a lone CR to CRLF.

use crate::encoding::Decoded;
use super::cut::{Decode, Encode};

/// Converts an ASCII hex digit without accepting other characters or locale-specific digits.
fn hex(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'A'..=b'F' => Some(u32::from(byte - b'A' + 10)),
        b'a'..=b'f' => Some(u32::from(byte - b'a' + 10)),
        _ => None,
    }
}

/// Decodes full modern tokens, preserving PHP's truncated-CR and invalid-hex behavior.
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
    let (mut offset, mut batch) = (0, 0);
    while offset < input.len() {
        if output.points.len() - batch >= capacity - 2 {
            if single { break; }
            output.case_batches.push(output.points.len());
            batch = output.points.len();
        }
        let start = offset;
        let byte = input[offset];
        offset += 1;
        if byte != b'=' || offset == input.len() { output.push(u32::from(byte), start); continue; }
        let second = input[offset];
        offset += 1;
        if let Some(high) = hex(second).filter(|_| offset < input.len()) {
            let third = input[offset];
            offset += 1;
            if let Some(low) = hex(third) { output.push((high << 4) | low, start); }
            else { for code in [byte, second, third] { output.push(u32::from(code), start); } }
        } else if second == b'\r' && offset < input.len() {
            let third = input[offset];
            offset += 1;
            if third != b'\n' { output.push(u32::from(third), start); }
        } else if second != b'\n' {
            output.push(u32::from(byte), start);
            output.push(u32::from(second), start);
        }
    }
    if output.points.len() != batch { output.case_batches.push(output.points.len()); }
    (output, offset)
}

/// Emits a raw transfer codepoint and updates the current output column.
fn emit(code: u32, column: &mut usize, output: &mut Vec<u8>) {
    if code == 0 { output.push(0); *column = 0; return; }
    if code == 10 { output.extend_from_slice(b"\r\n"); *column = 0; return; }
    if code == 13 { return; }
    if *column >= 72 { output.extend_from_slice(b"=\r\n"); *column = 0; }
    if code >= 128 || code == 61 {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        output.extend_from_slice(&[b'=', HEX[((code >> 4) & 15) as usize], HEX[(code & 15) as usize]]);
        *column += 3;
    } else { output.push(code as u8); *column += 1; }
}

/// Encodes with PHP's 72-column soft wraps, literal controls, and CR removal.
pub(super) fn encode(input: &[u32]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut column = 0;
    for &code in input { emit(code, &mut column, &mut output); }
    output
}

/// Incomplete escape state for PHP's historical bytewise quoted-printable filter.
#[derive(Clone, Default)]
pub(super) struct Decoder { state: u8, first: u8 }

impl Decode for Decoder {
    /// Decodes a byte or preserves a malformed escape exactly as the legacy filter emits it.
    fn push(&mut self, byte: u8, output: &mut Vec<u32>) {
        match self.state {
            1 => {
                if hex(byte).is_some() { self.first = byte; self.state = 2; }
                else if byte == b'\r' { self.state = 3; }
                else {
                    self.state = 0;
                    if byte != b'\n' { output.extend_from_slice(&[61, u32::from(byte)]); }
                }
            }
            2 => {
                self.state = 0;
                if let Some(low) = hex(byte) { output.push((hex(self.first).unwrap() << 4) | low); }
                else { output.extend_from_slice(&[61, u32::from(self.first), u32::from(byte)]); }
            }
            3 => { self.state = 0; if byte != b'\n' { output.push(u32::from(byte)); } }
            _ if byte == b'=' => self.state = 1,
            _ => output.push(u32::from(byte)),
        }
    }

    /// Flushes incomplete hex prefixes while discarding a pending soft CR line ending.
    fn finish(&mut self, output: &mut Vec<u32>) {
        if self.state == 1 { output.push(61); }
        if self.state == 2 { output.extend_from_slice(&[61, u32::from(self.first)]); }
        self.state = 0;
    }
}

/// Legacy one-character lookahead needed to distinguish CRLF from a lone CR.
#[derive(Clone, Default)]
pub(super) struct Encoder { pending: Option<u32>, column: usize }

impl Encode for Encoder {
    /// Emits the previous character, normalizing lone CR while preserving CRLF as one line end.
    fn push(&mut self, code: u32, output: &mut Vec<u8>) {
        if let Some(previous) = self.pending.replace(code) {
            let previous = if previous == 13 && code != 10 { 10 } else { previous };
            emit(previous, &mut self.column, output);
        }
    }

    /// Flushes the final pending character using a synthetic lookahead NUL.
    fn finish(&mut self, output: &mut Vec<u8>) {
        self.push(0, output);
        self.pending = None;
        self.column = 0;
    }
}
