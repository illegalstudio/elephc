//! Purpose:
//! C ABI for libsodium's Curve25519-XSalsa20-Poly1305 `crypto_box` family and
//! the raw Curve25519 `crypto_scalarmult` helpers.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - Keypairs use libsodium's `secret_key || public_key` layout.
//! - `crypto_box_seed_keypair` derives the secret key as `SHA-512(seed)[..32]`,
//!   matching libsodium rather than clamping the seed directly.
//! - Sealed boxes are composed in the PHP prelude from these primitives and
//!   `sodium_crypto_generichash`, keeping the ephemeral nonce derivation shared.

use std::ptr;

use crypto_box::{aead::Aead, Nonce, PublicKey, SalsaBox, SecretKey};
use curve25519_dalek::MontgomeryPoint;
use sha2::{Digest, Sha512};

use crate::{bytes, guarded, publish};

/// Derives the Curve25519 public key for a secret key.
fn public_key(secret_key: &[u8]) -> Option<[u8; 32]> {
    let secret_key: [u8; 32] = secret_key.try_into().ok()?;
    Some(MontgomeryPoint::mul_base_clamped(secret_key).to_bytes())
}

/// C ABI: derives a `crypto_box` keypair from a 32-byte seed.
///
/// # Safety
/// `seed` must reference `seed_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_box_seed_keypair(
    seed: *const u8,
    seed_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(seed) = bytes(seed, seed_len) else {
            return ptr::null();
        };
        if seed.len() != 32 {
            return ptr::null();
        }
        let hashed = Sha512::digest(seed);
        let secret = &hashed[..32];
        let Some(public) = public_key(secret) else {
            return ptr::null();
        };
        let mut pair = Vec::with_capacity(64);
        pair.extend_from_slice(secret);
        pair.extend_from_slice(&public);
        publish(&pair)
    })
}

/// C ABI: derives the public key for a `crypto_box` secret key.
///
/// # Safety
/// `secret_key` must reference `secret_key_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_box_publickey_from_secretkey(
    secret_key: *const u8,
    secret_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(secret_key) = bytes(secret_key, secret_key_len) else {
            return ptr::null();
        };
        public_key(secret_key)
            .map(|key| publish(&key))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: `crypto_box` encryption with a `secret_key || public_key` keypair.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_box_encrypt(
    message: *const u8,
    message_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key_pair: *const u8,
    key_pair_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(message), Some(nonce), Some(key_pair)) = (
            bytes(message, message_len),
            bytes(nonce, nonce_len),
            bytes(key_pair, key_pair_len),
        ) else {
            return ptr::null();
        };
        if nonce.len() != 24 || key_pair.len() != 64 {
            return ptr::null();
        }
        let secret_key = SecretKey::from_bytes(
            key_pair[..32].try_into().expect("secret key length"),
        );
        let public_key = PublicKey::from_bytes(
            key_pair[32..].try_into().expect("public key length"),
        );
        let cipher = SalsaBox::new(&public_key, &secret_key);
        match cipher.encrypt(Nonce::from_slice(nonce), message) {
            Ok(ciphertext) => publish(&ciphertext),
            Err(_) => ptr::null(),
        }
    })
}

/// C ABI: `crypto_box` decryption with a `secret_key || public_key` keypair.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_box_decrypt(
    ciphertext: *const u8,
    ciphertext_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key_pair: *const u8,
    key_pair_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(ciphertext), Some(nonce), Some(key_pair)) = (
            bytes(ciphertext, ciphertext_len),
            bytes(nonce, nonce_len),
            bytes(key_pair, key_pair_len),
        ) else {
            return ptr::null();
        };
        if nonce.len() != 24 || key_pair.len() != 64 {
            return ptr::null();
        }
        let secret_key = SecretKey::from_bytes(
            key_pair[..32].try_into().expect("secret key length"),
        );
        let public_key = PublicKey::from_bytes(
            key_pair[32..].try_into().expect("public key length"),
        );
        let cipher = SalsaBox::new(&public_key, &secret_key);
        match cipher.decrypt(Nonce::from_slice(nonce), ciphertext) {
            Ok(plaintext) => publish(&plaintext),
            Err(_) => ptr::null(),
        }
    })
}

/// C ABI: X25519 scalar multiplication of a secret key and a public key.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_scalarmult(
    secret_key: *const u8,
    secret_key_len: usize,
    public_key: *const u8,
    public_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(secret_key), Some(public_key)) = (
            bytes(secret_key, secret_key_len),
            bytes(public_key, public_key_len),
        ) else {
            return ptr::null();
        };
        if secret_key.len() != 32 || public_key.len() != 32 {
            return ptr::null();
        }
        let scalar: [u8; 32] = secret_key.try_into().expect("scalar length");
        let point: [u8; 32] = public_key.try_into().expect("point length");
        let shared = MontgomeryPoint(point).mul_clamped(scalar).to_bytes();
        if shared == [0u8; 32] {
            return ptr::null();
        }
        publish(&shared)
    })
}

/// C ABI: X25519 base-point multiplication.
///
/// # Safety
/// `secret_key` must reference `secret_key_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_scalarmult_base(
    secret_key: *const u8,
    secret_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(secret_key) = bytes(secret_key, secret_key_len) else {
            return ptr::null();
        };
        public_key(secret_key)
            .map(|key| publish(&key))
            .unwrap_or(ptr::null())
    })
}
