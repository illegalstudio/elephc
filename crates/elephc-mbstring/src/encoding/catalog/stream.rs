//! Purpose:
//! Reads one bounded source-decoder invocation without decoding the unread suffix.
//!
//! Called from:
//! - MIME operations which preserve PHP's scratch-buffer and decoder-state boundaries.
//!
//! Key details:
//! - Stateful codecs stop only at their own atomic unit and output-reservation boundaries.
//! - Stateless lookahead is bounded by four source bytes per point plus two complete units.

use super::*;

impl Encoding {
    /// Advances the input by one PHP decoder call, retaining state for the next source buffer.
    pub(crate) fn decode_next(self, input: &mut &[u8], capacity: usize, state: &mut u32) -> Decoded {
        assert!(capacity >= 5, "decoder scratch buffer must hold the largest atomic expansion");
        let (decoded, consumed) = match ENCODINGS[self.0].codec {
            Codec::Unicode(codec) => {
                let (codec, bytes) = codec.stateful_input(input, state);
                let skipped = input.len() - bytes.len();
                let encoding = Self::all().find(|encoding| encoding.unicode() == Some(codec)).expect("registered Unicode codec");
                let (mut decoded, consumed) = encoding.decode_stateless_next(bytes, capacity);
                for offset in &mut decoded.offsets { *offset += skipped; }
                (decoded, consumed + skipped)
            }
            Codec::Utf7(codec) => codec.decode_next(input, capacity, state),
            Codec::Jis(codec) => codec.decode_next(input, capacity, state),
            Codec::Jis2004(codec) => codec.decode_next(input, capacity, state),
            Codec::Hz(codec) => codec.decode_next(input, capacity, state),
            Codec::Iso2022Kr(codec) => codec.decode_next(input, capacity, state),
            Codec::Transfer(codec) => codec.decode_next(input, capacity, state),
            Codec::DoubleByte(codec) if !codec.escapes.is_empty() => codec.decode_next(input, capacity, state),
            Codec::MobileSjis(codec) if !codec.base.escapes.is_empty() => codec.base.decode_next(input, capacity, state),
            _ => self.decode_stateless_next(input, capacity),
        };
        assert!(consumed <= input.len() && (input.is_empty() || consumed != 0), "decoder must make bounded source progress");
        *input = &input[consumed..];
        decoded
    }

    /// Limits stateless lookahead while preserving atomic expansions and PHP's UTF-16 partitions.
    fn decode_stateless_next(self, input: &[u8], capacity: usize) -> (Decoded, usize) {
        let lookahead = capacity.saturating_mul(4).saturating_add(8).min(input.len());
        let mut decoded = self.decode_buffer(&input[..lookahead], capacity);
        let end = decoded.case_batches.first().copied().unwrap_or(decoded.points.len());
        let consumed = decoded.offsets.get(end).copied().unwrap_or(lookahead);
        decoded.points.truncate(end);
        decoded.offsets.truncate(end);
        decoded.case_batches.clear();
        decoded.case_batches.push(end);
        (decoded, consumed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks resumed conversion across all codecs, malformed units, and varying buffer limits.
    #[test]
    fn bounded_decoders_preserve_complete_source_points() {
        let text = "Ascii café 猫 東京 한글 😀 🇯🇵 123!? ".repeat(35).chars().map(u32::from).collect::<Vec<_>>();
        let malformed = (0..1200).map(|index| ((index * 73 + index / 7) & 255) as u8).collect::<Vec<_>>();
        for encoding in Encoding::all() {
            let encoded = encoding.encode(&text, Substitute::default());
            for input in [encoded.as_slice(), malformed.as_slice()] {
                for capacity in [5, 7, 15, 64, 90, 128] {
                    let expected = encoding.decode_buffer(input, capacity);
                    let (mut remaining, mut state, mut points) = (input, 0, Vec::new());
                    while !remaining.is_empty() {
                        let decoded = encoding.decode_next(&mut remaining, capacity, &mut state);
                        assert!(decoded.points.len() <= capacity, "{}: {capacity}", encoding.name());
                        points.extend(decoded.points);
                    }
                    assert_eq!(points, expected.points, "{}: capacity={capacity}, encoded={}", encoding.name(), input == encoded);
                }
            }
        }
    }

    /// Stops inside UTF-7 Base64 data without flushing the shift or treating the buffer end as EOF.
    #[test]
    fn bounded_utf7_keeps_unread_shifted_bytes() {
        let encoding = Encoding::lookup(b"UTF-7").unwrap();
        let bytes = encoding.encode(&vec![0x732b; 12], Substitute::default());
        let (mut input, mut state) = (bytes.as_slice(), 0);
        let decoded = encoding.decode_next(&mut input, 5, &mut state);
        assert_eq!(decoded.points, vec![0x732b; 3]);
        assert_eq!(bytes.len() - input.len(), 9);
        assert_eq!(state, 1);
        let decoded = encoding.decode_next(&mut input, 5, &mut state);
        assert_eq!(decoded.points, vec![0x732b; 3]);
        assert_eq!(bytes.len() - input.len(), 17);
    }
}
