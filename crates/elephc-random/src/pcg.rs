//! Purpose:
//! PHP's PCG 128-bit one-stream XSL-RR 64-bit engine.
//!
//! Called from:
//! - `crate::Engine` for `Random\Engine\PcgOneseq128XslRr64`.
//!
//! Key details:
//! - The state is a single `u128`; the multiplier, increment and jump exponentiation
//!   constants are transcribed from php-src `ext/random/engine_pcgoneseq128xslrr64.c`.
//! - `seed128` performs the two-step constant/seed/add sequence php-src uses, which is
//!   observable through the serialized state.

/// PCG multiplier constant as `(high, low)`.
const MULTIPLIER: u128 = ((2_549_297_995_355_413_924u128) << 64) | 486_554_059_571_442_2341u128;
/// PCG increment constant as `(high, low)`.
const INCREMENT: u128 = ((6_364_136_223_846_793_005u128) << 64) | 1_442_695_040_888_963_407u128;

/// One PCG one-stream XSL-RR 64-bit generator.
#[derive(Clone)]
pub struct Pcg {
    /// The 128-bit LCG state.
    pub state: u128,
}

impl Pcg {
    /// Builds the state from a 128-bit seed.
    pub fn seeded(seed: u128) -> Self {
        let mut generator = Self { state: 0 };
        generator.seed128(seed);
        generator
    }

    /// Builds the state from a 64-bit integer seed, matching `constant(0, (uint64) seed)`.
    pub fn seeded_int(seed: u64) -> Self {
        Self::seeded(seed as u128)
    }

    /// Builds the state from 16 raw seed bytes, little-endian per 8-byte half.
    pub fn seeded_bytes(bytes: [u8; 16]) -> Self {
        let high = u64::from_le_bytes(bytes[0..8].try_into().expect("8 high bytes"));
        let low = u64::from_le_bytes(bytes[8..16].try_into().expect("8 low bytes"));
        Self::seeded(((high as u128) << 64) | low as u128)
    }

    /// Runs php-src's `seed128` sequence.
    pub fn seed128(&mut self, seed: u128) {
        self.state = 0;
        self.step();
        self.state = self.state.wrapping_add(seed);
        self.step();
    }

    /// Advances the LCG by one step.
    pub fn step(&mut self) {
        self.state = self
            .state
            .wrapping_mul(MULTIPLIER)
            .wrapping_add(INCREMENT);
    }

    /// Produces the XSL-RR 64-bit output after advancing one step.
    pub fn generate(&mut self) -> u64 {
        self.step();
        let high = (self.state >> 64) as u64;
        let low = self.state as u64;
        let value = high ^ low;
        let rotation = (high >> 58) as u32;
        value.rotate_right(rotation)
    }

    /// Advances the LCG by `advance` steps in logarithmic time.
    pub fn jump(&mut self, advance: u64) {
        let mut advance = advance;
        let mut current_multiplier = MULTIPLIER;
        let mut current_plus = INCREMENT;
        let mut accumulator_multiplier: u128 = 1;
        let mut accumulator_plus: u128 = 0;

        while advance > 0 {
            if advance & 1 == 1 {
                accumulator_multiplier = accumulator_multiplier.wrapping_mul(current_multiplier);
                accumulator_plus = accumulator_plus
                    .wrapping_mul(current_multiplier)
                    .wrapping_add(current_plus);
            }
            current_plus = current_multiplier
                .wrapping_add(1)
                .wrapping_mul(current_plus);
            current_multiplier = current_multiplier.wrapping_mul(current_multiplier);
            advance /= 2;
        }

        self.state = accumulator_multiplier
            .wrapping_mul(self.state)
            .wrapping_add(accumulator_plus);
    }

    /// Serializes the state as php-src does: high 64 bits, then low 64 bits.
    pub fn state_hex(&self) -> String {
        let high = (self.state >> 64) as u64;
        let low = self.state as u64;
        let mut text = crate::hex::encode_u64(high);
        text.push_str(&crate::hex::encode_u64(low));
        text
    }

    /// Restores the state from php-src's two little-endian 64-bit hex fields.
    pub fn restore_hex(&mut self, text: &str) -> bool {
        if text.len() != 32 {
            return false;
        }
        let Some(bytes) = crate::hex::decode(text) else {
            return false;
        };
        let high = u64::from_le_bytes(bytes[0..8].try_into().expect("8 high bytes"));
        let low = u64::from_le_bytes(bytes[8..16].try_into().expect("8 low bytes"));
        self.state = ((high as u128) << 64) | low as u128;
        true
    }
}
