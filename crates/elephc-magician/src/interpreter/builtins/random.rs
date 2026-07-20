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
