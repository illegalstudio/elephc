//! Purpose:
//! Shared pseudo-random word source for eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::math` random builtins.
//! - `crate::interpreter::builtins::array` randomizing builtins.
//!
//! Key details:
//! - `eval_random_u128` is eval-local, process-local, and non-cryptographic; it
//!   backs `rand`, `mt_rand`, `shuffle` and `array_rand`, which PHP itself does not
//!   promise to make cryptographically secure.
//! - `random_int` is different: PHP guarantees a CSPRNG and raises when none is
//!   available, so it goes through `eval_csprng_range`, which draws from the OS
//!   entropy source and rejection-samples to stay unbiased.

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

/// Draws one uint64 from the operating system's CSPRNG.
///
/// PHP's `random_int()` is specified to fail rather than fall back to a weaker
/// source, so an unavailable entropy source becomes a runtime fatal here instead
/// of silently degrading to `eval_random_u128`.
fn eval_csprng_u64() -> Result<u64, EvalStatus> {
    let mut bytes = [0u8; 8];
    getrandom::getrandom(&mut bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(u64::from_le_bytes(bytes))
}

/// Returns a uniform value in `[0, umax]` drawn from the OS CSPRNG.
///
/// Mirrors php-src `php_random_range64` (ext/random/random.c): `UINT64_MAX` is
/// returned unreduced, a power-of-two count is masked, and anything else is
/// rejection-sampled against `UINT64_MAX - (UINT64_MAX % count) - 1`. A plain
/// `draw % count` would be modulo-biased toward the low end of the range, which is
/// exactly what PHP takes care to avoid.
pub(in crate::interpreter) fn eval_csprng_range(umax: u64) -> Result<u64, EvalStatus> {
    if umax == u64::MAX {
        return eval_csprng_u64();
    }
    let count = umax + 1;
    if count.is_power_of_two() {
        return Ok(eval_csprng_u64()? & (count - 1));
    }
    let limit = u64::MAX - (u64::MAX % count) - 1;
    loop {
        let candidate = eval_csprng_u64()?;
        if candidate <= limit {
            return Ok(candidate % count);
        }
    }
}
