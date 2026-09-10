//! Purpose:
//! Shares the legacy streaming byte-budget cut driver for transfer encodings.
//!
//! Called from:
//! - `super::Transfer::cut`.
//!
//! Key details:
//! - Prefix bytes establish decoder state while discarding decoded output.
//! - Each candidate includes both decoder and encoder flush output before acceptance.

/// A cloneable historical decoder whose pending bytes may emit characters on flush.
pub(super) trait Decode: Clone {
    /// Consumes one encoded byte and appends any resulting codepoints.
    fn push(&mut self, byte: u8, output: &mut Vec<u32>);
    /// Flushes the decoder's pending character or literal prefix.
    fn finish(&mut self, output: &mut Vec<u32>);
}

/// A cloneable historical encoder retaining line wrapping and pending output bytes.
pub(super) trait Encode: Clone {
    /// Encodes one decoded character into the ongoing output stream.
    fn push(&mut self, code: u32, output: &mut Vec<u8>);
    /// Flushes pending output without appending a separate string terminator.
    fn finish(&mut self, output: &mut Vec<u8>);
}

/// Keeps the last streaming state whose complete flushed output fits the clamped byte budget.
pub(super) fn apply<D: Decode, E: Encode>(input: &[u8], from: usize, length: usize, mut decoder: D, mut encoder: E) -> Vec<u8> {
    let budget = length.min(input.len() - from);
    let mut points = Vec::new();
    for &byte in &input[..from] { decoder.push(byte, &mut points); points.clear(); }
    let mut output = Vec::new();
    for &byte in &input[from..] {
        let (previous_decoder, previous_encoder, previous_length) = (decoder.clone(), encoder.clone(), output.len());
        decoder.push(byte, &mut points);
        for code in points.drain(..) { encoder.push(code, &mut output); }
        let (mut probe_decoder, mut probe_encoder) = (decoder.clone(), encoder.clone());
        let mut tail = Vec::new();
        probe_decoder.finish(&mut points);
        for code in points.drain(..) { probe_encoder.push(code, &mut tail); }
        probe_encoder.finish(&mut tail);
        if output.len() + tail.len() > budget {
            decoder = previous_decoder;
            encoder = previous_encoder;
            output.truncate(previous_length);
            break;
        }
    }
    decoder.finish(&mut points);
    for code in points { encoder.push(code, &mut output); }
    encoder.finish(&mut output);
    output
}
