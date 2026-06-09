//! Purpose:
//! Generic HMAC over any `HashState`, avoiding a per-digest-type match.
//!
//! Called from:
//! - `crate::elephc_crypto_hmac` and the HMAC-mode incremental context (Task 5).
//!
//! Key details:
//! - Implements RFC 2104 using the algorithm's block size from `HashState`.
//!   A key longer than the block size is hashed down first, then zero-padded.
//! - Returns None for unknown algorithms AND for non-crypto checksums
//!   (block_size == 0), matching PHP's ValueError for hash_hmac over checksums.

use crate::algos::make;

/// Derives the block-sized HMAC key K': hash-down if longer than the block,
/// then zero-pad to the block size. Returns None for an unknown algorithm or a
/// non-crypto checksum (block_size == 0, which PHP rejects for HMAC).
pub(crate) fn block_key(algo: &str, key: &[u8]) -> Option<Vec<u8>> {
    let mut probe = make(algo)?;
    let block = probe.block_size();
    if block == 0 {
        return None;
    }
    let mut k = if key.len() > block {
        probe.update(key);
        probe.finalize_box()
    } else {
        key.to_vec()
    };
    k.resize(block, 0);
    Some(k)
}

/// Computes HMAC(key, data) under `algo`, returning the raw digest, or None for
/// an unknown algorithm or a non-crypto checksum.
pub(crate) fn hmac(algo: &str, key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let k = block_key(algo, key)?;
    let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();

    let mut inner = make(algo)?;
    inner.update(&ipad);
    inner.update(data);
    let inner_digest = inner.finalize_box();

    let mut outer = make(algo)?;
    outer.update(&opad);
    outer.update(&inner_digest);
    Some(outer.finalize_box())
}
