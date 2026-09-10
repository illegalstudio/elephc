//! Purpose:
//! Shares automatic source selection for mb_convert_encoding's string and container adapters.
//!
//! Called from:
//! - Request-state source-list preparation and per-string conversion in backend adapters.
//!
//! Key details:
//! - A single explicit source bypasses detection and may be a transfer encoding.
//! - Multiple sources exclude non-text encodings before detecting each key or value.
//! - The caller emits PHP's detection-failure warning when conversion returns None.

use crate::encoding::{Encoding, Substitute};
use crate::error::{MbError, MbResult};

/// A validated nonempty source list, ready to be reused for each string in a container.
pub struct ConversionSources(Vec<Encoding>);

impl ConversionSources {
    /// Applies conversion's transfer-filter rule and rejects an empty resulting source list.
    pub fn new(mut encodings: Vec<Encoding>) -> MbResult<Self> {
        if encodings.len() > 1 { encodings.retain(|encoding| encoding.supports_detection()); }
        if encodings.is_empty() {
            return Err(MbError::argument("mb_convert_encoding", 3, "from_encoding", "must specify at least one encoding"));
        }
        Ok(Self(encodings))
    }

    /// Selects the explicit source directly or invokes the shared weighted detector.
    pub fn select(&self, input: &[u8], strict: bool) -> Option<Encoding> {
        if self.0.len() == 1 { Some(self.0[0]) }
        else { crate::detect::guess(input, &self.0, strict, true) }
    }

    /// Converts one string using its detected source and the common codec replacement policy.
    pub fn convert(&self, input: &[u8], to: Encoding, strict: bool, substitution: Substitute) -> Option<Vec<u8>> {
        self.select(input, strict).map(|from| super::convert_encoding(input, from, to, substitution))
    }
}
