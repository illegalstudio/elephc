//! Purpose:
//! Centralizes mbstring encoding identities, aliases, MIME names, and codec selection.
//!
//! Called from:
//! - Shared mbstring operations, encoding-setting APIs, and compatibility tests.
//!
//! Key details:
//! - Only aliases recorded by the PHP baseline are accepted, case-insensitively.
//! - Every canonical encoding selects an explicit native codec implementation.

mod stream;
mod mime;
mod output;

use super::{catalog_data::ENCODINGS, doublebyte::DoubleByte, mobile_utf8::MobileUtf8, singlebyte::SingleByte, Decoded, Substitute, UnicodeEncoding};
use super::utf7::Utf7;
use super::gb18030::Gb18030;
use super::euctw::EucTw;
use super::hz::Hz;
use super::iso2022kr::Iso2022Kr;
use super::jis::Jis;
use super::jis2004::Jis2004;
use super::transfer::Transfer;
use super::mobile_sjis::MobileSjis;

/// Codec implementation associated with one canonical encoding identity.
#[derive(Clone, Copy, Debug)]
pub(super) enum Codec {
    Unicode(UnicodeEncoding),
    SingleByte(SingleByte),
    DoubleByte(DoubleByte),
    MobileUtf8(MobileUtf8),
    MobileSjis(MobileSjis),
    Utf7(Utf7),
    Gb18030(Gb18030),
    EucTw(EucTw),
    Hz(Hz),
    Iso2022Kr(Iso2022Kr),
    Jis(Jis),
    Jis2004(Jis2004),
    Transfer(Transfer),
}

/// Generated metadata for a canonical PHP encoding.
pub(super) struct EncodingInfo {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub mime: Option<&'static str>,
    pub supports_ord_chr: bool,
    pub supports_detection: bool,
    pub codec: Codec,
    pub slicing: Slicing,
}

/// PHP's raw-byte slicing strategy for a canonical encoding.
#[derive(Clone, Copy, Debug)]
pub enum Slicing {
    /// Raw code units of fixed byte width, including incomplete final units in substr/split.
    Fixed(usize),
    /// Byte width selected only by the leading byte, even for malformed input.
    LeadingByte(&'static [u8]),
    /// Decode codepoints and re-encode the selected portion.
    Converted,
}

/// One canonical encoding index in PHP's stable list order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoding(usize);

impl Encoding {
    /// Resolves canonical names, then MIME names, then aliases in PHP's catalog order.
    pub fn lookup(name: &[u8]) -> Option<Self> {
        ENCODINGS.iter().position(|entry| name.eq_ignore_ascii_case(entry.name.as_bytes()))
            .or_else(|| ENCODINGS.iter().position(|entry| entry.mime.is_some_and(|mime| name.eq_ignore_ascii_case(mime.as_bytes()))))
            .or_else(|| ENCODINGS.iter().position(|entry| entry.aliases.iter().any(|alias| name.eq_ignore_ascii_case(alias.as_bytes()))))
            .map(Self)
    }

    /// Resolves the NUL-terminated name accepted by ordinary PHP encoding arguments.
    pub fn lookup_c_string(name: &[u8]) -> Option<Self> {
        Self::lookup(name.split(|&byte| byte == 0).next().unwrap_or_default())
    }

    /// Returns the deprecation emitted by explicit ordinary lookup of a transfer encoding.
    pub(crate) fn deprecation(self) -> Option<&'static str> {
        match ENCODINGS[self.0].codec {
            Codec::Transfer(codec) => Some(codec.deprecation()),
            _ => None,
        }
    }

    /// Enumerates canonical encodings in the order returned by `mb_list_encodings()`.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..ENCODINGS.len()).map(Self)
    }

    /// Returns the canonical name while preserving PHP's spelling.
    pub fn name(self) -> &'static str {
        ENCODINGS[self.0].name
    }

    /// Returns every public alias in PHP's enumeration order.
    pub fn aliases(self) -> &'static [&'static str] {
        ENCODINGS[self.0].aliases
    }

    /// Returns the registered MIME name, if PHP has one for this encoding.
    pub fn mime_name(self) -> Option<&'static str> {
        ENCODINGS[self.0].mime
    }

    /// Reports whether PHP permits this encoding in mb_ord and mb_chr.
    pub fn supports_ord_chr(self) -> bool { ENCODINGS[self.0].supports_ord_chr }

    /// Reports whether PHP keeps this encoding as a candidate during text detection.
    pub fn supports_detection(self) -> bool { ENCODINGS[self.0].supports_detection }

    /// Identifies codecs with a separate strict validation pass before detection scoring.
    pub(crate) fn detection_precheck(self) -> bool {
        matches!(ENCODINGS[self.0].codec, Codec::Utf7(_))
            || matches!(ENCODINGS[self.0].codec, Codec::Jis(codec)
                if matches!(codec.variant, super::jis::Variant::Jis | super::jis::Variant::Iso2022))
    }

    /// Returns the authoritative substring/split/cut fast-path strategy for this encoding.
    pub fn slicing(self) -> Slicing {
        ENCODINGS[self.0].slicing
    }

    /// Applies an encoding-specific legacy cut filter when the codec owns one.
    pub(crate) fn cut(self, input: &[u8], from: usize, length: usize) -> Option<Vec<u8>> {
        match ENCODINGS[self.0].codec {
            Codec::Utf7(codec) => Some(codec.cut(input, from, length)),
            Codec::Gb18030(codec) => Some(codec.cut(input, from, length)),
            Codec::Hz(codec) => Some(codec.cut(input, from, length)),
            Codec::Iso2022Kr(codec) => Some(codec.cut(input, from, length)),
            Codec::Jis(codec) => Some(codec.cut(input, from, length)),
            Codec::Jis2004(codec) => Some(codec.cut(input, from, length)),
            Codec::Transfer(codec) => Some(codec.cut(input, from, length)),
            _ => None,
        }
    }

    /// Selects a built-in Unicode codec when this encoding has that representation.
    pub fn unicode(self) -> Option<UnicodeEncoding> {
        match ENCODINGS[self.0].codec {
            Codec::Unicode(codec) => Some(codec),
            _ => None,
        }
    }

    /// Reports whether this encoding uses PHP's Turkish dotted/dotless I exceptions.
    pub fn uses_turkish_case(self) -> bool {
        self.name() == "ISO-8859-9"
    }

    /// Reports whether fast conversion from this encoding returns bytes regardless of its destination.
    pub(crate) fn raw_conversion_destination(self) -> bool {
        matches!(ENCODINGS[self.0].codec, Codec::Transfer(codec) if codec.raw_destination())
    }

    /// Identifies output encoders whose invocation boundaries affect transformed character streams.
    pub(crate) fn needs_transform_batches(self) -> bool {
        matches!(ENCODINGS[self.0].codec, Codec::MobileSjis(_))
            || matches!(ENCODINGS[self.0].codec, Codec::Jis(codec) if codec.needs_encoder_batches())
    }

    /// Selects one mobile decoder batch, reserving space for an atomic two-point expansion.
    pub(crate) fn mobile_batch_end(self, decoded: &Decoded, start: usize, capacity: usize) -> usize {
        debug_assert!(self.needs_transform_batches());
        let mut end = start;
        while end < decoded.points.len() && end - start < capacity - 1 {
            end += 1;
            while end < decoded.points.len() && decoded.offsets[end] == decoded.offsets[end - 1] { end += 1; }
        }
        end
    }

    /// Decodes input with this encoding, preserving invalid-unit markers and offsets.
    pub fn decode(self, input: &[u8]) -> Decoded {
        match ENCODINGS[self.0].codec {
            Codec::Unicode(codec) => codec.decode(input),
            Codec::SingleByte(codec) => codec.decode(input),
            Codec::DoubleByte(codec) => codec.decode(input),
            Codec::MobileUtf8(codec) => codec.decode(input),
            Codec::MobileSjis(codec) => codec.base.decode(input),
            Codec::Utf7(codec) => codec.decode(input),
            Codec::Gb18030(codec) => codec.decode(input),
            Codec::EucTw(codec) => codec.decode(input),
            Codec::Hz(codec) => codec.decode(input),
            Codec::Iso2022Kr(codec) => codec.decode(input),
            Codec::Jis(codec) => codec.decode(input),
            Codec::Jis2004(codec) => codec.decode(input),
            Codec::Transfer(codec) => codec.decode(input, 64),
        }
    }

    /// Reconstructs bounded decoder batches for conversion and numeric-entity consumers.
    pub(crate) fn decode_buffer(self, input: &[u8], capacity: usize) -> Decoded {
        let decoded = match ENCODINGS[self.0].codec {
            Codec::Utf7(codec) => codec.decode_buffer(input, capacity),
            Codec::Jis(codec) => codec.decode_buffer(input, capacity),
            Codec::Jis2004(codec) => codec.decode_buffer(input, capacity),
            Codec::Transfer(codec) => codec.decode(input, capacity),
            Codec::DoubleByte(codec) => codec.decode_buffer_state(input, capacity, &mut 0),
            Codec::MobileSjis(codec) => codec.base.decode_buffer_state(input, capacity, &mut 0),
            _ => self.decode(input),
        };
        self.partition_decoded(input, capacity, decoded)
    }

    /// Supplies bounded partitions for codecs which report only complete decoded streams.
    fn partition_decoded(self, input: &[u8], capacity: usize, mut decoded: Decoded) -> Decoded {
        if self.name().starts_with("UTF-16") {
            decoded.case_batches = super::utf16_batches::ends(input, self.name() == "UTF-16LE", self.name() == "UTF-16", capacity);
        } else if decoded.case_batches.is_empty() {
            let reserve = usize::from(matches!(self.name(), "EUC-JP-2004" | "SJIS-2004") || self.name().contains("Mobile#"));
            let mut start = 0;
            while start < decoded.points.len() {
                let mut end = start;
                while end < decoded.points.len() && end - start < capacity - reserve {
                    let mut next = end + 1;
                    while next < decoded.points.len() && decoded.offsets[next] == decoded.offsets[end] { next += 1; }
                    if next - start > capacity { break; }
                    end = next;
                }
                decoded.case_batches.push(end);
                start = end;
            }
        }
        decoded
    }

    /// Uses explicit decoder chunks only for encoders whose lookahead ends at those boundaries.
    pub(crate) fn encode_conversion(self, input: &[u8], from: Self, substitution: Substitute) -> Vec<u8> {
        if input.is_empty() { return Vec::new(); }
        if let Codec::Transfer(codec) = ENCODINGS[self.0].codec {
            if codec.raw_source() { return codec.encode(&input.iter().copied().map(u32::from).collect::<Vec<_>>()); }
        }
        if let Codec::Transfer(codec) = ENCODINGS[from.0].codec {
            if codec.raw_destination() {
                return UnicodeEncoding::EightBit.encode(&codec.decode(input, 128).points, substitution);
            }
        }
        if let Codec::Jis(codec) = ENCODINGS[self.0].codec {
            if codec.needs_encoder_batches() {
                let decoded = from.decode_buffer(input, 128);
                let mut start = 0;
                let chunks = decoded.case_batches.iter().map(|&end| {
                    let chunk = &decoded.points[start..end];
                    start = end;
                    chunk
                });
                return codec.encode_chunks(chunks, substitution);
            }
        }
        if let Codec::MobileSjis(codec) = ENCODINGS[self.0].codec {
            let decoded = from.decode_buffer(input, 128);
            let mut start = 0;
            return codec.encode_chunks(decoded.case_batches.iter().map(|&end| {
                let chunk = &decoded.points[start..end];
                start = end;
                chunk
            }), substitution);
        }
        self.encode(&from.decode(input).points, substitution)
    }

    /// Encodes transformed chunks while preserving codec-specific lookahead boundaries.
    pub(crate) fn encode_chunks(self, chunks: &[Vec<u32>], substitution: Substitute) -> Vec<u8> {
        if let Codec::MobileSjis(codec) = ENCODINGS[self.0].codec {
            return codec.encode_chunks(chunks.iter().map(Vec::as_slice), substitution);
        }
        if let Codec::Jis(codec) = ENCODINGS[self.0].codec {
            if codec.needs_encoder_batches() {
                return codec.encode_chunks(chunks.iter().map(Vec::as_slice), substitution);
            }
        }
        self.encode(&chunks.concat(), substitution)
    }

    /// Encodes MIME chunks with a separate empty final call and fixed replacement policy.
    pub(crate) fn encode_mime_chunks(self, chunks: &[Vec<u32>]) -> Vec<u8> {
        if let Codec::Transfer(codec) = ENCODINGS[self.0].codec { return codec.encode_chunks(chunks); }
        self.encode_chunks(chunks, Substitute::default())
    }

    /// Encodes codepoints using the exact destination mapping and substitution policy.
    pub fn encode(self, points: &[u32], substitution: Substitute) -> Vec<u8> {
        match ENCODINGS[self.0].codec {
            Codec::Unicode(codec) => codec.encode(points, substitution),
            Codec::SingleByte(codec) => codec.encode(points, substitution),
            Codec::DoubleByte(codec) => codec.encode(points, substitution),
            Codec::MobileUtf8(codec) => codec.encode(points, substitution),
            Codec::MobileSjis(codec) => codec.encode_chunks(std::iter::once(points), substitution),
            Codec::Utf7(codec) => codec.encode(points, substitution),
            Codec::Gb18030(codec) => codec.encode(points, substitution),
            Codec::EucTw(codec) => codec.encode(points, substitution),
            Codec::Hz(codec) => codec.encode(points, substitution),
            Codec::Iso2022Kr(codec) => codec.encode(points, substitution),
            Codec::Jis(codec) => codec.encode(points, substitution),
            Codec::Jis2004(codec) => codec.encode(points, substitution),
            Codec::Transfer(codec) => codec.encode(points),
        }
    }

    /// Encodes one scalar for mb_chr, rejecting errors even if the codec emits control bytes.
    pub(crate) fn encode_scalar(self, code: u32) -> Option<Vec<u8>> {
        if let Codec::Iso2022Kr(codec) = ENCODINGS[self.0].codec { return codec.encode_scalar(code); }
        let output = self.encode(&[code], Substitute { mode: super::SubstituteMode::None, ..Substitute::default() });
        if output.is_empty() { None } else { Some(output) }
    }

    /// Counts characters using PHP's encoding-specific optimized length rules.
    pub fn strlen(self, input: &[u8]) -> usize {
        match ENCODINGS[self.0].codec {
            Codec::Unicode(codec) => codec.strlen(input),
            Codec::SingleByte(_) => input.len(),
            Codec::DoubleByte(codec) => codec.decode(input).points.len(),
            Codec::MobileUtf8(codec) => codec.decode(input).points.len(),
            Codec::MobileSjis(codec) => codec.base.decode(input).points.len(),
            Codec::Utf7(codec) => codec.decode(input).points.len(),
            Codec::Gb18030(codec) => codec.decode(input).points.len(),
            Codec::EucTw(codec) => codec.decode(input).points.len(),
            Codec::Hz(codec) => codec.decode(input).points.len(),
            Codec::Iso2022Kr(codec) => codec.decode(input).points.len(),
            Codec::Jis(codec) => codec.decode(input).points.len(),
            Codec::Jis2004(codec) => codec.decode(input).points.len(),
            Codec::Transfer(Transfer::Uuencode) => input.len(),
            Codec::Transfer(codec) => codec.decode(input, 128).points.len(),
        }
    }
}
