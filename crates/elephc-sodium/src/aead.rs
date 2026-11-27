//! Purpose:
//! C ABI for libsodium's AEAD families: ChaCha20-Poly1305 (legacy and IETF),
//! XChaCha20-Poly1305-IETF and AES-256-GCM.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - Ciphertexts are `ciphertext || 16-byte tag`, matching libsodium's
//!   `crypto_aead_*_encrypt` wire format.
//! - The Poly1305 input uses libsodium's padded AAD/ciphertext and little-endian
//!   length words; AES-GCM uses GHASH with big-endian bit lengths.

use std::ptr;

use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes256;
use ghash::universal_hash::UniversalHash;
use ghash::GHash;
use poly1305::Poly1305;

use crate::chacha;
use crate::{bytes, guarded, publish};

/// Appends zero padding until `data` is a multiple of 16 bytes.
fn pad16(data: &mut Vec<u8>) {
    data.resize((data.len() + 15) & !15, 0);
}

/// Builds the Poly1305 MAC input for an AEAD message.
///
/// libsodium's legacy ChaCha20-Poly1305 layout is `ad || le64(adlen) || ct ||
/// le64(ctlen)` without padding; the IETF layout pads each section to 16 bytes
/// and appends both lengths at the end.
fn poly_input(aad: &[u8], ciphertext: &[u8], ietf: bool) -> Vec<u8> {
    let mut data = Vec::with_capacity(aad.len() + ciphertext.len() + 40);
    data.extend_from_slice(aad);
    if ietf {
        pad16(&mut data);
        data.extend_from_slice(ciphertext);
        pad16(&mut data);
        data.extend_from_slice(&(aad.len() as u64).to_le_bytes());
        data.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());
    } else {
        data.extend_from_slice(&(aad.len() as u64).to_le_bytes());
        data.extend_from_slice(ciphertext);
        data.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());
    }
    data
}

/// Computes a Poly1305 tag over the AEAD input.
fn poly1305_tag(poly_key: &[u8], aad: &[u8], ciphertext: &[u8], ietf: bool) -> [u8; 16] {
    let mut key = [0u8; 32];
    key.copy_from_slice(&poly_key[..32]);
    let mac = Poly1305::new((&key).into()).compute_unpadded(&poly_input(aad, ciphertext, ietf));
    mac.into()
}

/// Constant-time byte comparison.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= a ^ b;
    }
    difference == 0
}

/// ChaCha20-Poly1305 seal, returning `ciphertext || tag`.
fn chacha20poly1305_seal(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    message: &[u8],
    ietf: bool,
) -> Option<Vec<u8>> {
    let key: [u8; 32] = key.try_into().ok()?;
    let mut poly_block = [0u8; 64];
    let mut ciphertext = message.to_vec();
    if ietf {
        let nonce: [u8; 12] = nonce.try_into().ok()?;
        chacha::apply_ietf(&key, &nonce, 0, &mut poly_block);
        chacha::apply_ietf(&key, &nonce, 1, &mut ciphertext);
    } else {
        let nonce: [u8; 8] = nonce.try_into().ok()?;
        chacha::apply_legacy(&key, &nonce, 0, &mut poly_block);
        chacha::apply_legacy(&key, &nonce, 1, &mut ciphertext);
    }
    let tag = poly1305_tag(&poly_block, aad, &ciphertext, ietf);
    ciphertext.extend_from_slice(&tag);
    Some(ciphertext)
}

/// ChaCha20-Poly1305 open.
fn chacha20poly1305_open(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
    ietf: bool,
) -> Option<Vec<u8>> {
    if ciphertext.len() < 16 {
        return None;
    }
    let (body, tag) = ciphertext.split_at(ciphertext.len() - 16);
    let key: [u8; 32] = key.try_into().ok()?;
    let mut poly_block = [0u8; 64];
    if ietf {
        let nonce: [u8; 12] = nonce.try_into().ok()?;
        chacha::apply_ietf(&key, &nonce, 0, &mut poly_block);
        if !constant_time_eq(&poly1305_tag(&poly_block, aad, body, ietf), tag) {
            return None;
        }
        let mut plaintext = body.to_vec();
        chacha::apply_ietf(&key, &nonce, 1, &mut plaintext);
        Some(plaintext)
    } else {
        let nonce: [u8; 8] = nonce.try_into().ok()?;
        chacha::apply_legacy(&key, &nonce, 0, &mut poly_block);
        if !constant_time_eq(&poly1305_tag(&poly_block, aad, body, false), tag) {
            return None;
        }
        let mut plaintext = body.to_vec();
        chacha::apply_legacy(&key, &nonce, 1, &mut plaintext);
        Some(plaintext)
    }
}

/// XChaCha20-Poly1305-IETF seal.
fn xchacha20poly1305_seal(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    message: &[u8],
) -> Option<Vec<u8>> {
    let key: [u8; 32] = key.try_into().ok()?;
    let nonce: [u8; 24] = nonce.try_into().ok()?;
    let mut poly_block = [0u8; 64];
    let mut ciphertext = message.to_vec();
    chacha::apply_xchacha20(&key, &nonce, 0, &mut poly_block);
    chacha::apply_xchacha20(&key, &nonce, 1, &mut ciphertext);
    let tag = poly1305_tag(&poly_block, aad, &ciphertext, true);
    ciphertext.extend_from_slice(&tag);
    Some(ciphertext)
}

/// XChaCha20-Poly1305-IETF open.
fn xchacha20poly1305_open(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Option<Vec<u8>> {
    if ciphertext.len() < 16 {
        return None;
    }
    let (body, tag) = ciphertext.split_at(ciphertext.len() - 16);
    let key: [u8; 32] = key.try_into().ok()?;
    let nonce: [u8; 24] = nonce.try_into().ok()?;
    let mut poly_block = [0u8; 64];
    chacha::apply_xchacha20(&key, &nonce, 0, &mut poly_block);
    if !constant_time_eq(&poly1305_tag(&poly_block, aad, body, true), tag) {
        return None;
    }
    let mut plaintext = body.to_vec();
    chacha::apply_xchacha20(&key, &nonce, 1, &mut plaintext);
    Some(plaintext)
}

/// AES-256-GCM seal.
fn aes256gcm_seal(key: &[u8], nonce: &[u8], aad: &[u8], message: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 32 || nonce.len() != 12 {
        return None;
    }
    let cipher = Aes256::new_from_slice(key).ok()?;
    let mut hash_key = [0u8; 16];
    cipher.encrypt_block((&mut hash_key).into());
    let mut j0 = [0u8; 16];
    j0[..12].copy_from_slice(nonce);
    j0[15] = 1;
    let mut ciphertext = message.to_vec();
    let mut counter = j0;
    for chunk in ciphertext.chunks_mut(16) {
        counter[15] = counter[15].wrapping_add(1);
        if counter[15] == 0 {
            for index in (12..15).rev() {
                counter[index] = counter[index].wrapping_add(1);
                if counter[index] != 0 {
                    break;
                }
            }
        }
        let mut block = counter;
        cipher.encrypt_block((&mut block).into());
        for (byte, mask) in chunk.iter_mut().zip(block.iter()) {
            *byte ^= *mask;
        }
    }
    let mut ghash = GHash::new_from_slice(&hash_key).ok()?;
    ghash.update_padded(aad);
    ghash.update_padded(&ciphertext);
    let mut lengths = [0u8; 16];
    lengths[..8].copy_from_slice(&((aad.len() as u64) * 8).to_be_bytes());
    lengths[8..].copy_from_slice(&((ciphertext.len() as u64) * 8).to_be_bytes());
    ghash.update_padded(&lengths);
    let mut mask = j0;
    cipher.encrypt_block((&mut mask).into());
    let tag = ghash.finalize();
    let mut output = ciphertext;
    for index in 0..16 {
        output.push(tag[index] ^ mask[index]);
    }
    Some(output)
}

/// AES-256-GCM open.
fn aes256gcm_open(key: &[u8], nonce: &[u8], aad: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 32 || nonce.len() != 12 || ciphertext.len() < 16 {
        return None;
    }
    let (body, tag) = ciphertext.split_at(ciphertext.len() - 16);
    let cipher = Aes256::new_from_slice(key).ok()?;
    let mut hash_key = [0u8; 16];
    cipher.encrypt_block((&mut hash_key).into());
    let mut j0 = [0u8; 16];
    j0[..12].copy_from_slice(nonce);
    j0[15] = 1;
    let mut ghash = GHash::new_from_slice(&hash_key).ok()?;
    ghash.update_padded(aad);
    ghash.update_padded(body);
    let mut lengths = [0u8; 16];
    lengths[..8].copy_from_slice(&((aad.len() as u64) * 8).to_be_bytes());
    lengths[8..].copy_from_slice(&((body.len() as u64) * 8).to_be_bytes());
    ghash.update_padded(&lengths);
    let mut mask = j0;
    cipher.encrypt_block((&mut mask).into());
    let computed = ghash.finalize();
    let mut expected = [0u8; 16];
    for index in 0..16 {
        expected[index] = computed[index] ^ mask[index];
    }
    if !constant_time_eq(&expected, tag) {
        return None;
    }
    let mut plaintext = body.to_vec();
    let mut counter = j0;
    for chunk in plaintext.chunks_mut(16) {
        counter[15] = counter[15].wrapping_add(1);
        if counter[15] == 0 {
            for index in (12..15).rev() {
                counter[index] = counter[index].wrapping_add(1);
                if counter[index] != 0 {
                    break;
                }
            }
        }
        let mut block = counter;
        cipher.encrypt_block((&mut block).into());
        for (byte, mask) in chunk.iter_mut().zip(block.iter()) {
            *byte ^= *mask;
        }
    }
    Some(plaintext)
}

/// C ABI: ChaCha20-Poly1305 (legacy, 8-byte nonce) encryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_chacha20poly1305_encrypt(
    message: *const u8,
    message_len: usize,
    aad: *const u8,
    aad_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(message), Some(aad), Some(nonce), Some(key)) = (
            bytes(message, message_len),
            bytes(aad, aad_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        chacha20poly1305_seal(key, nonce, aad, message, false)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: ChaCha20-Poly1305 (legacy) decryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_chacha20poly1305_decrypt(
    ciphertext: *const u8,
    ciphertext_len: usize,
    aad: *const u8,
    aad_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(ciphertext), Some(aad), Some(nonce), Some(key)) = (
            bytes(ciphertext, ciphertext_len),
            bytes(aad, aad_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        chacha20poly1305_open(key, nonce, aad, ciphertext, false)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: ChaCha20-Poly1305-IETF encryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_chacha20poly1305_ietf_encrypt(
    message: *const u8,
    message_len: usize,
    aad: *const u8,
    aad_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(message), Some(aad), Some(nonce), Some(key)) = (
            bytes(message, message_len),
            bytes(aad, aad_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        chacha20poly1305_seal(key, nonce, aad, message, true)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: ChaCha20-Poly1305-IETF decryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_chacha20poly1305_ietf_decrypt(
    ciphertext: *const u8,
    ciphertext_len: usize,
    aad: *const u8,
    aad_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(ciphertext), Some(aad), Some(nonce), Some(key)) = (
            bytes(ciphertext, ciphertext_len),
            bytes(aad, aad_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        chacha20poly1305_open(key, nonce, aad, ciphertext, true)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: XChaCha20-Poly1305-IETF encryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_xchacha20poly1305_ietf_encrypt(
    message: *const u8,
    message_len: usize,
    aad: *const u8,
    aad_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(message), Some(aad), Some(nonce), Some(key)) = (
            bytes(message, message_len),
            bytes(aad, aad_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        xchacha20poly1305_seal(key, nonce, aad, message)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: XChaCha20-Poly1305-IETF decryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_xchacha20poly1305_ietf_decrypt(
    ciphertext: *const u8,
    ciphertext_len: usize,
    aad: *const u8,
    aad_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(ciphertext), Some(aad), Some(nonce), Some(key)) = (
            bytes(ciphertext, ciphertext_len),
            bytes(aad, aad_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        xchacha20poly1305_open(key, nonce, aad, ciphertext)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: AES-256-GCM encryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_aes256gcm_encrypt(
    message: *const u8,
    message_len: usize,
    aad: *const u8,
    aad_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(message), Some(aad), Some(nonce), Some(key)) = (
            bytes(message, message_len),
            bytes(aad, aad_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        aes256gcm_seal(key, nonce, aad, message)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: AES-256-GCM decryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_aes256gcm_decrypt(
    ciphertext: *const u8,
    ciphertext_len: usize,
    aad: *const u8,
    aad_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(ciphertext), Some(aad), Some(nonce), Some(key)) = (
            bytes(ciphertext, ciphertext_len),
            bytes(aad, aad_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        aes256gcm_open(key, nonce, aad, ciphertext)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}
