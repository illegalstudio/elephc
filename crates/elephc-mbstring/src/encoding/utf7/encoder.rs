//! Purpose:
//! Encodes PHP UTF-7 and UTF7-IMAP with persistent Base64 state across replacement output.
//!
//! Called from:
//! - `super::Utf7::encode`.
//!
//! Key details:
//! - UTF-7 can omit the closing dash before selected direct characters.
//! - UCS-originated surrogate values retain PHP's encoder behavior.

use crate::encoding::Substitute;

/// Writes a complete encoded string and closes any remaining modified Base64 section.
pub(super) fn encode(input: &[u32], imap: bool, substitute: Substitute) -> Vec<u8> {
    encode_prefix(input, imap, substitute, true)
}

/// Provides a provisional MIME line with or without its final Base64 padding and shift close.
pub(super) fn encode_prefix(input: &[u32], imap: bool, substitute: Substitute, finish: bool) -> Vec<u8> {
    let mut state = Encoder::new(imap, false);
    let mut output = Vec::new();
    for &code in input {
        if !state.push(code, &mut output) {
            substitute.append(code, &mut output, |code, output| state.push(code, output));
        }
    }
    if finish { state.close(true, &mut output); }
    output
}

/// Maintains residual bits and alphabet selection for one output stream.
#[derive(Clone, Copy)]
pub(super) struct Encoder {
    imap: bool,
    legacy: bool,
    active: bool,
    bits: u64,
    count: u8,
}

impl Encoder {
    /// Creates a direct-mode encoder, optionally retaining the streaming IMAP NUL exception.
    pub(super) fn new(imap: bool, legacy: bool) -> Self {
        Self { imap, legacy, active: false, bits: 0, count: 0 }
    }

    /// Emits one representable codepoint without modifying state when the value is rejected.
    pub(super) fn push(&mut self, code: u32, output: &mut Vec<u8>) -> bool {
        if code >= 0x110000 { return false; }
        if super::direct(code, self.imap) || (self.imap && self.legacy && code == 0) {
            self.close(self.imap || !super::implicit_end(code), output);
            output.push(code as u8);
            if self.imap && code == u32::from(b'&') { output.push(b'-'); }
        } else {
            if !self.active {
                output.push(if self.imap { b'&' } else { b'+' });
                self.active = true;
            }
            if code >= 0x10000 {
                let code = code - 0x10000;
                self.unit(0xd800 | (code >> 10), output);
                self.unit(0xdc00 | (code & 0x3ff), output);
            } else {
                self.unit(code, output);
            }
        }
        true
    }

    /// Adds one UTF-16 word and drains each complete six-bit alphabet digit.
    fn unit(&mut self, code: u32, output: &mut Vec<u8>) {
        self.bits = (self.bits << 16) | u64::from(code);
        self.count += 16;
        while self.count >= 6 {
            self.count -= 6;
            output.push(self.alphabet(((self.bits >> self.count) & 63) as usize));
        }
        self.bits &= (1 << self.count) - 1;
    }

    /// Pads residual bits and optionally emits a closing dash before returning to direct mode.
    pub(super) fn close(&mut self, dash: bool, output: &mut Vec<u8>) {
        if !self.active { return; }
        if self.count != 0 { output.push(self.alphabet((self.bits << (6 - self.count)) as usize)); }
        if dash { output.push(b'-'); }
        self.active = false;
        self.bits = 0;
        self.count = 0;
    }

    /// Selects the UTF-7 slash or IMAP comma for the final alphabet position.
    fn alphabet(&self, digit: usize) -> u8 {
        if self.imap && digit == 63 { b',' }
        else { b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"[digit] }
    }
}
