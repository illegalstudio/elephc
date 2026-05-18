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

pub(crate) fn push_escaped_byte(byte: u8, out: &mut String) {
    if byte.is_ascii() {
        out.push(byte as char);
    } else {
        out.push(char::from_u32(ESCAPED_BYTE_BASE + u32::from(byte)).unwrap());
    }
}

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

    #[test]
    fn test_high_escaped_byte_round_trips_to_single_php_byte() {
        let mut value = String::new();
        push_escaped_byte(0xff, &mut value);

        assert_eq!(literal_byte_len(&value), 1);
        assert_eq!(literal_bytes(&value), vec![0xff]);
    }

    #[test]
    fn test_unicode_source_text_stays_utf8() {
        assert_eq!(literal_byte_len("😀"), 4);
        assert_eq!(literal_bytes("é"), vec![0xc3, 0xa9]);
    }

    #[test]
    fn test_private_use_source_text_does_not_collide_with_byte_markers() {
        let mut value = String::new();
        push_literal_char('\u{e000}', &mut value);

        assert_eq!(literal_byte_len(&value), 3);
        assert_eq!(literal_bytes(&value), vec![0xee, 0x80, 0x80]);
    }
}
