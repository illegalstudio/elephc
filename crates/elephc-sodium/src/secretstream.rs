//! Purpose:
//! C ABI for libsodium's `crypto_secretstream_xchacha20poly1305` streaming
//! construction, including rekeying.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - State strings are 52 bytes (`key || nonce counter || inonce || padding`) so
//!   PHP's copy-by-assignment preserves independent stream positions.
//! - Push/pull return `state || payload` because PHP passes the opaque state by
//!   reference; the wrapper writes the state back and returns the payload.

use std::ptr;

use poly1305::universal_hash::KeyInit;
use poly1305::Key;
use poly1305::Poly1305;

use crate::chacha;
use crate::{bytes, guarded, publish};

/// Serialised state length, matching PHP's opaque state size.
pub const STATE_BYTES: usize = 52;

/// Secretstream state.
#[derive(Clone, Copy)]
struct State {
    key: [u8; 32],
    /// Four-byte little-endian counter followed by the eight-byte inonce.
    nonce: [u8; 12],
}

impl State {
    /// Serialises the state into PHP's 52-byte opaque string.
    fn serialize(&self) -> Vec<u8> {
        let mut out = vec![0u8; STATE_BYTES];
        out[..32].copy_from_slice(&self.key);
        out[32..44].copy_from_slice(&self.nonce);
        out
    }

    /// Restores a state from PHP's opaque string.
    fn deserialize(data: &[u8]) -> Option<Self> {
        if data.len() != STATE_BYTES {
            return None;
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&data[..32]);
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&data[32..44]);
        Some(Self { key, nonce })
    }

    /// Initialises from a header and key.
    fn from_header(header: &[u8], key: &[u8]) -> Option<Self> {
        if header.len() != 24 || key.len() != 32 {
            return None;
        }
        let mut prefix = [0u8; 16];
        prefix.copy_from_slice(&header[..16]);
        let key: [u8; 32] = key.try_into().ok()?;
        let key = chacha::hchacha20(&key, &prefix);
        let mut nonce = [0u8; 12];
        nonce[0] = 1;
        nonce[4..].copy_from_slice(&header[16..24]);
        Some(Self { key, nonce })
    }

    /// Increments the four-byte little-endian counter.
    fn increment_counter(&mut self) {
        for byte in &mut self.nonce[..4] {
            *byte = byte.wrapping_add(1);
            if *byte != 0 {
                break;
            }
        }
    }

    /// Reports whether the counter wrapped to zero.
    fn counter_is_zero(&self) -> bool {
        self.nonce[..4].iter().all(|byte| *byte == 0)
    }

    /// Rekeys the stream, matching libsodium's ChaCha20-based derivation.
    fn rekey(&mut self) {
        let mut material = [0u8; 40];
        material[..32].copy_from_slice(&self.key);
        material[32..].copy_from_slice(&self.nonce[4..12]);
        chacha::apply_ietf(&self.key, &self.nonce, 0, &mut material);
        self.key.copy_from_slice(&material[..32]);
        self.nonce[4..12].copy_from_slice(&material[32..]);
        self.nonce[..4].fill(0);
        self.nonce[0] = 1;
    }
}

/// Appends zero padding until `data` is a multiple of 16 bytes.
fn pad16(data: &mut Vec<u8>) {
    data.resize((data.len() + 15) & !15, 0);
}

/// C ABI: initialises a push state from a caller-supplied header.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_secretstream_init_push(
    key: *const u8,
    key_len: usize,
    header: *const u8,
    header_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(key), Some(header)) = (bytes(key, key_len), bytes(header, header_len)) else {
            return ptr::null();
        };
        State::from_header(header, key)
            .map(|state| publish(&state.serialize()))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: initialises a pull state from a header.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_secretstream_init_pull(
    header: *const u8,
    header_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    elephc_sodium_secretstream_init_push(key, key_len, header, header_len)
}

/// C ABI: pushes one message; returns `state || ciphertext_with_tag`.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_secretstream_push(
    state: *const u8,
    state_len: usize,
    message: *const u8,
    message_len: usize,
    aad: *const u8,
    aad_len: usize,
    tag: u8,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(state), Some(message), Some(aad)) = (
            bytes(state, state_len),
            bytes(message, message_len),
            bytes(aad, aad_len),
        ) else {
            return ptr::null();
        };
        let Some(state) = State::deserialize(state) else {
            return ptr::null();
        };
        let mut block = [0u8; 64];
        chacha::apply_ietf(&state.key, &state.nonce, 0, &mut block);
        let poly_key = block;
        let mut tag_block = [0u8; 64];
        tag_block[0] = tag;
        chacha::apply_ietf(&state.key, &state.nonce, 1, &mut tag_block);
        let mut ciphertext = message.to_vec();
        chacha::apply_ietf(&state.key, &state.nonce, 2, &mut ciphertext);

        let mut mac_input = Vec::with_capacity(aad.len() + ciphertext.len() + 96);
        mac_input.extend_from_slice(aad);
        pad16(&mut mac_input);
        mac_input.extend_from_slice(&tag_block);
        mac_input.extend_from_slice(&ciphertext);
        mac_input.resize(mac_input.len() + ciphertext.len() % 16, 0);
        mac_input.extend_from_slice(&(aad.len() as u64).to_le_bytes());
        mac_input.extend_from_slice(&(64u64 + message.len() as u64).to_le_bytes());
        let mac = Poly1305::new(Key::from_slice(&poly_key[..32]))
            .compute_unpadded(&mac_input);

        let mut out = Vec::with_capacity(STATE_BYTES + 1 + ciphertext.len() + 16);
        let mut next = state;
        for (byte, mask) in next.nonce[4..12].iter_mut().zip(mac.iter()) {
            *byte ^= mask;
        }
        next.increment_counter();
        if tag & 0x02 != 0 || next.counter_is_zero() {
            next.rekey();
        }
        out.extend_from_slice(&next.serialize());
        out.push(tag_block[0]);
        out.extend_from_slice(&ciphertext);
        out.extend_from_slice(&mac);
        publish(&out)
    })
}

/// C ABI: pulls one message; returns `state || tag || plaintext`, or null.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_secretstream_pull(
    state: *const u8,
    state_len: usize,
    ciphertext: *const u8,
    ciphertext_len: usize,
    aad: *const u8,
    aad_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(state), Some(ciphertext), Some(aad)) = (
            bytes(state, state_len),
            bytes(ciphertext, ciphertext_len),
            bytes(aad, aad_len),
        ) else {
            return ptr::null();
        };
        if ciphertext.len() < 17 {
            return ptr::null();
        }
        let Some(mut state) = State::deserialize(state) else {
            return ptr::null();
        };
        let body_len = ciphertext.len() - 17;
        let tag_byte = ciphertext[0];
        let body = &ciphertext[1..1 + body_len];
        let stored_mac = &ciphertext[1 + body_len..];

        let mut block = [0u8; 64];
        chacha::apply_ietf(&state.key, &state.nonce, 0, &mut block);
        let poly_key = block;
        let mut tag_keystream = [0u8; 64];
        chacha::apply_ietf(&state.key, &state.nonce, 1, &mut tag_keystream);
        let tag = tag_byte ^ tag_keystream[0];
        let mut mac_tag_block = tag_keystream;
        mac_tag_block[0] = tag_byte;

        let mut mac_input = Vec::with_capacity(aad.len() + body_len + 96);
        mac_input.extend_from_slice(aad);
        pad16(&mut mac_input);
        mac_input.extend_from_slice(&mac_tag_block);
        mac_input.extend_from_slice(body);
        mac_input.resize(mac_input.len() + body_len % 16, 0);
        mac_input.extend_from_slice(&(aad.len() as u64).to_le_bytes());
        mac_input.extend_from_slice(&(64u64 + body_len as u64).to_le_bytes());
        let mac = Poly1305::new(Key::from_slice(&poly_key[..32]))
            .compute_unpadded(&mac_input);
        let mut difference = 0u8;
        for (a, b) in mac.iter().zip(stored_mac.iter()) {
            difference |= a ^ b;
        }
        if difference != 0 {
            return ptr::null();
        }
        let mut plaintext = body.to_vec();
        chacha::apply_ietf(&state.key, &state.nonce, 2, &mut plaintext);

        let mut out = Vec::with_capacity(STATE_BYTES + 1 + plaintext.len());
        for (byte, mask) in state.nonce[4..12].iter_mut().zip(mac.iter()) {
            *byte ^= mask;
        }
        state.increment_counter();
        if tag & 0x02 != 0 || state.counter_is_zero() {
            state.rekey();
        }
        out.extend_from_slice(&state.serialize());
        out.push(tag);
        out.extend_from_slice(&plaintext);
        publish(&out)
    })
}

/// C ABI: rekeys a serialised state.
///
/// # Safety
/// `state` must reference `state_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_secretstream_rekey(
    state: *const u8,
    state_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(state) = bytes(state, state_len) else {
            return ptr::null();
        };
        let Some(mut state) = State::deserialize(state) else {
            return ptr::null();
        };
        state.rekey();
        publish(&state.serialize())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes a lowercase hex fixture into bytes.
    fn hex(value: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(value.len() / 2);
        let digits = |byte: u8| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => panic!("invalid hex"),
        };
        for pair in value.as_bytes().chunks_exact(2) {
            out.push(digits(pair[0]) << 4 | digits(pair[1]));
        }
        out
    }

    /// Matches a PHP/libsodium secretstream push for a fixed header.
    #[test]
    fn php_push_vector() {
        let key = [b'k'; 32];
        let header = hex("aed3f1ba74be5f265a1b0191f31edd27c3562569c53e90a3");
        let state = State::from_header(&header, &key).unwrap().serialize();
        let output = unsafe {
            let pointer = elephc_sodium_secretstream_push(
                state.as_ptr(),
                state.len(),
                b"hello".as_ptr(),
                5,
                b"".as_ptr(),
                0,
                0,
            );
            assert!(!pointer.is_null());
            let length = crate::elephc_sodium_last_len();
            std::slice::from_raw_parts(pointer, length).to_vec()
        };
        assert_eq!(&output[52..], &hex("ee6221186394aaaafee93cf16d06b24496bf15186db1")[..]);
    }

    /// Pulls the PHP/libsodium fixture through the C ABI.
    #[test]
    fn php_pull_vector() {
        let key = [b'k'; 32];
        let header = hex("aed3f1ba74be5f265a1b0191f31edd27c3562569c53e90a3");
        let state = State::from_header(&header, &key).unwrap().serialize();
        let ciphertext = hex("ee6221186394aaaafee93cf16d06b24496bf15186db1");
        let output = unsafe {
            let pointer = elephc_sodium_secretstream_pull(
                state.as_ptr(),
                state.len(),
                ciphertext.as_ptr(),
                ciphertext.len(),
                b"".as_ptr(),
                0,
            );
            assert!(!pointer.is_null(), "secretstream pull failed");
            let length = crate::elephc_sodium_last_len();
            std::slice::from_raw_parts(pointer, length).to_vec()
        };
        assert_eq!(output[52], 0);
        assert_eq!(&output[53..], b"hello");
    }
}
