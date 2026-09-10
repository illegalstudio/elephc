//! Purpose:
//! Implements mbstring's UTF, UCS, ASCII, and raw-byte codecs without platform libraries.
//!
//! Called from:
//! - The encoding dispatcher and byte-level PHP compatibility tests.
//!
//! Key details:
//! - Automatic UTF/UCS variants detect and consume a BOM; explicit endian variants do not.
//! - UCS codecs preserve surrogate/raw values that UTF decoders reject.
//! - UTF encoders preserve UCS surrogate values, matching libmbfl's conversion contract.

use super::{Decoded, Substitute, BAD_INPUT};

/// Encodings whose complete byte representation is implemented by the Unicode core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnicodeEncoding {
    Ascii,
    EightBit,
    Utf8,
    Utf16,
    Utf16Be,
    Utf16Le,
    Utf32,
    Utf32Be,
    Utf32Le,
    Ucs2,
    Ucs2Be,
    Ucs2Le,
    Ucs4,
    Ucs4Be,
    Ucs4Le,
}

impl UnicodeEncoding {
    /// Selects BOM-controlled byte order using PHP's state shared between MIME words.
    pub(super) fn stateful_input<'a>(self, input: &'a [u8], state: &mut u32) -> (Self, &'a [u8]) {
        use UnicodeEncoding::*;
        let (big, little, width) = match self {
            Utf16 => (Utf16Be, Utf16Le, 2), Utf32 => (Utf32Be, Utf32Le, 4),
            Ucs2 => (Ucs2Be, Ucs2Le, 2), Ucs4 => (Ucs4Be, Ucs4Le, 4),
            _ => return (self, input),
        };
        if *state == 1 { return (big, input); }
        if *state == 2 { return (little, input); }
        *state = 1;
        if input.len() >= width {
            let head = read_unit(&input[..width], false);
            if head == 0xfeff { return (big, &input[width..]); }
            if head == if width == 4 { 0xfffe0000 } else { 0xfffe } {
                *state = 2;
                return (little, &input[width..]);
            }
        }
        (big, input)
    }

    /// Resolves canonical names and aliases belonging to the built-in Unicode codecs.
    pub fn lookup(name: &[u8]) -> Option<Self> {
        super::Encoding::lookup(name).and_then(super::Encoding::unicode)
    }

    /// Decodes all bytes, recording each emitted character's starting byte offset.
    pub fn decode(self, input: &[u8]) -> Decoded {
        let mut output = Decoded::default();
        match self {
            Self::Ascii | Self::EightBit => {
                for (offset, &byte) in input.iter().enumerate() {
                    let code = if self == Self::Ascii && byte > 127 { BAD_INPUT } else { u32::from(byte) };
                    output.push(code, offset);
                }
            }
            Self::Utf8 => decode_utf8(input, &mut output),
            _ => self.decode_fixed(input, &mut output),
        }
        if matches!(self, Self::Utf16 | Self::Utf16Be | Self::Utf16Le) {
            output.case_batches = super::utf16_batches::ends(input, self == Self::Utf16Le, self == Self::Utf16, 64);
        }
        output
    }

    /// Counts characters with PHP's fast paths, including truncated UCS/UTF-32 units.
    pub fn strlen(self, input: &[u8]) -> usize {
        match self {
            Self::Ascii | Self::EightBit => input.len(),
            Self::Ucs2 | Self::Ucs2Be | Self::Ucs2Le => input.len() / 2,
            Self::Ucs4 | Self::Ucs4Be | Self::Ucs4Le
                | Self::Utf32 | Self::Utf32Be | Self::Utf32Le => input.len() / 4,
            _ => self.decode(input).points.len(),
        }
    }

    /// Encodes decoded units, applying the caller's replacement policy to rejected units.
    pub fn encode(self, points: &[u32], substitution: Substitute) -> Vec<u8> {
        let mut output = Vec::new();
        for &code in points {
            if !self.append(code, &mut output) {
                substitution.append(code, &mut output, |point, bytes| self.append(point, bytes));
            }
        }
        output
    }

    /// Decodes fixed-width units with BOM selection and optional UTF-16 surrogate pairing.
    fn decode_fixed(self, input: &[u8], output: &mut Decoded) {
        use UnicodeEncoding::*;
        let wide = matches!(self, Utf32 | Utf32Be | Utf32Le | Ucs4 | Ucs4Be | Ucs4Le);
        let mut little = matches!(self, Utf16Le | Utf32Le | Ucs2Le | Ucs4Le);
        let auto = matches!(self, Utf16 | Utf32 | Ucs2 | Ucs4);
        let unit = if wide { 4 } else { 2 };
        let utf16 = matches!(self, Utf16 | Utf16Be | Utf16Le);
        let utf32 = matches!(self, Utf32 | Utf32Be | Utf32Le);
        let mut offset = 0;
        if auto && input.len() >= unit {
            let head = read_unit(&input[..unit], false);
            if head == 0xfeff {
                offset = unit;
            } else if head == if wide { 0xfffe0000 } else { 0xfffe } {
                little = true;
                offset = unit;
            }
        }
        while offset + unit <= input.len() {
            let start = offset;
            let mut code = read_unit(&input[offset..offset + unit], little);
            offset += unit;
            if utf16 && (0xd800..=0xdbff).contains(&code) {
                if offset + unit <= input.len() {
                    let next = read_unit(&input[offset..offset + unit], little);
                    if (0xdc00..=0xdfff).contains(&next) {
                        code = 0x10000 + ((code - 0xd800) << 10) + next - 0xdc00;
                        offset += unit;
                    } else {
                        code = BAD_INPUT;
                    }
                } else {
                    code = BAD_INPUT;
                }
            } else if (utf16 && (0xdc00..=0xdfff).contains(&code))
                || (utf32 && ((0xd800..=0xdfff).contains(&code) || code > 0x10ffff))
            {
                code = BAD_INPUT;
            }
            output.push(code, start);
        }
        if offset < input.len() {
            output.push(BAD_INPUT, offset);
        }
    }

    /// Appends one representable codepoint, leaving output unchanged on failure.
    fn append(self, code: u32, output: &mut Vec<u8>) -> bool {
        use UnicodeEncoding::*;
        if code == BAD_INPUT {
            return false;
        }
        match self {
            Ascii | EightBit => {
                if code > if self == Ascii { 127 } else { 255 } {
                    return false;
                }
                output.push(code as u8);
            }
            Utf8 => return append_utf8(code, output),
            _ => {
                let wide = matches!(self, Utf32 | Utf32Be | Utf32Le | Ucs4 | Ucs4Be | Ucs4Le);
                let little = matches!(self, Utf16Le | Utf32Le | Ucs2Le | Ucs4Le);
                let ucs4 = matches!(self, Ucs4 | Ucs4Be | Ucs4Le);
                let ucs2 = matches!(self, Ucs2 | Ucs2Be | Ucs2Le);
                if !ucs4 && (code > 0x10ffff || (ucs2 && code > 0xffff)) {
                    return false;
                }
                if wide {
                    output.extend_from_slice(&if little { code.to_le_bytes() } else { code.to_be_bytes() });
                } else if code > 0xffff {
                    append_u16(0xd800 + ((code - 0x10000) >> 10) as u16, little, output);
                    append_u16(0xdc00 + ((code - 0x10000) & 0x3ff) as u16, little, output);
                } else {
                    append_u16(code as u16, little, output);
                }
            }
        }
        true
    }
}

/// Reads one two- or four-byte unit in the selected byte order.
fn read_unit(bytes: &[u8], little: bool) -> u32 {
    let mut value = 0;
    if little {
        for &byte in bytes.iter().rev() {
            value = (value << 8) | u32::from(byte);
        }
    } else {
        for &byte in bytes {
            value = (value << 8) | u32::from(byte);
        }
    }
    value
}

/// Appends one 16-bit unit without introducing an automatic BOM.
fn append_u16(code: u16, little: bool, output: &mut Vec<u8>) {
    output.extend_from_slice(&if little { code.to_le_bytes() } else { code.to_be_bytes() });
}

/// Decodes UTF-8 using the same maximal-valid-prefix boundaries as PHP.
fn decode_utf8(input: &[u8], output: &mut Decoded) {
    let mut offset = 0;
    while offset < input.len() {
        let (valid_len, invalid_len) = match std::str::from_utf8(&input[offset..]) {
            Ok(valid) => (valid.len(), 0),
            Err(error) => (error.valid_up_to(), error.error_len().unwrap_or(input.len() - offset - error.valid_up_to())),
        };
        let valid = std::str::from_utf8(&input[offset..offset + valid_len]).expect("validated prefix");
        for (start, code) in valid.char_indices() {
            output.push(code as u32, offset + start);
        }
        offset += valid_len;
        if invalid_len != 0 {
            output.push(BAD_INPUT, offset);
            offset += invalid_len;
        }
    }
}

/// Encodes a Unicode-range value, including a surrogate originating in UCS input.
pub(super) fn append_utf8(code: u32, output: &mut Vec<u8>) -> bool {
    if code > 0x10ffff {
        return false;
    }
    if code < 0x80 {
        output.push(code as u8);
    } else if code < 0x800 {
        output.extend_from_slice(&[(0xc0 | code >> 6) as u8, (0x80 | code & 0x3f) as u8]);
    } else if code < 0x10000 {
        output.extend_from_slice(&[(0xe0 | code >> 12) as u8,
            (0x80 | (code >> 6) & 0x3f) as u8, (0x80 | code & 0x3f) as u8]);
    } else {
        output.extend_from_slice(&[(0xf0 | code >> 18) as u8, (0x80 | (code >> 12) & 0x3f) as u8,
            (0x80 | (code >> 6) & 0x3f) as u8, (0x80 | code & 0x3f) as u8]);
    }
    true
}
