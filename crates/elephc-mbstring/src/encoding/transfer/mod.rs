//! Purpose:
//! Centralizes mbstring's historical transfer encodings and their byte-oriented conversion rules.
//!
//! Called from:
//! - The canonical encoding catalog and shared string operations.
//!
//! Key details:
//! - Fast conversion can replace the requested source or destination with raw bytes.
//! - Character-oriented operations still consume each transfer decoder's codepoint stream.

mod base64;
mod qprint;
mod uuencode;
mod html;
mod cut;

use super::Decoded;

/// The four deprecated transfer encodings still present in PHP's public mbstring catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Transfer { Base64, QuotedPrintable, Uuencode, Html }

impl Transfer {
    /// Supplies PHP's explicit-encoding diagnostic for the shared cached lookup adapter.
    pub fn deprecation(self) -> &'static str {
        match self {
            Self::Base64 => "Handling Base64 via mbstring is deprecated; use base64_encode/base64_decode instead",
            Self::QuotedPrintable => "Handling QPrint via mbstring is deprecated; use quoted_printable_encode/quoted_printable_decode instead",
            Self::Uuencode => "Handling Uuencode via mbstring is deprecated; use convert_uuencode/convert_uudecode instead",
            Self::Html => "Handling HTML entities via mbstring is deprecated; use htmlspecialchars, htmlentities, or mb_encode_numericentity/mb_decode_numericentity instead",
        }
    }

    /// Decodes one transfer representation using the requested scratch-buffer capacity.
    pub fn decode(self, input: &[u8], capacity: usize) -> Decoded {
        match self {
            Self::Base64 => base64::decode(input, capacity),
            Self::QuotedPrintable => qprint::decode(input, capacity),
            Self::Uuencode => uuencode::decode(input, capacity),
            Self::Html => html::decode(input, capacity),
        }
    }

    /// Reads one bounded transfer-decoder call and preserves its packed carry state.
    pub fn decode_next(self, input: &[u8], capacity: usize, state: &mut u32) -> (Decoded, usize) {
        match self {
            Self::Base64 => base64::decode_next(input, capacity, state),
            Self::QuotedPrintable => qprint::decode_next(input, capacity),
            Self::Uuencode => uuencode::decode_next(input, capacity, state),
            Self::Html => html::decode_next(input, capacity),
        }
    }

    /// Preserves codec-specific partial-group behavior at each output invocation.
    pub fn encode_chunks(self, chunks: &[Vec<u32>]) -> Vec<u8> {
        if self == Self::Uuencode { uuencode::encode_chunks(chunks) }
        else { self.encode(&chunks.concat()) }
    }

    /// Builds a provisional MIME payload with the transfer codec's explicit final-call policy.
    pub fn encode_prefix(self, chunks: &[Vec<u32>], finish: bool) -> Vec<u8> {
        match self {
            Self::Base64 => base64::encode_prefix(&chunks.concat(), finish),
            Self::Uuencode => uuencode::encode_prefix(chunks, finish),
            _ => self.encode(&chunks.concat()),
        }
    }

    /// Marks the final nonempty input batch without inserting a separate MIME-style flush call.
    pub fn encode_output_chunks(self, chunks: &[Vec<u32>], finish: bool) -> Vec<u8> {
        if self == Self::Uuencode { uuencode::encode_output_chunks(chunks, finish) }
        else { self.encode_prefix(chunks, finish) }
    }

    /// Encodes the decoded stream without applying Unicode substitution to raw transfer bytes.
    pub fn encode(self, input: &[u32]) -> Vec<u8> {
        match self {
            Self::Base64 => base64::encode(input),
            Self::QuotedPrintable => qprint::encode(input),
            Self::Uuencode => uuencode::encode(input),
            Self::Html => html::encode(input),
        }
    }

    /// Applies the legacy filter pair used by mb_strcut, or UUENCODE's raw-byte slicing rule.
    pub fn cut(self, input: &[u8], from: usize, length: usize) -> Vec<u8> {
        match self {
            Self::Base64 => cut::apply(input, from, length, base64::Decoder::default(), base64::Encoder::default()),
            Self::QuotedPrintable => cut::apply(input, from, length, qprint::Decoder::default(), qprint::Encoder::default()),
            Self::Html => cut::apply(input, from, length, html::Decoder::default(), html::Encoder),
            Self::Uuencode => input[from..from + length.min(input.len() - from)].to_vec(),
        }
    }

    /// Identifies destinations that force the fast conversion source to 8bit.
    pub fn raw_source(self) -> bool { matches!(self, Self::Base64 | Self::QuotedPrintable) }

    /// Identifies sources that force the fast conversion destination to 8bit.
    pub fn raw_destination(self) -> bool { matches!(self, Self::Base64 | Self::QuotedPrintable | Self::Uuencode) }
}
