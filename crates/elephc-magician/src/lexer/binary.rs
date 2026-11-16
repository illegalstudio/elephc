//! Purpose:
//! Rewrites PHP source bytes that are not valid UTF-8 into private-use markers the scanner
//! can carry through unchanged. PHP source is a byte stream and PHP strings are byte
//! sequences, while this lexer consumes Rust `str`; the markers are the bridge.
//!
//! Called from:
//! - `crate::parser::parse_fragment()` before tokenizing an eval fragment.
//!
//! Key details:
//! - One marker per byte, so the original bytes are recoverable exactly.
//! - Markers are substituted everywhere, not only inside quotes: PHP identifiers accept
//!   every byte from `0x80` up, so a high byte outside a literal is not a source error.
//! - Valid UTF-8 source is returned untouched, which is the overwhelmingly common case and
//!   keeps identifiers and literals spelled as themselves.

/// The first code point of the private-use block reserved for one raw source byte.
///
/// `crate::parser` decodes this same range back into bytes when it builds a string constant,
/// so the two ends of the substitution must agree on where the block starts.
pub(crate) const BINARY_MARKER_BASE: u32 = 0xF_0000;

/// Rewrites every non-UTF-8 byte of PHP source to its private-use marker.
///
/// A high byte is legal PHP wherever it appears: inside a literal it is a string byte, and
/// outside one it is an identifier character. `php -n` 8.5.6 accepts
/// `class \xA9 { public const V = 5; } echo \xA9::V;` and prints `5`, so refusing a high byte
/// outside quotes rejected a program PHP runs. Marking it instead lets `is_ident_start()`
/// admit it and lets the string-constant builder restore the byte it stood for.
///
/// The substitution runs over the whole source only when the source is not valid UTF-8, and
/// then it applies to every high byte, including bytes that formed a valid UTF-8 sequence
/// elsewhere in the file. That is deliberate: the file has no single encoding to trust, and
/// per-byte markers round-trip to the same bytes either way.
pub(crate) fn normalize_binary_source(code: &[u8]) -> String {
    if let Ok(source) = std::str::from_utf8(code) {
        // A genuine character of the marker block itself (`U+F0041` in a valid UTF-8 file) would
        // be read back as the single byte its code point names, so those characters are spelled
        // as the markers of their own UTF-8 bytes, which decode to exactly those bytes.
        if !source.chars().any(is_marker_char) {
            return source.to_owned();
        }
        let mut output = String::with_capacity(source.len());
        for ch in source.chars() {
            if is_marker_char(ch) {
                let mut encoded = [0; 4];
                for byte in ch.encode_utf8(&mut encoded).bytes() {
                    push_marker(byte, &mut output);
                }
            } else {
                output.push(ch);
            }
        }
        return output;
    }
    let mut output = String::with_capacity(code.len());
    for &byte in code {
        if byte >= 0x80 {
            push_marker(byte, &mut output);
        } else {
            output.push(byte as char);
        }
    }
    output
}

/// Appends the marker character that stands for one raw source byte at or above `0x80`.
fn push_marker(byte: u8, output: &mut String) {
    output.push(
        char::from_u32(BINARY_MARKER_BASE + u32::from(byte))
            .expect("the private use plane holds every marker code point"),
    );
}

/// Reports whether `ch` lies in the marker block, whatever the source meant by it.
pub(crate) fn is_marker_char(ch: char) -> bool {
    (BINARY_MARKER_BASE..=BINARY_MARKER_BASE + 0xFF).contains(&(ch as u32))
}

#[cfg(test)]
mod tests {
    use super::{normalize_binary_source, BINARY_MARKER_BASE};

    /// Verifies valid UTF-8 source is handed to the scanner unchanged.
    #[test]
    fn valid_utf8_source_is_returned_verbatim() {
        assert_eq!(normalize_binary_source("class Café {}".as_bytes()), "class Café {}");
    }

    /// Verifies a real marker-block character in valid UTF-8 source keeps its own UTF-8 bytes.
    ///
    /// `"\u{F0041}"` is the four bytes `f3b08181` in php; read as the marker for `0x41` it was
    /// the single byte `A`.
    #[test]
    fn a_real_marker_block_character_becomes_the_markers_of_its_utf8_bytes() {
        let normalized = normalize_binary_source("echo \"\u{F0041}\";".as_bytes());
        let restored: Vec<u8> = normalized
            .chars()
            .filter(|ch| (BINARY_MARKER_BASE..=BINARY_MARKER_BASE + 0xFF).contains(&(*ch as u32)))
            .map(|ch| u8::try_from(ch as u32 - BINARY_MARKER_BASE).expect("marker holds one byte"))
            .collect();
        assert_eq!(restored, "\u{F0041}".as_bytes());
    }

    /// Verifies a high byte outside any quote survives as a marker instead of failing.
    ///
    /// This is the shape `php -n` 8.5.6 accepts in `class \xA9 { public const V = 5; }`;
    /// before the markers reached identifier position it was refused as invalid UTF-8.
    #[test]
    fn high_byte_in_identifier_position_becomes_a_marker() {
        let normalized = normalize_binary_source(b"class \xA9 {}");
        let expected_marker =
            char::from_u32(BINARY_MARKER_BASE + 0xA9).expect("marker is a valid code point");
        assert_eq!(
            normalized,
            format!("class {expected_marker} {{}}"),
        );
    }

    /// Verifies a high byte inside a literal still becomes the marker the parser decodes.
    #[test]
    fn high_byte_inside_a_literal_keeps_its_marker() {
        let normalized = normalize_binary_source(b"echo \"\xFF\";");
        let expected_marker =
            char::from_u32(BINARY_MARKER_BASE + 0xFF).expect("marker is a valid code point");
        assert_eq!(normalized, format!("echo \"{expected_marker}\";"));
    }

    /// Verifies every marker maps back to the byte it replaced, for all 128 high bytes.
    #[test]
    fn every_high_byte_round_trips_through_its_marker() {
        let source: Vec<u8> = (0x80..=0xFF).collect();
        let normalized = normalize_binary_source(&source);
        let restored: Vec<u8> = normalized
            .chars()
            .map(|ch| u8::try_from(ch as u32 - BINARY_MARKER_BASE).expect("marker holds one byte"))
            .collect();
        assert_eq!(restored, source);
    }
}
