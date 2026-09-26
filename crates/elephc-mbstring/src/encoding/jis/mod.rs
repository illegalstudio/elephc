//! Purpose:
//! Shares JIS and ISO-2022-JP character planes, shift modes, validation, and encoder mappings.
//!
//! Called from:
//! - The canonical mbstring encoding catalog.
//!
//! Key details:
//! - Decoder variants share Japanese planes while retaining separate validation policies.
//! - Canonical encoder maps and legacy cut behavior remain distinct where PHP differs.

mod decoder;
mod encoder;
mod cut;
mod microsoft;
mod mobile;

use super::{doublebyte::DoubleByte, mapping::word, Decoded, Substitute, BAD_INPUT};

/// Shared Japanese planes plus each public encoding's canonical scalar output mapping.
#[derive(Clone, Copy, Debug)]
pub(super) struct Jis {
    pub base: DoubleByte,
    pub encode: &'static [u8],
    pub error_encode: &'static [u8],
    pub supplementary: &'static [u8],
    pub composites: &'static [u8],
    pub variant: Variant,
}

/// Selects the PHP validation, decoder, and legacy encoder rules for one public variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Variant {
    Jis,
    Iso2022,
    Microsoft,
    Cp50220,
    Cp50221,
    Cp50222,
    Kddi,
}

impl Variant {
    /// Identifies the shared CP932-extended plane and permissive CP5022x decoder policy.
    fn is_cp5022x(self) -> bool {
        matches!(self, Self::Cp50220 | Self::Cp50221 | Self::Cp50222)
    }
}

/// Character modes used by conversion and stricter JIS validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Ascii,
    Roman,
    Kana,
    Kanji,
    Plane212,
    User,
    KanaSo,
    Unknown(u32),
}

impl Mode {
    /// Interprets PHP's shared decoder-state word using the current JIS variant.
    fn from_state(state: u32, variant: Variant) -> Self {
        if matches!(variant, Variant::Microsoft | Variant::Kddi) {
            match state { 0 => Self::Ascii, 0x20 => Self::Kana, 0x80 => Self::Kanji,
                0xa0 if variant == Variant::Microsoft => Self::User, other => Self::Unknown(other) }
        } else {
            match state { 0 => Self::Ascii, 1 => Self::Roman, 2 => Self::Kana,
                3 => Self::Kanji, 4 => Self::Plane212, other => Self::Unknown(other) }
        }
    }

    /// Preserves the integer state for a subsequent word, which may select another codec.
    fn to_state(self, variant: Variant) -> u32 {
        if matches!(variant, Variant::Microsoft | Variant::Kddi) {
            match self { Self::Ascii => 0, Self::Kana | Self::KanaSo => 0x20,
                Self::Kanji => 0x80, Self::User => 0xa0, Self::Unknown(value) => value,
                _ => unreachable!("mobile JIS decoder state") }
        } else {
            match self { Self::Ascii => 0, Self::Roman => 1, Self::Kana | Self::KanaSo => 2,
                Self::Kanji => 3, Self::Plane212 => 4, Self::Unknown(value) => value,
                Self::User => unreachable!("ordinary JIS decoder state") }
        }
    }
}

impl Jis {
    /// Decodes characters and records validation failures and contextual conversion batches.
    pub fn decode(self, input: &[u8]) -> Decoded {
        if self.variant == Variant::Microsoft { microsoft::decode(input, self) }
        else if self.variant == Variant::Kddi { mobile::decode(input) }
        else { decoder::decode(input, self) }
    }

    /// Retains explicit decoder partitions for consumers whose buffer differs from casing.
    pub fn decode_buffer(self, input: &[u8], capacity: usize) -> Decoded {
        if self.variant == Variant::Kddi { mobile::decode_buffer(input, capacity) }
        else if self.variant == Variant::Microsoft { microsoft::decode(input, self) }
        else { decoder::decode_buffer(input, self, capacity) }
    }

    /// Returns the first bounded source batch with its precise input position and shift state.
    pub fn decode_next(self, input: &[u8], capacity: usize, state: &mut u32) -> (Decoded, usize) {
        if self.variant == Variant::Kddi { mobile::decode_next(input, capacity, state) }
        else if self.variant == Variant::Microsoft { microsoft::decode_next(input, self, capacity, state) }
        else { decoder::decode_next(input, self, capacity, state) }
    }

    /// Encodes through the PHP-selected character mode, keeping replacement state continuous.
    pub fn encode(self, input: &[u32], substitute: Substitute) -> Vec<u8> {
        self.encode_chunks(std::iter::once(input), substitute)
    }

    /// Keeps shift state between chunks while limiting KDDI emoji composition to each chunk.
    pub fn encode_chunks<'a>(self, chunks: impl Iterator<Item = &'a [u32]>, substitute: Substitute) -> Vec<u8> {
        self.encode_prefix(chunks, substitute, true)
    }

    /// Preserves per-call composition while optionally omitting the provisional final shift reset.
    pub fn encode_prefix<'a>(self, chunks: impl Iterator<Item = &'a [u32]>, substitute: Substitute, finish: bool) -> Vec<u8> {
        let mut encoder = encoder::Encoder::new(self, false);
        let mut output = Vec::new();
        for input in chunks {
            let mut offset = 0;
            while offset < input.len() {
                if self.variant == Variant::Kddi && offset + 1 < input.len() {
                    if let Some(mapped) = self.composite(input[offset], input[offset + 1]) {
                        encoder.mapped(mapped, &mut output);
                        offset += 2;
                        continue;
                    }
                }
                encoder.append(input[offset], substitute, &mut output);
                offset += 1;
            }
        }
        if finish { encoder.close(&mut output); }
        output
    }

    /// Reports a transport whose modern encoder does not preserve emoji lookahead across chunks.
    pub fn needs_encoder_batches(self) -> bool { self.variant == Variant::Kddi }

    /// Cuts bytes using the legacy decoder/encoder pair required by mb_strcut.
    pub fn cut(self, input: &[u8], from: usize, length: usize) -> Vec<u8> {
        if self.variant == Variant::Microsoft { microsoft::cut(input, from, length, self) }
        else if self.variant == Variant::Kddi { mobile::cut(input, from, length, self) }
        else { cut::cut(input, from, length, self) }
    }

    /// Reads a JIS plane character from shared EUC-JP maps without re-decoding control syntax.
    fn pair(self, mode: Mode, first: u8, second: u8) -> u32 {
        if !(0x21..=0x7e).contains(&second) { return BAD_INPUT; }
        if self.variant.is_cp5022x() {
            if mode == Mode::Kanji && (0x21..=0x97).contains(&first) {
                let offset = (usize::from(first - 0x21) * 94 + usize::from(second - 0x21)) * 4;
                return word(include_bytes!("../data/cp5022x-plane.bin"), offset);
            }
            if first > 0x7e { return BAD_INPUT; }
        }
        match mode {
            Mode::Kanji => self.base.decoded_pair(first | 0x80, second | 0x80),
            Mode::Plane212 | Mode::Unknown(_) => self.base.decoded_triple(0x8f, first | 0x80, second | 0x80),
            Mode::User if (0x21..=0x34).contains(&first) =>
                0xe000 + u32::from(first - 0x21) * 94 + u32::from(second - 0x21),
            _ => BAD_INPUT,
        }
    }

    /// Returns the canonical mobile mapping for a captured two-codepoint emoji composition.
    fn composite(self, first: u32, second: u32) -> Option<u32> {
        (0..self.composites.len()).step_by(12).find_map(|offset| {
            (word(self.composites, offset) == first && word(self.composites, offset + 4) == second)
                .then(|| word(self.composites, offset + 8))
        })
    }
}
