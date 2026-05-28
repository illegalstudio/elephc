//! Purpose:
//! Converts parser string-literal payloads into PHP runtime bytes.
//! Preserves byte escapes that cannot be represented directly inside Rust UTF-8 strings.
//!
//! Called from:
//! - `crate::lexer::literals::strings` when decoding `\xNN` and octal escapes.
//! - `crate::codegen` and optimizer paths that need PHP byte lengths or data.
//!
//! Key details:
//! - PHP strings are byte strings, while Rust `String` must be valid UTF-8.
//! - Escaped bytes above ASCII are stored in a private-use marker range until codegen.
//! - Source text in the marker range is encoded as UTF-8 bytes to avoid marker collisions.

const ESCAPED_BYTE_BASE: u32 = 0xe000;
const ESCAPED_BYTE_END: u32 = ESCAPED_BYTE_BASE + 0xff;

/// Appends the literal representation of `byte` to `out`.
///
/// ASCII bytes are emitted as their char equivalent. Non-ASCII bytes are encoded
/// as a private-use char in the range U+E000–U+E0FF, preserving the byte value
/// through the PHP compilation pipeline where Rust strings must be valid UTF-8.
///
/// # Parameters
/// - `byte`: The raw byte to emit.
/// - `out`: The growing literal string buffer.
pub(crate) fn push_escaped_byte(byte: u8, out: &mut String) {
    if byte.is_ascii() {
        out.push(byte as char);
    } else {
        out.push(char::from_u32(ESCAPED_BYTE_BASE + u32::from(byte)).unwrap());
    }
}

/// Appends the literal representation of `ch` to `out`.
///
/// If `ch` is in the private-use marker range U+E000–U+E0FF it is UTF-8 encoded
/// and each resulting byte is passed through `push_escaped_byte` to avoid
/// colliding with the encoding scheme. Otherwise `ch` is pushed directly.
///
/// # Parameters
/// - `ch`: The character to emit.
/// - `out`: The growing literal string buffer.
pub(crate) fn push_literal_char(ch: char, out: &mut String) {
    let codepoint = ch as u32;
    if (ESCAPED_BYTE_BASE..=ESCAPED_BYTE_END).contains(&codepoint) {
        let mut encoded = [0; 4];
        for byte in ch.encode_utf8(&mut encoded).bytes() {
            push_escaped_byte(byte, out);
        }
    } else {
        out.push(ch);
    }
}

/// Decodes a literal string into its raw PHP byte representation.
///
/// Private-use marker chars (U+E000–U+E0FF) are decoded back to single bytes;
/// all other chars are UTF-8 decoded and emitted as-is. The result has the same
/// length in bytes as the original PHP source literal.
///
/// # Parameters
/// - `value`: A Rust string that may contain private-use marker chars.
///
/// # Returns
/// A `Vec<u8>` containing the raw bytes of the PHP literal.
pub(crate) fn literal_bytes(value: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(value.len());
    for ch in value.chars() {
        let codepoint = ch as u32;
        if (ESCAPED_BYTE_BASE..=ESCAPED_BYTE_END).contains(&codepoint) {
            bytes.push((codepoint - ESCAPED_BYTE_BASE) as u8);
        } else {
            let mut encoded = [0; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        }
    }
    bytes
}

/// Computes the PHP byte length of a literal string.
///
/// Each private-use marker char (U+E000–U+E0FF) counts as 1 byte; all other
/// chars count as their UTF-8 encoded length. This is equivalent to
/// `literal_bytes(value).len()` but avoids allocating.
///
/// # Parameters
/// - `value`: A Rust string that may contain private-use marker chars.
///
/// # Returns
/// The number of bytes the PHP literal would occupy.
pub(crate) fn literal_byte_len(value: &str) -> usize {
    value
        .chars()
        .map(|ch| {
            let codepoint = ch as u32;
            if (ESCAPED_BYTE_BASE..=ESCAPED_BYTE_END).contains(&codepoint) {
                1
            } else {
                ch.len_utf8()
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{literal_byte_len, literal_bytes, push_escaped_byte, push_literal_char};

    /// Verifies high escaped byte round trips to single PHP byte.
    #[test]
    fn test_high_escaped_byte_round_trips_to_single_php_byte() {
        let mut value = String::new();
        push_escaped_byte(0xff, &mut value);

        assert_eq!(literal_byte_len(&value), 1);
        assert_eq!(literal_bytes(&value), vec![0xff]);
    }

    /// Verifies unicode source text stays UTF-8.
    #[test]
    fn test_unicode_source_text_stays_utf8() {
        assert_eq!(literal_byte_len("😀"), 4);
        assert_eq!(literal_bytes("é"), vec![0xc3, 0xa9]);
    }

    /// Verifies private use source text does not collide with byte markers.
    #[test]
    fn test_private_use_source_text_does_not_collide_with_byte_markers() {
        let mut value = String::new();
        push_literal_char('\u{e000}', &mut value);

        assert_eq!(literal_byte_len(&value), 3);
        assert_eq!(literal_bytes(&value), vec![0xee, 0x80, 0x80]);
    }
}
