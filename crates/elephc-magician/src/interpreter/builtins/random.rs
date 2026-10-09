//! Purpose:
//! Shared pseudo-random word source for eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::math` random builtins.
//! - `crate::interpreter::builtins::array` randomizing builtins.
//!
//! Key details:
//! - This is eval-local, process-local, and non-cryptographic; PHP-visible
//!   builtin owners decide range and key semantics.

use super::super::*;

/// Produces a process-local pseudo-random word for non-cryptographic eval builtins.
pub(in crate::interpreter) fn eval_random_u128() -> u128 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = u128::from(EVAL_RANDOM_COUNTER.fetch_add(1, Ordering::Relaxed));
    let pid = u128::from(std::process::id());
    let mut value = nanos ^ (counter.wrapping_mul(0x9e37_79b9_7f4a_7c15)) ^ (pid << 64);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// php's Mersenne Twister state for eval code, `None` until `mt_srand()` / `srand()` runs.
///
/// It transcribes php-src's `ext/random/engine_mt19937.c` and `php_random_range32/64`, so a
/// seeded `mt_rand()` / `rand()` in eval returns php's sequence. The compiled program keeps its
/// own engine in its runtime (`_mt_state`); the two are seeded independently.
static EVAL_MT19937: std::sync::Mutex<Option<EvalMt19937>> = std::sync::Mutex::new(None);

/// One seeded engine: 624 words, the next-word index, and whether the deprecated
/// `MT_RAND_PHP` variant was requested.
pub(in crate::interpreter) struct EvalMt19937 {
    state: [u32; 624],
    count: usize,
    legacy: bool,
}

impl EvalMt19937 {
    /// `php_random_mt19937_seed32`: Knuth's seeding loop, then one reload.
    fn seeded(seed: u32, legacy: bool) -> Self {
        let mut state = [0u32; 624];
        state[0] = seed;
        for i in 1..624 {
            let prev = state[i - 1];
            state[i] = 1_812_433_253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(i as u32);
        }
        let mut engine = Self { state, count: 0, legacy };
        engine.reload();
        engine
    }

    /// `mt19937_reload`: php's two loops, written as one in-place pass over `k`.
    fn reload(&mut self) {
        for k in 0..624 {
            let u = self.state[k];
            let v = self.state[(k + 1) % 624];
            let m = self.state[(k + 397) % 624];
            let mixed = (u & 0x8000_0000) | (v & 0x7fff_ffff);
            let low_bit = if self.legacy { u & 1 } else { v & 1 };
            self.state[k] = m ^ (mixed >> 1) ^ (0u32.wrapping_sub(low_bit) & 0x9908_b0df);
        }
        self.count = 0;
    }

    /// php's `generate`: the next tempered word.
    fn next_u32(&mut self) -> u32 {
        if self.count >= 624 {
            self.reload();
        }
        let mut s1 = self.state[self.count];
        self.count += 1;
        s1 ^= s1 >> 11;
        s1 ^= (s1 << 7) & 0x9d2c_5680;
        s1 ^= (s1 << 15) & 0xefc6_0000;
        s1 ^ (s1 >> 18)
    }

    /// `php_random_range32` / `php_random_range64` over this engine, for `umax = max - min`.
    fn range(&mut self, umax: u64) -> u64 {
        if umax <= u64::from(u32::MAX) {
            let mut result = self.next_u32();
            if umax == u64::from(u32::MAX) {
                return u64::from(result);
            }
            let bound = umax as u32 + 1;
            if bound & (bound - 1) == 0 {
                return u64::from(result & (bound - 1));
            }
            let limit = u32::MAX - (u32::MAX % bound) - 1;
            while result > limit {
                result = self.next_u32();
            }
            return u64::from(result % bound);
        }
        let draw = |engine: &mut Self| {
            let low = u64::from(engine.next_u32());
            low | (u64::from(engine.next_u32()) << 32)
        };
        let mut result = draw(self);
        if umax == u64::MAX {
            return result;
        }
        let bound = umax + 1;
        if bound & (bound - 1) == 0 {
            return result & (bound - 1);
        }
        let limit = u64::MAX - (u64::MAX % bound) - 1;
        while result > limit {
            result = draw(self);
        }
        result % bound
    }
}

/// Seeds the eval engine, as `mt_srand()` does; `legacy` selects `MT_RAND_PHP`.
pub(in crate::interpreter) fn eval_mt_seed(seed: u32, legacy: bool) {
    if let Ok(mut engine) = EVAL_MT19937.lock() {
        *engine = Some(EvalMt19937::seeded(seed, legacy));
    }
}

/// `mt_rand()` without a range from the seeded engine (`genrand_int31`), or `None` when unseeded.
pub(in crate::interpreter) fn eval_mt_rand_raw() -> Option<i64> {
    let mut engine = EVAL_MT19937.lock().ok()?;
    let engine = engine.as_mut()?;
    Some(i64::from(engine.next_u32() >> 1))
}

/// `php_mt_rand_common(min, max)` for an ordered range from the seeded engine, or `None` when
/// unseeded. The `MT_RAND_PHP` variant scales one 31-bit draw the way php's legacy formula does.
pub(in crate::interpreter) fn eval_mt_rand_range(min: i64, max: i64) -> Option<i64> {
    let mut engine = EVAL_MT19937.lock().ok()?;
    let engine = engine.as_mut()?;
    if engine.legacy {
        let r = f64::from(engine.next_u32() >> 1);
        let offset = ((max as f64 - min as f64 + 1.0) * (r / 2_147_483_648.0)) as u64;
        return Some(offset.wrapping_add(min as u64) as i64);
    }
    let umax = (max as u64).wrapping_sub(min as u64);
    Some(engine.range(umax).wrapping_add(min as u64) as i64)
}
