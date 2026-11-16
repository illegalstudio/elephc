//! Purpose:
//! PHP's Mt19937 engine, including both the standard and the historical `MT_RAND_PHP`
//! twist variants, plus the unbiased 32/64-bit range samplers php-src layers on it.
//!
//! Called from:
//! - `crate::Engine` for `Random\Engine\Mt19937` and the global procedural random state.
//!
//! Key details:
//! - The state layout, seed loop, twist loop, tempering and range rejection are transcribed
//!   from php-src `ext/random/engine_mt19937.c` and `ext/random/random.c`. The two twist
//!   variants differ only in whether the low bit is read from `u` or `v`; that single bit is
//!   the documented PHP bug the `MT_RAND_PHP` mode preserves.
//! - `seed32` immediately twists the state and leaves `count` at zero, exactly like
//!   `php_random_mt19937_seed32`, which is observable through `serialize()`.

use crate::csprng;

/// Length of the MT19937 state vector.
pub const STATE_WORDS: usize = 624;
/// Period parameter separating the two twist halves.
const M: usize = 397;
/// Twisting matrix constant.
const MATRIX_A: u32 = 0x9908_b0df;

/// `MT_RAND_MT19937`: the standard, unbiased variant.
pub const MODE_MT19937: u32 = 0;
/// `MT_RAND_PHP`: PHP's historical biased variant.
pub const MODE_PHP: u32 = 1;
/// Number of rejection attempts before php-src gives up.
pub const RANGE_ATTEMPTS: u32 = 50;

/// One Mt19937 generator instance.
#[derive(Clone)]
pub struct Mt19937 {
    /// The 624-word state vector.
    pub state: [u32; STATE_WORDS],
    /// Index of the next word to temper.
    pub count: u32,
    /// `MODE_MT19937` or `MODE_PHP`.
    pub mode: u32,
}

impl Mt19937 {
    /// Creates a generator seeded from `seed` in the requested mode.
    pub fn seeded(seed: u32, mode: u32) -> Self {
        let mut generator = Self {
            state: [0; STATE_WORDS],
            count: 0,
            mode,
        };
        generator.seed32(seed);
        generator
    }

    /// Creates a generator seeded from the operating system CSPRNG.
    pub fn seeded_random(mode: u32) -> Self {
        let mut bytes = [0u8; 4];
        csprng::fill_or_fallback(&mut bytes);
        Self::seeded(u32::from_le_bytes(bytes), mode)
    }

    /// Initializes the state vector from one 32-bit seed and twists it once.
    pub fn seed32(&mut self, seed: u32) {
        self.state[0] = seed;
        for index in 1..STATE_WORDS {
            let previous = self.state[index - 1];
            self.state[index] = 1_812_433_253u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(index as u32);
        }
        self.count = STATE_WORDS as u32;
        self.reload();
    }

    /// Regenerates the whole state vector and resets the draw cursor.
    pub fn reload(&mut self) {
        let mode = self.mode;
        let state = &mut self.state;
        let mut position = 0usize;
        for _ in 0..(STATE_WORDS - M) {
            state[position] = twist(
                state[position + M],
                state[position],
                state[position + 1],
                mode,
            );
            position += 1;
        }
        for _ in 0..(M - 1) {
            state[position] = twist(
                state[position + M - STATE_WORDS],
                state[position],
                state[position + 1],
                mode,
            );
            position += 1;
        }
        state[position] = twist(
            state[position + M - STATE_WORDS],
            state[position],
            state[0],
            mode,
        );
        self.count = 0;
    }

    /// Draws one tempered 32-bit value.
    pub fn generate(&mut self) -> u32 {
        if self.count >= STATE_WORDS as u32 {
            self.reload();
        }
        let index = self.count as usize;
        self.count += 1;
        let mut value = self.state[index];
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^ (value >> 18)
    }

    /// Draws one 31-bit value, matching `php_mt_rand() >> 1`.
    pub fn generate31(&mut self) -> u32 {
        self.generate() >> 1
    }

    /// Samples `[0, umax]` with php-src's 32-bit rejection sampler.
    pub fn range32(&mut self, umax: u32) -> u32 {
        let mut result = 0u32;
        let mut total_size = 0usize;
        while total_size < 4 {
            result |= self.generate() << (total_size * 8);
            total_size += 4;
        }

        if umax == u32::MAX {
            return result;
        }

        let count = umax + 1;
        if count.is_power_of_two() {
            return result & (count - 1);
        }

        let limit = u32::MAX - (u32::MAX % count) - 1;
        let mut attempts = 0u32;
        while result > limit {
            attempts += 1;
            if attempts > RANGE_ATTEMPTS {
                return result % count;
            }
            result = 0;
            total_size = 0;
            while total_size < 4 {
                result |= self.generate() << (total_size * 8);
                total_size += 4;
            }
        }

        result % count
    }

    /// Samples `[0, umax]` with php-src's 64-bit rejection sampler.
    pub fn range64(&mut self, umax: u64) -> u64 {
        let mut result = 0u64;
        let mut total_size = 0usize;
        while total_size < 8 {
            result |= (self.generate() as u64) << (total_size * 8);
            total_size += 4;
        }

        if umax == u64::MAX {
            return result;
        }

        let count = umax + 1;
        if count.is_power_of_two() {
            return result & (count - 1);
        }

        let limit = u64::MAX - (u64::MAX % count) - 1;
        let mut attempts = 0u32;
        while result > limit {
            attempts += 1;
            if attempts > RANGE_ATTEMPTS {
                return result % count;
            }
            result = 0;
            total_size = 0;
            while total_size < 8 {
                result |= (self.generate() as u64) << (total_size * 8);
                total_size += 4;
            }
        }

        result % count
    }

    /// Samples an inclusive signed range with php-src's `php_random_range` dispatch.
    pub fn range(&mut self, min: i64, max: i64) -> i64 {
        let umax = (max as u64).wrapping_sub(min as u64);
        if umax > u32::MAX as u64 {
            (self.range64(umax).wrapping_add(min as u64)) as i64
        } else {
            ((self.range32(umax as u32) as u64).wrapping_add(min as u64)) as i64
        }
    }

    /// Implements `php_mt_rand_common`, including the PHP-mode bad-scaling formula.
    pub fn rand_common(&mut self, min: i64, max: i64) -> i64 {
        if self.mode == MODE_MT19937 {
            return self.range(min, max);
        }
        let draw = self.generate31();
        let offset = (max as f64 - min as f64 + 1.0) * (draw as f64 / 2_147_483_648.0);
        (offset + min as f64) as i64
    }

    /// Serializes the 624 state words in php-src's little-endian hex form.
    pub fn state_hex(&self) -> String {
        let mut text = String::with_capacity(STATE_WORDS * 8);
        for word in self.state {
            text.push_str(&crate::hex::encode_u32(word));
        }
        text
    }
}

/// Applies one twist step with the mode-selected low-bit source.
fn twist(matrix: u32, upper: u32, lower: u32, mode: u32) -> u32 {
    let mixed = (upper & 0x8000_0000) | (lower & 0x7fff_ffff);
    let low_bit = if mode == MODE_PHP { upper } else { lower } & 1;
    matrix ^ (mixed >> 1) ^ (low_bit.wrapping_neg() & MATRIX_A)
}
