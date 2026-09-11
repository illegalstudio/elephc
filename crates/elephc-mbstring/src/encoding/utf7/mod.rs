//! Purpose:
//! Shares modified Base64 and UTF-16 rules for UTF-7 and UTF7-IMAP.
//!
//! Called from:
//! - The canonical mbstring encoding catalog.
//!
//! Key details:
//! - The two variants have distinct alphabets, direct characters, and termination rules.
//! - PHP's streaming cut filter is separate from its modern conversion decoder.

mod decoder;
mod encoder;
mod cut;

use super::{Decoded, Substitute};

/// PHP's UTF-7 variant, including the IMAP modified alphabet and direct-character rules.
#[derive(Clone, Copy, Debug)]
pub(super) struct Utf7(pub bool);

impl Utf7 {
    /// Stops at PHP's Base64 buffer boundary while retaining the pending surrogate and mode.
    pub fn decode_next(self, input: &[u8], capacity: usize, state: &mut u32) -> (Decoded, usize) {
        decoder::decode_next(input, self.0, capacity, state)
    }

    /// Decodes with explicit scratch-buffer boundaries for a conversion consumer.
    pub fn decode_buffer(self, input: &[u8], capacity: usize) -> Decoded {
        decoder::decode_buffer(input, self.0, capacity)
    }

    /// Decodes one complete string with PHP's malformed-sequence and validation rules.
    pub fn decode(self, input: &[u8]) -> Decoded {
        decoder::decode(input, self.0)
    }

    /// Encodes UTF-16 units in contiguous modified Base64 runs, preserving substitution state.
    pub fn encode(self, input: &[u32], substitute: Substitute) -> Vec<u8> {
        encoder::encode(input, self.0, substitute)
    }

    /// Encodes a candidate MIME word while controlling whether pending bits are flushed.
    pub fn encode_prefix(self, input: &[u32], substitute: Substitute, finish: bool) -> Vec<u8> {
        encoder::encode_prefix(input, self.0, substitute, finish)
    }

    /// Cuts a byte budget through PHP's legacy streaming filters and default replacement.
    pub fn cut(self, input: &[u8], from: usize, length: usize) -> Vec<u8> {
        cut::cut(input, from, length, self.0)
    }
}

/// Returns a modified Base64 digit without interpreting shift or termination characters.
fn digit(byte: u8, imap: bool) -> Option<u32> {
    Some(match byte {
        b'A'..=b'Z' => u32::from(byte - b'A'),
        b'a'..=b'z' => u32::from(byte - b'a') + 26,
        b'0'..=b'9' => u32::from(byte - b'0') + 52,
        b'+' => 62,
        byte if byte == if imap { b',' } else { b'/' } => 63,
        _ => return None,
    })
}

/// Reports direct characters that can end an ordinary UTF-7 Base64 run without a dash.
fn implicit_end(code: u32) -> bool {
    code < 128 && b" \t\r\n'(),.:?".contains(&(code as u8))
}

/// Reports the canonical direct-encoding set used by the selected encoder.
fn direct(code: u32, imap: bool) -> bool {
    if imap { (0x20..=0x7e).contains(&code) }
    else { code < 128 && ((code as u8).is_ascii_alphanumeric() || matches!(code, 0 | 0x2f | 0x2d) || implicit_end(code)) }
}

/// Reports ASCII punctuation permitted directly by UTF-7 validation but encoded canonically.
fn optional_direct(byte: u8) -> bool {
    b"!\"#$%&*;<=>@[]^_`{|}".contains(&byte)
}
