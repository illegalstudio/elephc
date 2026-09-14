//! Purpose:
//! Encodes one output-handler feed with its explicit final-input flag.
//!
//! Called from:
//! - Shared request output conversion after bounded source decoding.
//!
//! Key details:
//! - The final flag applies to the final decoder batch rather than an extra empty call.
//! - Empty input makes no encoder call and produces no prologue or flush bytes.

use super::*;

impl Encoding {
    /// Preserves decoder batch boundaries and destination lookahead for one output-buffer feed.
    pub(crate) fn encode_output_chunks(self, chunks: &[Vec<u32>], substitute: Substitute, finish: bool) -> Vec<u8> {
        if chunks.is_empty() { return Vec::new(); }
        match ENCODINGS[self.0].codec {
            Codec::DoubleByte(codec) if self.name() == "SJIS-mac" =>
                super::super::mac::encode_output_chunks(codec, chunks, substitute, finish),
            Codec::DoubleByte(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::MobileSjis(codec) => codec.encode_output_chunks(chunks.iter().map(Vec::as_slice), substitute, finish),
            Codec::Jis(codec) => codec.encode_prefix(chunks.iter().map(Vec::as_slice), substitute, finish),
            Codec::Jis2004(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::Utf7(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::Hz(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::Iso2022Kr(codec) => codec.encode_prefix(&chunks.concat(), substitute, finish),
            Codec::Transfer(codec) => codec.encode_output_chunks(chunks, finish),
            _ => chunks.iter().flat_map(|chunk| self.encode(chunk, substitute)).collect(),
        }
    }
}
