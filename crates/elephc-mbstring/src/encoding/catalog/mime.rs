//! Purpose:
//! Encodes bounded MIME trial chunks with explicit provisional and final flush behavior.
//!
//! Called from:
//! - The MIME header encoder while determining how many codepoints fit on one line.
//!
//! Key details:
//! - Trials replay only the current bounded line, preserving original encoder call boundaries.
//! - Replacement uses a fixed question mark and does not update PHP request counters.

use super::*;

impl Encoding {
    /// Reports whether PHP permits this canonical charset as a MIME-header destination.
    pub(crate) fn supports_mime_header(self) -> bool {
        self.mime_name().is_some_and(|name| !name.is_empty())
            && !matches!(ENCODINGS[self.0].codec, Codec::Transfer(Transfer::QuotedPrintable))
    }

    /// Replays one line's accepted chunks, optionally including a separate final encoder call.
    pub(crate) fn encode_mime_prefix(self, chunks: &[Vec<u32>], finish: bool) -> Vec<u8> {
        let substitute = Substitute::default();
        match ENCODINGS[self.0].codec {
            Codec::DoubleByte(codec) if self.name() == "SJIS-mac" => super::super::mac::encode_prefix(codec, chunks, finish),
            Codec::DoubleByte(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::Jis(codec) => codec.encode_prefix(chunks.iter().map(Vec::as_slice), substitute, finish),
            Codec::MobileSjis(codec) => codec.encode_prefix(chunks, finish),
            Codec::Jis2004(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::Utf7(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::Hz(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::Iso2022Kr(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::Transfer(codec) => codec.encode_prefix(chunks, finish),
            _ => chunks.iter().flat_map(|chunk| self.encode(chunk, substitute)).collect(),
        }
    }
}
