//! Purpose:
//! Implements the RFC 2047 `Q` encoding used by MIME encoded-words.
//!
//! Called from:
//! - `crate::mime::encode` when emitting a `=?charset?Q?...?=` word.
//! - `crate::mime::decode` when reading one back.
//!
//! Key details:
//! - php-src's `qp_table` charges one output byte to characters it can pass through and
//!   three to everything else; `cost` is that table and drives the encoder's line fitting.
//! - Q-encoding differs from plain quoted-printable in that `_` decodes to a space.
//! - Hex digits are emitted uppercase, matching php-src's `qp_digits`.

/// Uppercase hex digits php-src emits for escaped bytes.
const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// Returns how many output bytes `byte` costs in a `Q` encoded-word.
///
/// Everything outside the RFC 2047 "printable, not special" set costs three bytes
/// (`=XX`); the rest passes through unchanged for one.
pub fn cost(byte: u8) -> usize {
    if is_literal(byte) {
        1
    } else {
        3
    }
}

/// Reports whether `byte` may appear literally inside a `Q` encoded-word.
fn is_literal(byte: u8) -> bool {
    match byte {
        b'=' | b'?' | b'_' => false,
        0x21..=0x3c | 0x3e | 0x40..=0x5e | 0x60..=0x7e => true,
        _ => false,
    }
}

/// Encodes `input` as an RFC 2047 `Q` payload.
pub fn encode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    for byte in input {
        if is_literal(*byte) {
            out.push(*byte);
        } else {
            out.push(b'=');
            out.push(HEX[usize::from(*byte >> 4)]);
            out.push(HEX[usize::from(*byte & 0x0f)]);
        }
    }
    out
}

/// Decodes an RFC 2047 `Q` payload back into raw bytes.
///
/// `_` becomes a space and `=XX` becomes the escaped byte; a malformed escape is kept
/// literally, which is what php-src's decoder does with unparsable input.
pub fn decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut index = 0usize;
    while index < input.len() {
        match input[index] {
            b'_' => {
                out.push(b' ');
                index += 1;
            }
            b'=' if index + 2 < input.len() => {
                match (hex_value(input[index + 1]), hex_value(input[index + 2])) {
                    (Some(high), Some(low)) => {
                        out.push((high << 4) | low);
                        index += 3;
                    }
                    _ => {
                        out.push(b'=');
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    out
}

/// Maps one ASCII hex digit onto its value.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
