//! Purpose:
//! Emits JIS scalar mappings with continuous shift state and PHP's legacy cut exceptions.
//!
//! Called from:
//! - `super::Jis::encode` and the streaming cut adapter.
//!
//! Key details:
//! - JIS modern replacement calls use the ISO-2022-JP mapping policy.
//! - Legacy JIS emits halfwidth kana through its historical two-byte path.
//! - Legacy ISO-2022-JP silently drops unmappable characters.
//! - CP50220 and legacy mobile JIS retain one pending character for contextual conversion.

use crate::encoding::{mapping::{lookup, word}, Substitute, BAD_INPUT};
use crate::unicode::kana::fullwidth_katakana;
use super::{Jis, Mode, Variant};

/// Retains the current output character mode and canonical-versus-legacy encoder policy.
#[derive(Clone, Copy)]
pub(super) struct Encoder {
    codec: Jis,
    mode: Mode,
    legacy: bool,
    pending: Option<(u32, Substitute)>,
}

impl Encoder {
    /// Starts an ASCII-mode encoder for either modern conversion or legacy streaming cuts.
    pub(super) fn new(codec: Jis, legacy: bool) -> Self {
        Self { codec, mode: Mode::Ascii, legacy, pending: None }
    }

    /// Emits one character and applies the appropriate callback policy to unrepresentable input.
    pub(super) fn append(&mut self, code: u32, substitute: Substitute, output: &mut Vec<u8>) {
        if self.legacy && self.codec.variant == Variant::Kddi {
            if let Some((previous, settings)) = self.pending.take() {
                if let Some(mapped) = self.codec.composite(previous, code) {
                    self.mapped(mapped, output);
                    return;
                }
                self.append_mapped(previous, settings, output);
            }
            if matches!(code, 0x23 | 0x30..=0x39) {
                self.pending = Some((code, substitute));
                return;
            }
        }
        if self.codec.variant == Variant::Cp50220 {
            if let Some((previous, settings)) = self.pending.take() {
                let (mapped, consumed) = fullwidth_katakana(previous, code);
                self.append_mapped(mapped, settings, output);
                if consumed { return; }
            }
            if self.legacy && code == 0 { output.push(0); return; }
            if self.legacy || (0xff61..=0xff9f).contains(&code) {
                self.pending = Some((code, substitute));
                return;
            }
        }
        self.append_mapped(code, substitute, output);
    }

    /// Encodes a normalized scalar, bypassing contextual transforms for replacement characters.
    fn append_mapped(&mut self, code: u32, substitute: Substitute, output: &mut Vec<u8>) {
        if !self.push(code, self.codec.encode, output) && !(self.legacy && self.codec.variant == Variant::Iso2022) {
            let errors = if self.legacy { self.codec.encode } else { self.codec.error_encode };
            substitute.append(code, output, |code, output| self.push(code, errors, output));
        }
    }

    /// Emits a captured scalar mapping without changing state if no mapping exists.
    fn push(&mut self, code: u32, table: &[u8], output: &mut Vec<u8>) -> bool {
        let mapped = if code <= 0xffff { word(table, code as usize * 4) }
            else { lookup(self.codec.supplementary, code).unwrap_or(BAD_INPUT) };
        if mapped == BAD_INPUT { return false; }
        self.mapped(mapped, output);
        true
    }

    /// Emits an already selected canonical scalar or composite mapping in the shared shift state.
    pub(super) fn mapped(&mut self, mapped: u32, output: &mut Vec<u8>) {
        let mut mode = match mapped >> 16 {
            0 => Mode::Ascii,
            1 => Mode::Roman,
            2 => Mode::Kana,
            3 => Mode::Kanji,
            4 => Mode::Plane212,
            5 => Mode::User,
            _ => unreachable!("generated JIS mode"),
        };
        let payload = mapped as u16;
        if self.legacy && self.codec.variant == Variant::Jis && mode == Mode::Kana { mode = Mode::Kanji; }
        self.shift(mode, output);
        if matches!(mode, Mode::Kanji | Mode::Plane212 | Mode::User) { output.push((payload >> 8) as u8); }
        output.push(payload as u8);
    }

    /// Changes output modes only when required by the next captured character mapping.
    fn shift(&mut self, mode: Mode, output: &mut Vec<u8>) {
        if self.mode == mode { return; }
        if self.codec.variant == Variant::Cp50222 {
            if self.mode == Mode::Kana { output.push(0x0f); self.mode = Mode::Ascii; }
            if mode == Mode::Kana { output.push(0x0e); self.mode = mode; return; }
            if self.mode == mode { return; }
        }
        output.extend_from_slice(match mode {
            Mode::Ascii => b"\x1b(B".as_slice(),
            Mode::Roman => b"\x1b(J",
            Mode::Kana | Mode::KanaSo => b"\x1b(I",
            Mode::Kanji => b"\x1b$B",
            Mode::Plane212 => b"\x1b$(D",
            Mode::User => b"\x1b$(?",
            Mode::Unknown(_) => unreachable!("encoder cannot select an unknown decoder state"),
        });
        self.mode = mode;
    }

    /// Finishes a string in ASCII mode, matching PHP's canonical closing escape.
    pub(super) fn close(&mut self, output: &mut Vec<u8>) {
        if let Some((code, settings)) = self.pending.take() {
            let mapped = if self.codec.variant == Variant::Cp50220 { fullwidth_katakana(code, 0).0 } else { code };
            self.append_mapped(mapped, settings, output);
        }
        self.shift(Mode::Ascii, output);
    }

    /// Finalizes a successful legacy cut with PHP's final-flush error suppression.
    pub(super) fn close_cut(&mut self, output: &mut Vec<u8>) {
        if let Some((_, settings)) = self.pending.as_mut() { settings.mode = crate::encoding::SubstituteMode::None; }
        self.close(output);
    }
}
