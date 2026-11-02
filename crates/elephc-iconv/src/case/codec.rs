//! Purpose:
//! Decodes a PHP byte string into mbstring "wchar" code points and encodes them back, for
//! every encoding PHP's case-mapping builtins can be asked to work in.
//!
//! Called from:
//! - `crate::case::mapping`, which case-maps the decoded code points chunk by chunk.
//!
//! Key details:
//! - UTF-8, the single-byte encodings, and the UTF-16 / UTF-32 / UCS-2 / UCS-4 families
//!   are ports of libmbfl's `to_wchar` / `from_wchar` filters, including how many code
//!   points one call produces: mbstring's final-sigma rule looks across those chunk
//!   boundaries differently from inside a chunk, so the boundaries are part of the result.
//! - An undecodable sequence becomes `BAD_INPUT`, and every code point an encoder cannot
//!   represent is replaced by `?` encoded in the same encoding, which is mbstring's default
//!   substitution (`mb_substitute_character()` is not available in elephc).
//! - The supported CJK encodings go through libc `iconv` into `UCS-4LE` and back. Their
//!   valid text matches mbstring (checked differentially), but a byte libc rejects always
//!   becomes one `?`, where libmbfl sometimes maps it (CP936 `0xFF`) or groups it
//!   differently; `crate::case::encoding` keeps every encoding this model gets wrong on
//!   valid text unsupported.

use crate::error::IconvError;
use crate::ffi::Converter;

/// libmbfl's `MBFL_BAD_INPUT` error marker for an undecodable input sequence.
pub(super) const BAD_INPUT: u32 = 0xFFFF_FFFE;

/// Code points one `to_wchar` call may produce (`php_unicode_convert_case`'s buffer).
pub(super) const CHUNK: usize = 64;

/// Wide charset the iconv fallback decodes into and encodes from.
const ICONV_WIDE: &[u8] = b"UCS-4LE";

/// The byte <-> code point tables of one single-byte encoding.
pub(super) struct SingleByteTable {
    /// Code point of each byte, or `tables::BAD` for a byte the encoding leaves undefined.
    pub(super) decode: [u16; 256],
    /// Every BMP code point the encoder accepts and the byte it becomes, sorted by code point.
    pub(super) encode: &'static [(u16, u8)],
}

/// Byte order of a UTF-16 / UTF-32 / UCS-2 / UCS-4 encoding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ByteOrder {
    /// Big-endian (`UTF-16BE`, ...).
    Big,
    /// Little-endian (`UTF-16LE`, ...).
    Little,
    /// The unsuffixed name: a leading BOM selects the order, big-endian otherwise.
    Detect,
}

/// One encoding's decoder/encoder pair.
#[derive(Clone, Copy)]
pub(super) enum Codec {
    /// `UTF-8`.
    Utf8,
    /// A table-driven single-byte encoding (`ASCII`, `ISO-8859-*`, `Windows-125*`, ...).
    SingleByte(&'static SingleByteTable),
    /// `UTF-16`, `UTF-16BE`, `UTF-16LE`.
    Utf16(ByteOrder),
    /// `UTF-32`, `UTF-32BE`, `UTF-32LE`.
    Utf32(ByteOrder),
    /// `UCS-2`, `UCS-2BE`, `UCS-2LE`.
    Ucs2(ByteOrder),
    /// `UCS-4`, `UCS-4BE`, `UCS-4LE`.
    Ucs4(ByteOrder),
    /// A multi-byte encoding reached through libc `iconv` under the given charset name.
    Iconv(&'static str),
}

/// A decoded input: every code point plus where each `to_wchar` call ended.
pub(super) struct Decoded {
    /// Decoded code points, `BAD_INPUT` marking undecodable sequences.
    pub(super) wchars: Vec<u32>,
    /// Exclusive end index into `wchars` of each chunk, in order.
    pub(super) chunk_ends: Vec<usize>,
}

impl Codec {
    /// Decodes `input` completely, recording libmbfl's chunk boundaries.
    ///
    /// Fails only when libc `iconv` cannot open the fallback charset.
    pub(super) fn decode(self, input: &[u8]) -> Result<Decoded, IconvError> {
        let mut decoded = Decoded {
            wchars: Vec::with_capacity(input.len()),
            chunk_ends: Vec::with_capacity(input.len() / CHUNK + 1),
        };
        if let Codec::Iconv(charset) = self {
            decode_iconv(charset, input, &mut decoded.wchars)?;
            let total = decoded.wchars.len();
            let mut end = 0;
            while end < total {
                end = (end + CHUNK).min(total);
                decoded.chunk_ends.push(end);
            }
            return Ok(decoded);
        }
        let mut pos = 0;
        let mut order = match self {
            Codec::Utf16(order) | Codec::Utf32(order) | Codec::Ucs2(order) | Codec::Ucs4(order) => {
                order
            }
            _ => ByteOrder::Big,
        };
        while pos < input.len() {
            let out = &mut decoded.wchars;
            match self {
                Codec::Utf8 => utf8_chunk(input, &mut pos, out),
                Codec::SingleByte(table) => single_byte_chunk(table, input, &mut pos, out),
                Codec::Utf16(_) => {
                    detect_order(input, &mut pos, &mut order, 2);
                    utf16_chunk(input, &mut pos, order == ByteOrder::Little, out);
                }
                Codec::Utf32(_) => {
                    detect_order(input, &mut pos, &mut order, 4);
                    utf32_chunk(input, &mut pos, order == ByteOrder::Little, out, true);
                }
                Codec::Ucs2(_) => {
                    detect_order(input, &mut pos, &mut order, 2);
                    ucs2_chunk(input, &mut pos, order == ByteOrder::Little, out);
                }
                Codec::Ucs4(_) => {
                    detect_order(input, &mut pos, &mut order, 4);
                    utf32_chunk(input, &mut pos, order == ByteOrder::Little, out, false);
                }
                Codec::Iconv(_) => unreachable!("iconv input is decoded in one pass"),
            }
            decoded.chunk_ends.push(decoded.wchars.len());
        }
        Ok(decoded)
    }

    /// Opens the encoder that writes code points back into this encoding.
    pub(super) fn encoder(self) -> Result<Encoder, IconvError> {
        Ok(match self {
            Codec::Utf8 => Encoder::Utf8,
            Codec::SingleByte(table) => Encoder::SingleByte(table),
            // The unsuffixed names all write big-endian, without a BOM.
            Codec::Utf16(order) => Encoder::Utf16(order == ByteOrder::Little),
            Codec::Utf32(order) => Encoder::Utf32(order == ByteOrder::Little),
            Codec::Ucs2(order) => Encoder::Ucs2(order == ByteOrder::Little),
            Codec::Ucs4(order) => Encoder::Ucs4(order == ByteOrder::Little),
            Codec::Iconv(charset) => {
                Encoder::Iconv(Converter::open(ICONV_WIDE, charset.as_bytes())?)
            }
        })
    }
}

/// Consumes a leading BOM for an unsuffixed UTF-16 / UTF-32 / UCS-2 / UCS-4 name.
///
/// Mirrors libmbfl's detecting `to_wchar` wrappers: the order is decided on the first
/// call only, a little-endian BOM switches to little-endian, a big-endian BOM is dropped,
/// and anything else (including input shorter than one unit) means big-endian.
fn detect_order(input: &[u8], pos: &mut usize, order: &mut ByteOrder, width: usize) {
    if *order != ByteOrder::Detect {
        return;
    }
    *order = ByteOrder::Big;
    if input.len() - *pos < width {
        return;
    }
    let unit = read_be(&input[*pos..*pos + width]);
    let (little_bom, big_bom) = if width == 2 { (0xFFFE, 0xFEFF) } else { (0xFFFE_0000, 0xFEFF) };
    if unit == little_bom {
        *pos += width;
        *order = ByteOrder::Little;
    } else if unit == big_bom {
        *pos += width;
    }
}

/// Reads one big-endian unit of 2 or 4 bytes.
fn read_be(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |acc, &byte| (acc << 8) | u32::from(byte))
}

/// Reads one unit of 2 or 4 bytes in the requested byte order.
fn read_unit(bytes: &[u8], little: bool) -> u32 {
    if little {
        bytes.iter().rev().fold(0u32, |acc, &byte| (acc << 8) | u32::from(byte))
    } else {
        read_be(bytes)
    }
}

/// Ports libmbfl's `mb_utf8_to_wchar`: one call decodes at most `CHUNK` code points.
///
/// Each maximal invalid subpart becomes one `BAD_INPUT`: a lead byte followed by a byte
/// that cannot continue it stands alone, and a valid prefix cut off by the end of the
/// input counts once.
fn utf8_chunk(input: &[u8], pos: &mut usize, out: &mut Vec<u32>) {
    let limit = out.len() + CHUNK;
    let end = input.len();
    let mut p = *pos;
    while p < end && out.len() < limit {
        let c = input[p];
        p += 1;
        if c < 0x80 {
            out.push(u32::from(c));
        } else if c < 0xC2 {
            out.push(BAD_INPUT);
        } else if c <= 0xDF {
            if p < end {
                let c2 = input[p];
                p += 1;
                if c2 & 0xC0 != 0x80 {
                    out.push(BAD_INPUT);
                    p -= 1;
                } else {
                    out.push((u32::from(c & 0x1F) << 6) | u32::from(c2 & 0x3F));
                }
            } else {
                out.push(BAD_INPUT);
            }
        } else if c <= 0xEF {
            if end - p >= 2 {
                let c2 = input[p];
                let c3 = input[p + 1];
                p += 2;
                if c2 & 0xC0 != 0x80 || (c == 0xE0 && c2 < 0xA0) || (c == 0xED && c2 >= 0xA0) {
                    out.push(BAD_INPUT);
                    p -= 2;
                } else if c3 & 0xC0 != 0x80 {
                    out.push(BAD_INPUT);
                    p -= 1;
                } else {
                    out.push(
                        (u32::from(c & 0x0F) << 12)
                            | (u32::from(c2 & 0x3F) << 6)
                            | u32::from(c3 & 0x3F),
                    );
                }
            } else {
                out.push(BAD_INPUT);
                if p < end
                    && (c != 0xE0 || input[p] >= 0xA0)
                    && (c != 0xED || input[p] < 0xA0)
                    && input[p] & 0xC0 == 0x80
                {
                    p += 1;
                    if p < end && input[p] & 0xC0 == 0x80 {
                        p += 1;
                    }
                }
            }
        } else if c <= 0xF4 {
            if end - p >= 3 {
                let c2 = input[p];
                let c3 = input[p + 1];
                let c4 = input[p + 2];
                p += 3;
                if c2 & 0xC0 != 0x80 || (c == 0xF0 && c2 < 0x90) || (c == 0xF4 && c2 >= 0x90) {
                    out.push(BAD_INPUT);
                    p -= 3;
                } else if c3 & 0xC0 != 0x80 {
                    out.push(BAD_INPUT);
                    p -= 2;
                } else if c4 & 0xC0 != 0x80 {
                    out.push(BAD_INPUT);
                    p -= 1;
                } else {
                    out.push(
                        (u32::from(c & 0x07) << 18)
                            | (u32::from(c2 & 0x3F) << 12)
                            | (u32::from(c3 & 0x3F) << 6)
                            | u32::from(c4 & 0x3F),
                    );
                }
            } else {
                out.push(BAD_INPUT);
                if p < end {
                    let c2 = input[p];
                    if (c == 0xF0 && c2 >= 0x90) || (c == 0xF4 && c2 < 0x90) || (0xF1..=0xF3).contains(&c) {
                        while p < end && input[p] & 0xC0 == 0x80 {
                            p += 1;
                        }
                    }
                }
            }
        } else {
            out.push(BAD_INPUT);
        }
    }
    *pos = p;
}

/// Ports the single-byte `to_wchar` filters: one byte is one code point.
fn single_byte_chunk(table: &SingleByteTable, input: &[u8], pos: &mut usize, out: &mut Vec<u32>) {
    let end = (*pos + CHUNK).min(input.len());
    for &byte in &input[*pos..end] {
        let code = table.decode[usize::from(byte)];
        out.push(if code == super::tables::BAD { BAD_INPUT } else { u32::from(code) });
    }
    *pos = end;
}

/// Ports libmbfl's `mb_utf16{be,le}_to_wchar_default`.
///
/// The loop stops one slot early because a lone high surrogate followed by an ordinary
/// unit produces two code points in one step; an odd trailing byte is one `BAD_INPUT`.
fn utf16_chunk(input: &[u8], pos: &mut usize, little: bool, out: &mut Vec<u32>) {
    let limit = out.len() + CHUNK - 1;
    let in_len = input.len() - *pos;
    let end = *pos + (in_len & !1);
    let mut p = *pos;
    while p < end && out.len() < limit {
        let n = read_unit(&input[p..p + 2], little);
        p += 2;
        if (0xD800..=0xDBFF).contains(&n) {
            if p < end {
                let n2 = read_unit(&input[p..p + 2], little);
                p += 2;
                if (0xD800..=0xDBFF).contains(&n2) {
                    out.push(BAD_INPUT);
                    p -= 2;
                } else if (0xDC00..=0xDFFF).contains(&n2) {
                    out.push((((n & 0x3FF) << 10) | (n2 & 0x3FF)) + 0x10000);
                } else {
                    out.push(BAD_INPUT);
                    out.push(n2);
                }
            } else {
                out.push(BAD_INPUT);
            }
        } else if (0xDC00..=0xDFFF).contains(&n) {
            out.push(BAD_INPUT);
        } else {
            out.push(n);
        }
    }
    if p == end && in_len & 1 == 1 && out.len() < limit {
        out.push(BAD_INPUT);
        p += 1;
    }
    *pos = p;
}

/// Ports libmbfl's `mb_ucs2{be,le}_to_wchar`: every unit is taken as-is, surrogates included.
fn ucs2_chunk(input: &[u8], pos: &mut usize, little: bool, out: &mut Vec<u32>) {
    let limit = out.len() + CHUNK;
    let in_len = input.len() - *pos;
    let end = *pos + (in_len & !1);
    let mut p = *pos;
    while p < end && out.len() < limit {
        out.push(read_unit(&input[p..p + 2], little));
        p += 2;
    }
    if p == end && in_len & 1 == 1 && out.len() < limit {
        out.push(BAD_INPUT);
        p += 1;
    }
    *pos = p;
}

/// Ports libmbfl's `mb_utf32{be,le}_to_wchar` and `mb_ucs4{be,le}_to_wchar`.
///
/// UTF-32 rejects surrogates and values past U+10FFFF; UCS-4 keeps any 32-bit value. One to
/// three trailing bytes become a single `BAD_INPUT`.
fn utf32_chunk(input: &[u8], pos: &mut usize, little: bool, out: &mut Vec<u32>, validate: bool) {
    let limit = out.len() + CHUNK;
    let start = *pos;
    let in_len = input.len() - start;
    let end = start + (in_len & !3);
    let mut p = start;
    while p < end && out.len() < limit {
        let w = read_unit(&input[p..p + 4], little);
        p += 4;
        if !validate || (w < 0x11_0000 && !(0xD800..=0xDFFF).contains(&w)) {
            out.push(w);
        } else {
            out.push(BAD_INPUT);
        }
    }
    if p == end && in_len & 3 != 0 && out.len() < limit {
        out.push(BAD_INPUT);
        p = start + in_len;
    }
    *pos = p;
}

/// Decodes a whole input through libc `iconv` into code points.
///
/// A rejected byte becomes one `BAD_INPUT` and decoding resumes after it; a truncated
/// final sequence becomes one `BAD_INPUT`.
fn decode_iconv(charset: &str, input: &[u8], out: &mut Vec<u32>) -> Result<(), IconvError> {
    let mut converter = Converter::open(charset.as_bytes(), ICONV_WIDE)?;
    let mut cursor = input;
    let mut buffer = [0u8; 1024];
    while !cursor.is_empty() {
        let before = cursor.len();
        let (produced, failure) = converter.step(&mut cursor, &mut buffer, true);
        push_wide(&buffer[..produced], out);
        match failure {
            Some(IconvError::IllegalSequence) => {
                out.push(BAD_INPUT);
                cursor = &cursor[1.min(cursor.len())..];
                converter.reset();
            }
            Some(_) => {
                out.push(BAD_INPUT);
                break;
            }
            None if produced == 0 && cursor.len() == before => break,
            None => {}
        }
    }
    let (produced, _) = converter.step(&mut cursor, &mut buffer, false);
    push_wide(&buffer[..produced], out);
    Ok(())
}

/// Appends the code points of a `UCS-4LE` buffer.
fn push_wide(bytes: &[u8], out: &mut Vec<u32>) {
    for unit in bytes.chunks_exact(4) {
        out.push(read_unit(unit, true));
    }
}

/// Writes code points back into one encoding, substituting `?` for what it cannot hold.
pub(super) enum Encoder {
    /// `UTF-8`.
    Utf8,
    /// A table-driven single-byte encoding.
    SingleByte(&'static SingleByteTable),
    /// UTF-16, little-endian when `true`.
    Utf16(bool),
    /// UTF-32, little-endian when `true`.
    Utf32(bool),
    /// UCS-2, little-endian when `true`.
    Ucs2(bool),
    /// UCS-4, little-endian when `true`.
    Ucs4(bool),
    /// A libc `iconv` descriptor from `UCS-4LE` into the target charset.
    Iconv(Converter),
}

impl Encoder {
    /// Encodes one chunk of case-mapped code points.
    pub(super) fn encode(&mut self, wchars: &[u32], out: &mut Vec<u8>) {
        for &w in wchars {
            if !self.put(w, out) {
                // mbstring's default substitution: `?`, through the same encoder.
                self.put(u32::from(b'?'), out);
            }
        }
    }

    /// Emits whatever a stateful encoding needs to return to its initial state.
    pub(super) fn finish(&mut self, out: &mut Vec<u8>) {
        if let Encoder::Iconv(converter) = self {
            let mut buffer = [0u8; 64];
            let mut empty: &[u8] = &[];
            let (produced, _) = converter.step(&mut empty, &mut buffer, false);
            out.extend_from_slice(&buffer[..produced]);
        }
    }

    /// Encodes one code point, reporting `false` when the encoding cannot represent it.
    fn put(&mut self, w: u32, out: &mut Vec<u8>) -> bool {
        match self {
            Encoder::Utf8 => put_utf8(w, out),
            Encoder::SingleByte(table) => {
                let Ok(code) = u16::try_from(w) else {
                    return false;
                };
                match table.encode.binary_search_by_key(&code, |&(cp, _)| cp) {
                    Ok(index) => {
                        out.push(table.encode[index].1);
                        true
                    }
                    Err(_) => false,
                }
            }
            Encoder::Utf16(little) => {
                if w < 0x1_0000 {
                    put_unit(w, 2, *little, out);
                } else if w < 0x11_0000 {
                    put_unit(((w >> 10) - 0x40) | 0xD800, 2, *little, out);
                    put_unit((w & 0x3FF) | 0xDC00, 2, *little, out);
                } else {
                    return false;
                }
                true
            }
            Encoder::Utf32(little) => {
                if w >= 0x11_0000 {
                    return false;
                }
                put_unit(w, 4, *little, out);
                true
            }
            Encoder::Ucs2(little) => {
                if w >= 0x1_0000 {
                    return false;
                }
                put_unit(w, 2, *little, out);
                true
            }
            Encoder::Ucs4(little) => {
                if w == BAD_INPUT {
                    return false;
                }
                put_unit(w, 4, *little, out);
                true
            }
            Encoder::Iconv(converter) => {
                if w >= 0x11_0000 {
                    return false;
                }
                let unit = w.to_le_bytes();
                let mut cursor: &[u8] = &unit;
                let mut buffer = [0u8; 64];
                let (produced, failure) = converter.step(&mut cursor, &mut buffer, true);
                if failure.is_some() || !cursor.is_empty() {
                    return false;
                }
                out.extend_from_slice(&buffer[..produced]);
                true
            }
        }
    }
}

/// Ports libmbfl's `mb_wchar_to_utf8`; anything at or past U+110000 is unrepresentable.
fn put_utf8(w: u32, out: &mut Vec<u8>) -> bool {
    if w < 0x80 {
        out.push(w as u8);
    } else if w < 0x800 {
        out.extend_from_slice(&[0xC0 | ((w >> 6) & 0x1F) as u8, 0x80 | (w & 0x3F) as u8]);
    } else if w < 0x1_0000 {
        out.extend_from_slice(&[
            0xE0 | ((w >> 12) & 0x0F) as u8,
            0x80 | ((w >> 6) & 0x3F) as u8,
            0x80 | (w & 0x3F) as u8,
        ]);
    } else if w < 0x11_0000 {
        out.extend_from_slice(&[
            0xF0 | ((w >> 18) & 0x07) as u8,
            0x80 | ((w >> 12) & 0x3F) as u8,
            0x80 | ((w >> 6) & 0x3F) as u8,
            0x80 | (w & 0x3F) as u8,
        ]);
    } else {
        return false;
    }
    true
}

/// Appends one 2- or 4-byte unit in the requested byte order.
fn put_unit(value: u32, width: usize, little: bool, out: &mut Vec<u8>) {
    let bytes = value.to_be_bytes();
    let unit = &bytes[4 - width..];
    if little {
        out.extend(unit.iter().rev());
    } else {
        out.extend_from_slice(unit);
    }
}
