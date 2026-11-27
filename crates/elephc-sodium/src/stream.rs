//! Purpose:
//! C ABI for libsodium's `crypto_stream` (XSalsa20) and
//! `crypto_stream_xchacha20` keystream helpers.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - XSalsa20 delegates to the `salsa20` crate; XChaCha20 uses the shared
//!   counter-aware keystream implementation so `_xor_ic` can start at an
//!   arbitrary block counter.

use std::ptr;

use salsa20::cipher::{KeyIvInit, StreamCipher};
use salsa20::XSalsa20;

use crate::chacha;
use crate::{bytes, guarded, publish};

/// C ABI: XSalsa20 keystream of `length` bytes.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_stream(
    length: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(nonce), Some(key)) = (bytes(nonce, nonce_len), bytes(key, key_len)) else {
            return ptr::null();
        };
        if nonce.len() != 24 || key.len() != 32 {
            return ptr::null();
        }
        let mut output = vec![0u8; length];
        let mut cipher = XSalsa20::new(key.into(), nonce.into());
        cipher.apply_keystream(&mut output);
        publish(&output)
    })
}

/// C ABI: XSalsa20 XOR of a message.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_stream_xor(
    message: *const u8,
    message_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(message), Some(nonce), Some(key)) = (
            bytes(message, message_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        if nonce.len() != 24 || key.len() != 32 {
            return ptr::null();
        }
        let mut output = message.to_vec();
        let mut cipher = XSalsa20::new(key.into(), nonce.into());
        cipher.apply_keystream(&mut output);
        publish(&output)
    })
}

/// C ABI: XChaCha20 keystream of `length` bytes.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_stream_xchacha20(
    length: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(nonce), Some(key)) = (bytes(nonce, nonce_len), bytes(key, key_len)) else {
            return ptr::null();
        };
        if nonce.len() != 24 || key.len() != 32 {
            return ptr::null();
        }
        let nonce: [u8; 24] = nonce.try_into().expect("nonce length");
        let key: [u8; 32] = key.try_into().expect("key length");
        let mut output = vec![0u8; length];
        chacha::apply_xchacha20(&key, &nonce, 0, &mut output);
        publish(&output)
    })
}

/// C ABI: XChaCha20 XOR of a message.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_stream_xchacha20_xor(
    message: *const u8,
    message_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    elephc_sodium_stream_xchacha20_xor_ic(
        message,
        message_len,
        nonce,
        nonce_len,
        0,
        key,
        key_len,
    )
}

/// C ABI: XChaCha20 XOR of a message starting at a block counter.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_stream_xchacha20_xor_ic(
    message: *const u8,
    message_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    counter: u64,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(message), Some(nonce), Some(key)) = (
            bytes(message, message_len),
            bytes(nonce, nonce_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        if nonce.len() != 24 || key.len() != 32 || counter > u32::MAX as u64 {
            return ptr::null();
        }
        let nonce: [u8; 24] = nonce.try_into().expect("nonce length");
        let key: [u8; 32] = key.try_into().expect("key length");
        let mut output = message.to_vec();
        chacha::apply_xchacha20(&key, &nonce, counter as u32, &mut output);
        publish(&output)
    })
}
