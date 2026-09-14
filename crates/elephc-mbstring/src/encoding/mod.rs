//! Purpose:
//! Owns byte decoding, byte offsets, and substitution policies for mbstring.
//!
//! Called from:
//! - Shared string operations before and after Unicode transformations.
//!
//! Key details:
//! - Invalid decoder units retain a sentinel until the destination encoder substitutes.
//! - Character starts are recorded separately from PHP's optimized length calculation.

mod catalog;
mod catalog_data;
mod list;
mod doublebyte;
mod mac;
mod gb18030;
mod euctw;
mod hz;
mod iso2022kr;
mod jis;
mod jis2004;
mod mapping;
pub(crate) mod errors;
mod mobile_utf8;
mod mobile_sjis;
mod singlebyte;
mod unicode;
mod utf7;
mod utf16_batches;
mod transfer;

pub use catalog::{Encoding, Slicing};
pub use list::{parse_encoding_list, EncodingList, EncodingListBuilder};
pub use unicode::UnicodeEncoding;

/// Decoder sentinel that represents an invalid byte sequence, as in libmbfl.
pub const BAD_INPUT: u32 = u32::MAX;

/// Decoded characters and the source byte offset at which each character starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decoded {
    /// Unicode codepoints, UCS raw values, or [`BAD_INPUT`].
    pub points: Vec<u32>,
    /// One byte offset per decoded point; BOM bytes do not produce a point.
    pub offsets: Vec<usize>,
    /// Decoder batch ends for the requested scratch capacity, normally 64 words for casing.
    pub(crate) case_batches: Vec<usize>,
    /// Noncanonical input rejected by validation even when conversion accepts its bytes.
    pub(super) validation_error: bool,
}

impl Decoded {
    /// Appends one decoded unit together with its original byte offset.
    pub(super) fn push(&mut self, code: u32, offset: usize) {
        self.points.push(code);
        self.offsets.push(offset);
    }

    /// Reports whether the decoder encountered an invalid byte sequence.
    pub fn is_valid(&self) -> bool {
        !self.validation_error && !self.points.contains(&BAD_INPUT)
    }
}

/// PHP's replacement policy for invalid input and unrepresentable output characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubstituteMode {
    Character,
    None,
    Long,
    Entity,
}

/// Replacement settings, including the remembered character used by long/entity modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Substitute {
    /// Active replacement strategy selected by `mb_substitute_character()`.
    pub mode: SubstituteMode,
    /// Remembered scalar replacement, also used for malformed input in long/entity modes.
    pub character: u32,
}

impl Default for Substitute {
    /// Starts with PHP's default question-mark replacement.
    fn default() -> Self {
        Self { mode: SubstituteMode::Character, character: b'?' as u32 }
    }
}

impl Substitute {
    /// Builds the codepoint marker passed recursively to a stateful output encoder.
    pub(super) fn marker(self, code: u32) -> Vec<u32> {
        match self.mode {
            SubstituteMode::None => Vec::new(),
            SubstituteMode::Long if code != BAD_INPUT => format!("U+{code:X}").bytes().map(u32::from).collect(),
            SubstituteMode::Entity if code != BAD_INPUT => format!("&#x{code:X};").bytes().map(u32::from).collect(),
            _ => vec![self.character],
        }
    }

    /// Prevents replacement recursion while retaining PHP's character-mode question-mark fallback.
    pub(super) fn recursive(self) -> Self {
        if self.mode == SubstituteMode::Character && self.character != u32::from(b'?') {
            Self::default()
        } else { Self { mode: SubstituteMode::None, ..self } }
    }

    /// Replaces a rejected unit without recursively applying the replacement policy.
    pub(super) fn append(&self, code: u32, output: &mut Vec<u8>, mut encode: impl FnMut(u32, &mut Vec<u8>) -> bool) {
        errors::rejected();
        for point in self.marker(code) {
            if !encode(point, output) {
                errors::rejected();
                if self.mode == SubstituteMode::Character && self.character != b'?' as u32
                    && !encode(b'?' as u32, output) {
                    errors::rejected();
                }
            }
        }
    }
}
