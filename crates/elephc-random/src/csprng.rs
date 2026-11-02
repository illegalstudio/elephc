//! Purpose:
//! Cryptographically secure byte source shared by the engines and the procedural helpers.
//!
//! Called from:
//! - `crate::Engine::generate` (the `Secure` engine), the global MT19937 seed paths, and
//!   `elephc_random_bytes_hex`.
//!
//! Key details:
//! - `/dev/urandom` is available on every supported target and keeps the crate free of
//!   platform-specific FFI. A missing or unreadable source falls back to a time/address
//!   mixed xorshift so a program never observes an uninitialized seed.

use std::cell::Cell;
use std::io::Read;

thread_local! {
    /// Whether the most recent strict CSPRNG read failed.
    static FAILED: Cell<bool> = const { Cell::new(false) };
}

/// Fills `buffer` from the operating system CSPRNG, returning whether every byte was read.
pub fn fill(buffer: &mut [u8]) -> bool {
    let filled = match std::fs::File::open("/dev/urandom") {
        Ok(mut file) => file.read_exact(buffer).is_ok(),
        Err(_) => false,
    };
    FAILED.with(|flag| flag.set(!filled));
    filled
}

/// Reports and clears whether a strict CSPRNG read has failed since the last query.
pub fn take_failure() -> bool {
    FAILED.with(|flag| flag.replace(false))
}

/// Fills `buffer` from the CSPRNG or, if unavailable, from a non-cryptographic fallback.
pub fn fill_or_fallback(buffer: &mut [u8]) {
    if fill(buffer) {
        return;
    }
    let mut state = fallback_seed();
    for slot in buffer.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *slot = state as u8;
    }
    // A fallback seed keeps the non-cryptographic Mt19937/LCG paths working; it must not
    // make a later strict `Secure`/`random_bytes` read believe the CSPRNG failed.
    FAILED.with(|flag| flag.set(false));
}

/// Builds a mixed seed for the fallback generator from the clock and a stack address.
fn fallback_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15);
    let probe = 0u8;
    let address = &probe as *const u8 as u64;
    nanos ^ address.rotate_left(17) ^ 0xa076_1d64_78bd_642f
}
