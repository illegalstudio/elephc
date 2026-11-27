//! Purpose:
//! C ABI for libsodium's `crypto_kx` key exchange helpers.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - Keypairs are `secret_key || public_key`; seeds are hashed with BLAKE2b-256
//!   exactly like libsodium's `crypto_kx_seed_keypair`.
//! - Session keys are BLAKE2b-512 over `shared || client_pk || server_pk`, then
//!   split into rx/tx in libsodium's client/server orientation.

use std::ptr;

use crate::blake2b;
use crate::boxes::{elephc_sodium_scalarmult, elephc_sodium_scalarmult_base};
use crate::{bytes, guarded, publish};

/// Derives a `crypto_kx` keypair from a 32-byte seed.
fn seed_keypair(seed: &[u8]) -> Option<Vec<u8>> {
    if seed.len() != 32 {
        return None;
    }
    let secret = blake2b::hash(32, seed, &[], &[], &[])?;
    let public = unsafe {
        let pointer = elephc_sodium_scalarmult_base(secret.as_ptr(), secret.len());
        if pointer.is_null() {
            return None;
        }
        let length = crate::elephc_sodium_last_len();
        std::slice::from_raw_parts(pointer, length).to_vec()
    };
    let mut pair = Vec::with_capacity(64);
    pair.extend_from_slice(&secret);
    pair.extend_from_slice(&public);
    Some(pair)
}

/// Builds the 64-byte `rx || tx` session-key blob.
fn session_keys(secret: &[u8], public: &[u8], peer: &[u8], client: bool) -> Option<Vec<u8>> {
    if secret.len() != 32 || public.len() != 32 || peer.len() != 32 {
        return None;
    }
    let shared = unsafe {
        let pointer = elephc_sodium_scalarmult(
            secret.as_ptr(),
            secret.len(),
            peer.as_ptr(),
            peer.len(),
        );
        if pointer.is_null() {
            return None;
        }
        let length = crate::elephc_sodium_last_len();
        std::slice::from_raw_parts(pointer, length).to_vec()
    };
    let mut input = Vec::with_capacity(96);
    input.extend_from_slice(&shared);
    if client {
        input.extend_from_slice(public);
        input.extend_from_slice(peer);
    } else {
        input.extend_from_slice(peer);
        input.extend_from_slice(public);
    }
    let keys = blake2b::hash(64, &input, &[], &[], &[])?;
    let (rx, tx) = if client {
        (&keys[..32], &keys[32..])
    } else {
        (&keys[32..], &keys[..32])
    };
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(rx);
    out.extend_from_slice(tx);
    Some(out)
}

/// C ABI: derives a `crypto_kx` keypair from a 32-byte seed.
///
/// # Safety
/// `seed` must reference `seed_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kx_seed_keypair(
    seed: *const u8,
    seed_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(seed) = bytes(seed, seed_len) else {
            return ptr::null();
        };
        seed_keypair(seed)
            .map(|pair| publish(&pair))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: client-side session keys (`rx || tx`).
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kx_client_session_keys(
    client_key_pair: *const u8,
    client_key_pair_len: usize,
    server_key: *const u8,
    server_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(pair), Some(server_key)) = (
            bytes(client_key_pair, client_key_pair_len),
            bytes(server_key, server_key_len),
        ) else {
            return ptr::null();
        };
        if pair.len() != 64 {
            return ptr::null();
        }
        session_keys(&pair[..32], &pair[32..], server_key, true)
            .map(|keys| publish(&keys))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: server-side session keys (`rx || tx`).
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kx_server_session_keys(
    server_key_pair: *const u8,
    server_key_pair_len: usize,
    client_key: *const u8,
    client_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(pair), Some(client_key)) = (
            bytes(server_key_pair, server_key_pair_len),
            bytes(client_key, client_key_len),
        ) else {
            return ptr::null();
        };
        if pair.len() != 64 {
            return ptr::null();
        }
        session_keys(&pair[..32], &pair[32..], client_key, false)
            .map(|keys| publish(&keys))
            .unwrap_or(ptr::null())
    })
}
