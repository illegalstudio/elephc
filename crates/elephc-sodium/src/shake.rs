//! Purpose:
//! Keccak sponge state for libsodium's SHAKE128/SHAKE256 XOF surface.
//!
//! Called from:
//! - `crate::` C ABI wrappers for `sodium_crypto_xof_shake*` and their state
//!   objects.
//!
//! Key details:
//! - The sponge is Keccak-f[1600] with 24 rounds, rate 168 (SHAKE128) or 136
//!   (SHAKE256), and libsodium's domain byte (0x1f by default).
//! - Squeezing is streaming: successive `squeeze` calls continue from the
//!   current offset, and `update` after a squeeze is rejected like libsodium's
//!   `crypto_xof_*_update` returning -1.
//! - State handles are process-wide and removed by the state object's
//!   destructor or by `close`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use keccak::p1600;

/// SHAKE128 rate in bytes (`crypto_xof_shake128_BLOCKBYTES`).
pub(crate) const SHAKE128_RATE: usize = 168;
/// SHAKE256 rate in bytes (`crypto_xof_shake256_BLOCKBYTES`).
pub(crate) const SHAKE256_RATE: usize = 136;
/// libsodium's standard SHAKE domain byte.
pub(crate) const DOMAIN_STANDARD: u8 = 0x1f;

/// Which SHKE variant a state belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShakeKind {
    /// SHAKE128, rate 168, 24 rounds.
    Shake128,
    /// SHAKE256, rate 136, 24 rounds.
    Shake256,
    /// TurboSHAKE128, rate 168, 12 rounds.
    TurboShake128,
    /// TurboSHAKE256, rate 136, 12 rounds.
    TurboShake256,
}

impl ShakeKind {
    /// Returns the sponge rate for this variant.
    fn rate(self) -> usize {
        match self {
            Self::Shake128 | Self::TurboShake128 => SHAKE128_RATE,
            Self::Shake256 | Self::TurboShake256 => SHAKE256_RATE,
        }
    }

    /// Returns the Keccak-p round count for this variant.
    fn rounds(self) -> usize {
        match self {
            Self::Shake128 | Self::Shake256 => 24,
            Self::TurboShake128 | Self::TurboShake256 => 12,
        }
    }
}

/// Sponge phase; updates after squeezing are rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    /// Absorbing input bytes.
    Absorbing,
    /// Emitting output bytes.
    Squeezing,
}

/// One incremental Keccak sponge state.
#[derive(Clone, Debug)]
pub(crate) struct Sponge {
    /// Keccak-f[1600] lane array.
    state: [u64; 25],
    /// Current byte offset inside the rate block.
    offset: usize,
    /// Absorbing or squeezing.
    phase: Phase,
    /// Domain separation byte applied during padding.
    domain: u8,
    /// Rate in bytes.
    rate: usize,
    /// Keccak-p round count (24 for SHAKE, 12 for TurboSHAKE).
    rounds: usize,
}

impl Sponge {
    /// Starts a SHAKE128 sponge with `domain`.
    pub(crate) fn new_128(domain: u8) -> Self {
        Self::new(ShakeKind::Shake128, domain)
    }

    /// Starts a SHAKE256 sponge with `domain`.
    pub(crate) fn new_256(domain: u8) -> Self {
        Self::new(ShakeKind::Shake256, domain)
    }

    /// Starts a TurboSHAKE128 sponge with `domain`.
    pub(crate) fn new_turboshake_128(domain: u8) -> Self {
        Self::new(ShakeKind::TurboShake128, domain)
    }

    /// Starts a TurboSHAKE256 sponge with `domain`.
    pub(crate) fn new_turboshake_256(domain: u8) -> Self {
        Self::new(ShakeKind::TurboShake256, domain)
    }

    /// Starts a sponge for `kind`.
    fn new(kind: ShakeKind, domain: u8) -> Self {
        Self {
            state: [0u64; 25],
            offset: 0,
            phase: Phase::Absorbing,
            domain,
            rate: kind.rate(),
            rounds: kind.rounds(),
        }
    }

    /// Absorbs `input`; returns -1-equivalent when the state already squeezed.
    pub(crate) fn update(&mut self, input: &[u8]) -> Result<(), ()> {
        if self.phase == Phase::Squeezing {
            return Err(());
        }
        for &byte in input {
            if self.offset == self.rate {
                p1600(&mut self.state, self.rounds);
                self.offset = 0;
            }
            let lane = self.offset / 8;
            let shift = (self.offset % 8) * 8;
            self.state[lane] ^= u64::from(byte) << shift;
            self.offset += 1;
        }
        Ok(())
    }

    /// Squeezes `output.len()` bytes, finalizing the sponge on first use.
    pub(crate) fn squeeze(&mut self, output: &mut [u8]) {
        if self.phase == Phase::Absorbing {
            self.finalize();
        }
        for byte in output {
            if self.offset == self.rate {
                p1600(&mut self.state, self.rounds);
                self.offset = 0;
            }
            let lane = self.offset / 8;
            let shift = (self.offset % 8) * 8;
            *byte = (self.state[lane] >> shift) as u8;
            self.offset += 1;
        }
    }

    /// Applies libsodium's SHAKE padding and performs the final permutation.
    fn finalize(&mut self) {
        if self.offset == self.rate {
            p1600(&mut self.state, self.rounds);
            self.offset = 0;
        }
        if self.offset == self.rate - 1 {
            self.xor_byte(self.domain ^ 0x80);
        } else {
            self.xor_byte(self.domain);
            self.xor_byte_at(0x80, self.rate - 1);
        }
        p1600(&mut self.state, self.rounds);
        self.offset = 0;
        self.phase = Phase::Squeezing;
    }

    /// XORs one byte at the current offset.
    fn xor_byte(&mut self, byte: u8) {
        let offset = self.offset;
        self.xor_byte_at(byte, offset);
    }

    /// XORs one byte at an explicit state offset.
    fn xor_byte_at(&mut self, byte: u8, offset: usize) {
        let lane = offset / 8;
        let shift = (offset % 8) * 8;
        self.state[lane] ^= u64::from(byte) << shift;
    }
}

/// One-shot SHAKE128 over `message`.
pub(crate) fn shake128(length: usize, message: &[u8]) -> Option<Vec<u8>> {
    one_shot(Sponge::new_128(DOMAIN_STANDARD), length, message)
}

/// One-shot SHAKE256 over `message`.
pub(crate) fn shake256(length: usize, message: &[u8]) -> Option<Vec<u8>> {
    one_shot(Sponge::new_256(DOMAIN_STANDARD), length, message)
}

/// One-shot TurboSHAKE128 over `message`.
pub(crate) fn turboshake128(length: usize, message: &[u8]) -> Option<Vec<u8>> {
    one_shot(Sponge::new_turboshake_128(DOMAIN_STANDARD), length, message)
}

/// One-shot TurboSHAKE256 over `message`.
pub(crate) fn turboshake256(length: usize, message: &[u8]) -> Option<Vec<u8>> {
    one_shot(Sponge::new_turboshake_256(DOMAIN_STANDARD), length, message)
}

/// Runs one sponge over `message` and returns `length` output bytes.
fn one_shot(mut sponge: Sponge, length: usize, message: &[u8]) -> Option<Vec<u8>> {
    if length == 0 {
        return None;
    }
    sponge.update(message).ok()?;
    let mut output = vec![0u8; length];
    sponge.squeeze(&mut output);
    Some(output)
}

static STATES: OnceLock<Mutex<HashMap<i64, Sponge>>> = OnceLock::new();
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

/// Returns the process-wide XOF state table.
fn states() -> &'static Mutex<HashMap<i64, Sponge>> {
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Starts a state for `kind`; `domain = -1` selects libsodium's standard byte.
pub(crate) fn init(kind: ShakeKind, domain: i64) -> i64 {
    let domain = match domain {
        -1 => DOMAIN_STANDARD,
        1..=0x7f => domain as u8,
        _ => return -1,
    };
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    states()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(id, Sponge::new(kind, domain));
    id
}

/// Absorbs `message` into one state handle; returns 0 on success or -1.
pub(crate) fn update(handle: i64, message: &[u8]) -> i64 {
    let mut guard = states()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(state) = guard.get_mut(&handle) else {
        return -1;
    };
    match state.update(message) {
        Ok(()) => 0,
        Err(()) => -1,
    }
}

/// Squeezes `length` bytes from one state handle.
pub(crate) fn squeeze(handle: i64, length: usize) -> Option<Vec<u8>> {
    if length == 0 {
        return None;
    }
    let mut guard = states()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let state = guard.get_mut(&handle)?;
    let mut output = vec![0u8; length];
    state.squeeze(&mut output);
    Some(output)
}

/// Drops one state handle.
pub(crate) fn drop_state(handle: i64) {
    let _ = states()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&handle);
}
