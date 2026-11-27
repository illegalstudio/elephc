//! Purpose:
//! C ABI for BLAKE2b `crypto_generichash`, HMAC-SHA512/256 `crypto_auth`,
//! BLAKE2b-derived `crypto_kdf` and SipHash-2-4 `crypto_shorthash`.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - Streaming generichash state strings are 384 bytes, matching PHP's opaque
//!   state size, and carry the serialised BLAKE2b state for value semantics.
//! - All output goes through the thread-local publish buffer read by
//!   `elephc_sodium_last_len`.

use std::ptr;

use hmac::{Hmac, Mac};
use sha2::Sha512;
use siphasher::sip::SipHasher24;
use std::hash::Hasher;

use crate::blake2b::Blake2b;
use crate::{bytes, guarded, publish};

/// Length of a serialised generichash state, matching PHP's opaque state size.
pub const GH_STATE_BYTES: usize = 384;

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

/// C ABI: one-shot BLAKE2b generichash with an optional key.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_generichash(
    message: *const u8,
    message_len: usize,
    key: *const u8,
    key_len: usize,
    output_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(message) = bytes(message, message_len) else {
            return ptr::null();
        };
        let Some(key) = bytes(key, key_len) else {
            return ptr::null();
        };
        crate::blake2b::hash(output_len, message, key, &[], &[])
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: creates a serialised BLAKE2b streaming state.
///
/// # Safety
/// `key` must reference `key_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_generichash_init(
    key: *const u8,
    key_len: usize,
    output_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(key) = bytes(key, key_len) else {
            return ptr::null();
        };
        Blake2b::new(output_len, key, &[], &[])
            .map(|state| publish(&state.serialize()))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: absorbs `message` into a serialised generichash state and returns the
/// updated state string.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_generichash_update(
    state: *const u8,
    state_len: usize,
    message: *const u8,
    message_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(state) = bytes(state, state_len) else {
            return ptr::null();
        };
        let Some(message) = bytes(message, message_len) else {
            return ptr::null();
        };
        let Some(mut state) = Blake2b::deserialize(state) else {
            return ptr::null();
        };
        state.update(message);
        publish(&state.serialize())
    })
}

/// C ABI: finalises a serialised generichash state.
///
/// # Safety
/// `state` must reference `state_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_generichash_final(
    state: *const u8,
    state_len: usize,
    output_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(state) = bytes(state, state_len) else {
            return ptr::null();
        };
        let Some(state) = Blake2b::deserialize(state) else {
            return ptr::null();
        };
        if output_len == 0 || output_len > 64 {
            return ptr::null();
        }
        let mut output = state.finalize();
        output.truncate(output_len);
        publish(&output)
    })
}

/// C ABI: one-shot HMAC-SHA512/256 authentication.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_auth(
    message: *const u8,
    message_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(message) = bytes(message, message_len) else {
            return ptr::null();
        };
        let Some(key) = bytes(key, key_len) else {
            return ptr::null();
        };
        if key.len() != 32 {
            return ptr::null();
        }
        let Ok(mut mac) = <Hmac<Sha512> as Mac>::new_from_slice(key) else {
            return ptr::null();
        };
        mac.update(message);
        let tag = mac.finalize().into_bytes();
        publish(&tag[..32])
    })
}

/// C ABI: constant-time HMAC-SHA512/256 verification (1 on success, 0 on failure).
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_auth_verify(
    mac: *const u8,
    mac_len: usize,
    message: *const u8,
    message_len: usize,
    key: *const u8,
    key_len: usize,
) -> i32 {
    guarded(0, || {
        let (Some(mac), Some(message), Some(key)) = (
            bytes(mac, mac_len),
            bytes(message, message_len),
            bytes(key, key_len),
        ) else {
            return 0;
        };
        if mac.len() != 32 || key.len() != 32 {
            return 0;
        }
        let Ok(mut expected) = <Hmac<Sha512> as Mac>::new_from_slice(key) else {
            return 0;
        };
        expected.update(message);
        let tag = expected.finalize().into_bytes();
        i32::from(constant_time_eq(&tag[..32], mac))
    })
}

/// C ABI: derives a BLAKE2b subkey with libsodium's salt/personal layout.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kdf(
    subkey_len: usize,
    subkey_id: u64,
    context: *const u8,
    context_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(context) = bytes(context, context_len) else {
            return ptr::null();
        };
        let Some(key) = bytes(key, key_len) else {
            return ptr::null();
        };
        if context.len() != 8 || key.len() != 32 || !(16..=64).contains(&subkey_len) {
            return ptr::null();
        }
        let mut salt = [0u8; 16];
        salt[..8].copy_from_slice(&subkey_id.to_le_bytes());
        crate::blake2b::hash(subkey_len, &[], key, &salt, context)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: SipHash-2-4 shorthash with a 16-byte key.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_shorthash(
    message: *const u8,
    message_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(message) = bytes(message, message_len) else {
            return ptr::null();
        };
        let Some(key) = bytes(key, key_len) else {
            return ptr::null();
        };
        if key.len() != 16 {
            return ptr::null();
        }
        let k0 = u64::from_le_bytes(key[..8].try_into().expect("key half"));
        let k1 = u64::from_le_bytes(key[8..].try_into().expect("key half"));
        let mut hasher = SipHasher24::new_with_keys(k0, k1);
        hasher.write(message);
        publish(&hasher.finish().to_le_bytes())
    })
}
