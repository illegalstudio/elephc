//! Purpose:
//! Implements the RFC 2045 base64 alphabet used by MIME `B` encoded-words.
//!
//! Called from:
//! - `crate::mime::encode` when emitting a `=?charset?B?...?=` word.
//! - `crate::mime::decode` when reading one back.
//!
//! Key details:
//! - Encoding always pads to a multiple of four characters, exactly like php-src.
//! - Decoding mirrors `php_base64_decode`'s lenient mode: unknown characters are skipped
//!   and a truncated final group contributes whatever whole bytes it carries.

/// Standard base64 alphabet in RFC 4648 order.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes `input` as padded base64.
pub fn encode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let packed = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(packed >> 18) as usize & 0x3f]);
        out.push(ALPHABET[(packed >> 12) as usize & 0x3f]);
        out.push(if chunk.len() > 1 {
            ALPHABET[(packed >> 6) as usize & 0x3f]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[packed as usize & 0x3f]
        } else {
            b'='
        });
    }
    out
}

/// Decodes lenient base64, ignoring separators and any character outside the alphabet.
pub fn decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    for byte in input {
        let Some(value) = decode_digit(*byte) else {
            continue;
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((accumulator >> bits) & 0xff) as u8);
        }
    }
    out
}

/// Maps one base64 character onto its six-bit value.
fn decode_digit(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
