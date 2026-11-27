//! Purpose:
//! C ABI for libsodium's Ed25519 `crypto_sign` family, including key
//! conversion to Curve25519.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - Secret keys use libsodium's 64-byte `seed || public_key` layout, and
//!   keypairs use `secret_key || public_key` (96 bytes).
//! - Open/verify failures return null or zero respectively instead of throwing;
//!   the PHP wrapper maps them to `false`.

use std::ptr;

use curve25519_dalek::edwards::CompressedEdwardsY;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha512};

use crate::{bytes, guarded, publish};

/// Builds a 96-byte keypair from a 32-byte seed.
fn keypair(seed: &[u8]) -> Option<Vec<u8>> {
    let seed: [u8; 32] = seed.try_into().ok()?;
    let signing = SigningKey::from_bytes(&seed);
    let public = signing.verifying_key().to_bytes();
    let mut out = Vec::with_capacity(96);
    out.extend_from_slice(&seed);
    out.extend_from_slice(&public);
    out.extend_from_slice(&public);
    Some(out)
}

/// C ABI: derives a `crypto_sign` keypair from a 32-byte seed.
///
/// # Safety
/// `seed` must reference `seed_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_sign_seed_keypair(
    seed: *const u8,
    seed_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(seed) = bytes(seed, seed_len) else {
            return ptr::null();
        };
        keypair(seed)
            .map(|pair| publish(&pair))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: returns the 32-byte public key from a 64-byte secret key.
///
/// # Safety
/// `secret_key` must reference `secret_key_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_sign_publickey_from_secretkey(
    secret_key: *const u8,
    secret_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(secret_key) = bytes(secret_key, secret_key_len) else {
            return ptr::null();
        };
        if secret_key.len() != 64 {
            return ptr::null();
        }
        publish(&secret_key[32..64])
    })
}

/// C ABI: Ed25519 detached signature.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_sign_detached(
    message: *const u8,
    message_len: usize,
    secret_key: *const u8,
    secret_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(message), Some(secret_key)) = (
            bytes(message, message_len),
            bytes(secret_key, secret_key_len),
        ) else {
            return ptr::null();
        };
        if secret_key.len() != 64 {
            return ptr::null();
        }
        let seed: [u8; 32] = secret_key[..32].try_into().expect("seed");
        let signing = SigningKey::from_bytes(&seed);
        let signature: Signature = signing.sign(message);
        publish(&signature.to_bytes())
    })
}

/// C ABI: Ed25519 combined signature (`signature || message`).
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_sign(
    message: *const u8,
    message_len: usize,
    secret_key: *const u8,
    secret_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(message), Some(secret_key)) = (
            bytes(message, message_len),
            bytes(secret_key, secret_key_len),
        ) else {
            return ptr::null();
        };
        if secret_key.len() != 64 {
            return ptr::null();
        }
        let seed: [u8; 32] = secret_key[..32].try_into().expect("seed");
        let signing = SigningKey::from_bytes(&seed);
        let signature: Signature = signing.sign(message);
        let mut out = Vec::with_capacity(64 + message.len());
        out.extend_from_slice(&signature.to_bytes());
        out.extend_from_slice(message);
        publish(&out)
    })
}

/// C ABI: opens a combined signature, returning the message or null.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_sign_open(
    signed_message: *const u8,
    signed_message_len: usize,
    public_key: *const u8,
    public_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(signed), Some(public_key)) = (
            bytes(signed_message, signed_message_len),
            bytes(public_key, public_key_len),
        ) else {
            return ptr::null();
        };
        if signed.len() < 64 || public_key.len() != 32 {
            return ptr::null();
        }
        let Ok(public_key) = VerifyingKey::from_bytes(
            &public_key.try_into().expect("public key length"),
        ) else {
            return ptr::null();
        };
        let signature = Signature::from_bytes(
            &signed[..64].try_into().expect("signature length"),
        );
        if public_key.verify(&signed[64..], &signature).is_err() {
            return ptr::null();
        }
        publish(&signed[64..])
    })
}

/// C ABI: verifies a detached Ed25519 signature (1 on success, 0 on failure).
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_sign_verify_detached(
    signature: *const u8,
    signature_len: usize,
    message: *const u8,
    message_len: usize,
    public_key: *const u8,
    public_key_len: usize,
) -> i32 {
    guarded(0, || {
        let (Some(signature), Some(message), Some(public_key)) = (
            bytes(signature, signature_len),
            bytes(message, message_len),
            bytes(public_key, public_key_len),
        ) else {
            return 0;
        };
        if signature.len() != 64 || public_key.len() != 32 {
            return 0;
        }
        let Ok(public_key) = VerifyingKey::from_bytes(
            &public_key.try_into().expect("public key length"),
        ) else {
            return 0;
        };
        let signature = Signature::from_bytes(
            &signature.try_into().expect("signature length"),
        );
        i32::from(public_key.verify_strict(message, &signature).is_ok())
    })
}

/// C ABI: converts an Ed25519 public key to a Curve25519 public key.
///
/// # Safety
/// `public_key` must reference `public_key_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_sign_pk_to_curve25519(
    public_key: *const u8,
    public_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(public_key) = bytes(public_key, public_key_len) else {
            return ptr::null();
        };
        if public_key.len() != 32 {
            return ptr::null();
        }
        let compressed = CompressedEdwardsY(public_key.try_into().expect("public key length"));
        let Some(point) = compressed.decompress() else {
            return ptr::null();
        };
        if point.is_small_order() || !point.is_torsion_free() {
            return ptr::null();
        }
        publish(&point.to_montgomery().to_bytes())
    })
}

/// C ABI: converts an Ed25519 secret key to a Curve25519 secret key.
///
/// # Safety
/// `secret_key` must reference `secret_key_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_sign_sk_to_curve25519(
    secret_key: *const u8,
    secret_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(secret_key) = bytes(secret_key, secret_key_len) else {
            return ptr::null();
        };
        if secret_key.len() != 64 {
            return ptr::null();
        }
        let mut hashed = Sha512::digest(&secret_key[..32]);
        let mut scalar = [0u8; 32];
        scalar.copy_from_slice(&hashed[..32]);
        scalar[0] &= 248;
        scalar[31] &= 127;
        scalar[31] |= 64;
        hashed.fill(0);
        publish(&scalar)
    })
}
