//! Purpose:
//! Combines shared codecs and Unicode algorithms for mbstring text operations.
//!
//! Called from:
//! - mbstring backend adapters and PHP compatibility tests.
//!
//! Key details:
//! - Source and destination encodings remain explicit; request settings resolve defaults.
//! - Casing preserves the original decoder's batch boundaries for contextual sigma.

use crate::encoding::{Decoded, Encoding, Substitute};
use crate::unicode::{self, CaseMode};

mod characters;
mod conversion;
mod cut;
mod entities;
mod pad;
mod search;
mod slicing;
mod trim;

pub use characters::{chr, first_case, ord};
pub use conversion::ConversionSources;
pub use cut::strcut;
pub use entities::{decode_numericentity, encode_numericentity};
pub use pad::str_pad;
pub use search::{strpos, strstr, substr_count, SearchMode};
pub use slicing::{str_split, substr, validate_split_length, validate_substr_bounds};
pub use trim::{strimwidth, trim, TrimSide};
pub use crate::unicode::kana::KanaMode;

/// Converts Japanese width and kana using options validated before resolving the encoding.
pub fn convert_kana(input: &[u8], mode: KanaMode, encoding: Encoding, substitution: Substitute) -> Vec<u8> {
    let decoded = encoding.decode(input);
    if decoded.points.is_empty() { return Vec::new(); }
    if encoding.needs_transform_batches() {
        return kana_batches(&decoded, mode, encoding, substitution);
    }
    encoding.encode(&mode.convert(&decoded.points), substitution)
}

/// Converts bytes between two canonical encodings with the selected replacement policy.
pub fn convert_encoding(
    input: &[u8],
    from: Encoding,
    to: Encoding,
    substitution: Substitute,
) -> Vec<u8> {
    to.encode_conversion(input, from, substitution)
}

/// Converts bytes and reports PHP's rejected-unit count, including replacement failures.
pub fn convert_encoding_with_errors(
    input: &[u8], from: Encoding, to: Encoding, substitution: Substitute,
) -> (Vec<u8>, u64) {
    crate::encoding::errors::measure(|| convert_encoding(input, from, to, substitution))
}

/// Replaces malformed units while preserving the source encoding's canonical representation.
pub fn scrub(input: &[u8], encoding: Encoding, substitution: Substitute) -> Vec<u8> {
    convert_encoding(input, encoding, encoding, substitution)
}

/// Converts case with PHP's Unicode, source-encoding, and contextual batch semantics.
pub fn convert_case(
    input: &[u8],
    mode: CaseMode,
    encoding: Encoding,
    substitution: Substitute,
) -> Vec<u8> {
    if input.is_empty() { return Vec::new(); }
    let decoded = encoding.decode_buffer(input, 64);
    let mut start = 0;
    let points = &decoded.points;
    let chunks = decoded.case_batches.iter().map(move |&end| {
        let chunk = &points[start..end];
        start = end;
        chunk
    });
    let converted = unicode::convert_case_buffers(chunks, mode, encoding.uses_turkish_case());
    encoding.encode_chunks(&converted, substitution)
}

/// Sums PHP's East Asian display widths, including controls and invalid units as width one.
pub fn strwidth(input: &[u8], encoding: Encoding) -> usize {
    encoding.decode(input).points.into_iter().map(unicode::character_width).sum()
}

/// Retains kana's one-character lookahead and encoder calls across mobile decoder buffers.
fn kana_batches(decoded: &Decoded, mode: KanaMode, encoding: Encoding, substitution: Substitute) -> Vec<u8> {
    let mut chunks = Vec::new();
    let (mut start, mut pending) = (0, Vec::new());
    while start < decoded.points.len() {
        let end = encoding.mobile_batch_end(decoded, start, 64 - pending.len());
        pending.extend_from_slice(&decoded.points[start..end]);
        let mut offset = 0;
        let mut output = Vec::new();
        while offset + 1 < pending.len() || (end == decoded.points.len() && offset < pending.len()) {
            let (points, consumed) = mode.point(pending[offset], pending.get(offset + 1).copied().unwrap_or(0));
            output.push(points[0]);
            if points[1] != 0 { output.push(points[1]); }
            offset += 1 + usize::from(consumed);
        }
        pending.drain(..offset);
        chunks.push(output);
        start = end;
    }
    encoding.encode_chunks(&chunks, substitution)
}
