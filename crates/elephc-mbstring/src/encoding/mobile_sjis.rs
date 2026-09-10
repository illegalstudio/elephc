//! Purpose:
//! Preserves mobile Shift-JIS encoder state across ordinary chunks and replacement calls.
//!
//! Called from:
//! - The encoding catalog for DOCOMO, KDDI, and SoftBank Shift-JIS variants.
//!
//! Key details:
//! - Replacement markers use the same keycap lookahead as ordinary input.
//! - A replacement's final digit can remain deferred until another encoder invocation.

use super::{doublebyte::DoubleByte, Substitute, SubstituteMode};

/// Shared character maps with the carrier's regional-indicator lookahead capability.
#[derive(Clone, Copy, Debug)]
pub(super) struct MobileSjis { pub base: DoubleByte, pub flags: bool }

impl MobileSjis {
    /// Encodes consecutive buffers with persistent lookahead and PHP's final-call flag.
    pub fn encode_chunks<'a>(self, chunks: impl Iterator<Item = &'a [u32]>, substitution: Substitute) -> Vec<u8> {
        self.encode_output_chunks(chunks, substitution, true)
    }

    /// Applies the final-input flag to the last actual batch of an output-handler call.
    pub fn encode_output_chunks<'a>(self, chunks: impl Iterator<Item = &'a [u32]>, substitution: Substitute, finish: bool) -> Vec<u8> {
        let mut chunks = chunks.peekable();
        let mut encoder = Encoder { codec: self, pending: None, output: Vec::new() };
        while let Some(chunk) = chunks.next() {
            encoder.call(chunk, substitution, finish && chunks.peek().is_none());
        }
        encoder.output
    }

    /// Applies ordinary MIME trial chunks followed by an optional separate empty final call.
    pub fn encode_prefix(self, chunks: &[Vec<u32>], finish: bool) -> Vec<u8> {
        let mut encoder = Encoder { codec: self, pending: None, output: Vec::new() };
        for chunk in chunks { encoder.call(chunk, Substitute::default(), false); }
        if finish { encoder.call(&[], Substitute::default(), true); }
        encoder.output
    }

}

/// Active lookahead and bytes accumulated by ordinary and recursively invoked encoding.
struct Encoder { codec: MobileSjis, pending: Option<u32>, output: Vec<u8> }

impl Encoder {
    /// Processes one invocation without flushing a marker digit deferred by an inner invocation.
    fn call(&mut self, input: &[u32], substitution: Substitute, end: bool) {
        let mut pending_input = Vec::new();
        let input = if let Some(pending) = self.pending.take() {
            pending_input.push(pending);
            pending_input.extend_from_slice(input);
            pending_input.as_slice()
        } else { input };
        let mut offset = 0;
        while offset < input.len() {
            let code = input[offset];
            let lookahead = code == u32::from(b'#') || (0x30..=0x39).contains(&code)
                || (self.codec.flags && (0x1f1e8..=0x1f1fa).contains(&code));
            if lookahead && offset + 1 == input.len() && !end {
                self.pending = Some(code);
                break;
            }
            if let Some((count, bytes)) = self.codec.base.composite(&input[offset..]) {
                self.output.extend_from_slice(bytes);
                offset += count;
                continue;
            }
            if let Some(bytes) = self.codec.base.encoded(code) {
                self.output.extend_from_slice(bytes);
            } else {
                super::errors::rejected();
                if substitution.mode != SubstituteMode::None {
                    self.call(&substitution.marker(code), substitution.recursive(), false);
                }
            }
            offset += 1;
        }
    }
}
