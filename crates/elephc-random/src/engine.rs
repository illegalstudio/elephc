//! Purpose:
//! Uniform engine value behind the opaque handle the PHP prelude stores in each
//! `Random\Engine\*` object.
//!
//! Called from:
//! - `crate::elephc_random_engine_*` C ABI functions.
//!
//! Key details:
//! - `kind` values match the prelude's private constants: 0 = Mt19937, 1 = Pcg, 2 =
//!   Xoshiro256**, 3 = Secure.
//! - The handle is a boxed `Engine`; `elephc_random_engine_free` is the only owner release
//!   path, and `elephc_random_engine_clone` allocates an independent box.

use crate::csprng;
use crate::mt::{Mt19937, STATE_WORDS};
use crate::pcg::Pcg;
use crate::xoshiro::Xoshiro;

/// Engine kind for `Random\Engine\Mt19937`.
pub const KIND_MT19937: i64 = 0;
/// Engine kind for `Random\Engine\PcgOneseq128XslRr64`.
pub const KIND_PCG: i64 = 1;
/// Engine kind for `Random\Engine\Xoshiro256StarStar`.
pub const KIND_XOSHIRO: i64 = 2;
/// Engine kind for `Random\Engine\Secure`.
pub const KIND_SECURE: i64 = 3;

/// One engine instance owned by a PHP object through an opaque handle.
#[derive(Clone)]
pub enum Engine {
    /// The Mt19937 generator.
    Mt(Mt19937),
    /// The PCG one-stream XSL-RR 64 generator.
    Pcg(Pcg),
    /// The Xoshiro256** generator.
    Xoshiro(Xoshiro),
    /// The operating-system CSPRNG engine.
    Secure,
}

impl Engine {
    /// Builds an engine from its kind, optional integer seed, optional string seed bytes and mode.
    pub fn build(
        kind: i64,
        seed_kind: i64,
        seed_int: i64,
        seed_bytes: Option<&[u8]>,
        mode: u32,
    ) -> Option<Self> {
        match kind {
            KIND_MT19937 => {
                let generator = if seed_kind == 0 {
                    let mut bytes = [0u8; 4];
                    csprng::fill_or_fallback(&mut bytes);
                    Mt19937::seeded(u32::from_le_bytes(bytes), mode)
                } else {
                    Mt19937::seeded(seed_int as u32, mode)
                };
                Some(Self::Mt(generator))
            }
            KIND_PCG => {
                let generator = match seed_bytes {
                    Some(bytes) if bytes.len() == 16 => {
                        Pcg::seeded_bytes(bytes.try_into().expect("16 PCG seed bytes"))
                    }
                    Some(_) => return None,
                    None if seed_kind == 0 => {
                        let mut bytes = [0u8; 16];
                        if !csprng::fill(&mut bytes) {
                            return None;
                        }
                        Pcg::seeded_bytes(bytes)
                    }
                    None => Pcg::seeded_int(seed_int as u64),
                };
                Some(Self::Pcg(generator))
            }
            KIND_XOSHIRO => {
                let generator = match seed_bytes {
                    Some(bytes) if bytes.len() == 32 => {
                        Xoshiro::seeded_bytes(bytes.try_into().expect("32 xoshiro seed bytes"))
                    }
                    Some(_) => return None,
                    None if seed_kind == 0 => {
                        loop {
                            let mut bytes = [0u8; 32];
                            if !csprng::fill(&mut bytes) {
                                return None;
                            }
                            let candidate = Xoshiro::seeded_bytes(bytes);
                            if candidate.state != [0; 4] {
                                break candidate;
                            }
                        }
                    }
                    None => Xoshiro::seeded_int(seed_int as u64),
                };
                Some(Self::Xoshiro(generator))
            }
            KIND_SECURE => Some(Self::Secure),
            _ => None,
        }
    }

    /// Produces the raw generated value; the caller reconstructs its little-endian bytes.
    pub fn generate(&mut self) -> u64 {
        match self {
            Self::Mt(generator) => generator.generate() as u64,
            Self::Pcg(generator) => generator.generate(),
            Self::Xoshiro(generator) => generator.generate(),
            Self::Secure => {
                let mut bytes = [0u8; 8];
                if !csprng::fill(&mut bytes) {
                    return 0;
                }
                u64::from_le_bytes(bytes)
            }
        }
    }

    /// Reports the number of bytes `generate()` contributes (four for Mt19937, eight otherwise).
    pub fn size(&self) -> usize {
        match self {
            Self::Mt(_) => 4,
            _ => 8,
        }
    }

    /// Reports the Mt19937 draw cursor; zero for engines without one.
    pub fn count(&self) -> i64 {
        match self {
            Self::Mt(generator) => generator.count as i64,
            _ => 0,
        }
    }

    /// Reports the Mt19937 mode; zero for engines without one.
    pub fn mode(&self) -> i64 {
        match self {
            Self::Mt(generator) => generator.mode as i64,
            _ => 0,
        }
    }

    /// Serializes the engine state as concatenated little-endian word hex.
    pub fn state_hex(&self) -> String {
        match self {
            Self::Mt(generator) => generator.state_hex(),
            Self::Pcg(generator) => generator.state_hex(),
            Self::Xoshiro(generator) => generator.state_hex(),
            Self::Secure => String::new(),
        }
    }

    /// Restores the engine state from php-src's serialized fields.
    pub fn restore_hex(&mut self, text: &str, count: i64, mode: i64) -> bool {
        match self {
            Self::Mt(generator) => {
                if count < 0 || count > STATE_WORDS as i64 {
                    return false;
                }
                if mode != 0 && mode != 1 {
                    return false;
                }
                if text.len() != STATE_WORDS * 8 {
                    return false;
                }
                let Some(bytes) = crate::hex::decode(text) else {
                    return false;
                };
                for (index, slot) in generator.state.iter_mut().enumerate() {
                    *slot = u32::from_le_bytes(
                        bytes[index * 4..index * 4 + 4]
                            .try_into()
                            .expect("4 state bytes"),
                    );
                }
                generator.count = count as u32;
                generator.mode = mode as u32;
                true
            }
            Self::Pcg(generator) => generator.restore_hex(text),
            Self::Xoshiro(generator) => generator.restore_hex(text),
            Self::Secure => false,
        }
    }

    /// Advances a PCG engine by `advance` steps.
    pub fn jump(&mut self, advance: u64) {
        if let Self::Pcg(generator) = self {
            generator.jump(advance);
        }
    }

    /// Advances a xoshiro engine by the `jumpLong()` distance.
    pub fn jump_long(&mut self) {
        if let Self::Xoshiro(generator) = self {
            generator.jump_long();
        }
    }

    /// Advances a xoshiro engine by the `jump()` distance.
    pub fn jump_short(&mut self) {
        if let Self::Xoshiro(generator) = self {
            generator.jump();
        }
    }
}
