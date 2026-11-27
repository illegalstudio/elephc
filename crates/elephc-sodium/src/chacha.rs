//! Purpose:
//! Pure-Rust ChaCha20, XChaCha20 and HChaCha20 keystream primitives used by the
//! sodium AEAD, secretstream and stream functions.
//!
//! Called from:
//! - `crate::aead`, `crate::secretstream` and `crate::stream`.
//!
//! Key details:
//! - Counter-aware keystream generation matches libsodium's IETF (32-bit
//!   counter, 12-byte nonce) and legacy (64-bit counter, 8-byte nonce) layouts.
//! - XChaCha20 derives its subkey with HChaCha20 and then uses a 4-zero-byte
//!   prefix before the final 8 nonce bytes.

/// ChaCha20 quarter round.
fn quarter(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] = (state[d] ^ state[a]).rotate_left(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_left(12);
    state[a] = state[a].wrapping_add(state[b]);
    state[d] = (state[d] ^ state[a]).rotate_left(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_left(7);
}

/// Applies 20 ChaCha rounds to a state in place.
fn rounds(state: &mut [u32; 16]) {
    for _ in 0..10 {
        quarter(state, 0, 4, 8, 12);
        quarter(state, 1, 5, 9, 13);
        quarter(state, 2, 6, 10, 14);
        quarter(state, 3, 7, 11, 15);
        quarter(state, 0, 5, 10, 15);
        quarter(state, 1, 6, 11, 12);
        quarter(state, 2, 7, 8, 13);
        quarter(state, 3, 4, 9, 14);
    }
}

/// Builds one 64-byte IETF ChaCha20 block.
fn ietf_block(key: &[u8; 32], nonce: &[u8; 12], counter: u32) -> [u8; 64] {
    let mut state = [0u32; 16];
    state[0] = 0x6170_7865;
    state[1] = 0x3320_646e;
    state[2] = 0x7962_2d32;
    state[3] = 0x6b20_6574;
    for index in 0..8 {
        state[4 + index] = u32::from_le_bytes(key[index * 4..index * 4 + 4].try_into().expect("key"));
    }
    state[12] = counter;
    for index in 0..3 {
        state[13 + index] =
            u32::from_le_bytes(nonce[index * 4..index * 4 + 4].try_into().expect("nonce"));
    }
    let initial = state;
    rounds(&mut state);
    let mut output = [0u8; 64];
    for index in 0..16 {
        let word = state[index].wrapping_add(initial[index]);
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    output
}

/// Builds one 64-byte legacy ChaCha20 block (64-bit nonce and counter).
fn legacy_block(key: &[u8; 32], nonce: &[u8; 8], counter: u64) -> [u8; 64] {
    let mut state = [0u32; 16];
    state[0] = 0x6170_7865;
    state[1] = 0x3320_646e;
    state[2] = 0x7962_2d32;
    state[3] = 0x6b20_6574;
    for index in 0..8 {
        state[4 + index] = u32::from_le_bytes(key[index * 4..index * 4 + 4].try_into().expect("key"));
    }
    state[12] = counter as u32;
    state[13] = (counter >> 32) as u32;
    for index in 0..2 {
        state[14 + index] =
            u32::from_le_bytes(nonce[index * 4..index * 4 + 4].try_into().expect("nonce"));
    }
    let initial = state;
    rounds(&mut state);
    let mut output = [0u8; 64];
    for index in 0..16 {
        let word = state[index].wrapping_add(initial[index]);
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    output
}

/// XORs `data` with the IETF ChaCha20 keystream starting at `counter`.
pub fn apply_ietf(key: &[u8; 32], nonce: &[u8; 12], counter: u32, data: &mut [u8]) {
    let mut block_index = 0u32;
    for chunk in data.chunks_mut(64) {
        let block = ietf_block(key, nonce, counter.wrapping_add(block_index));
        for (byte, mask) in chunk.iter_mut().zip(block.iter()) {
            *byte ^= *mask;
        }
        block_index = block_index.wrapping_add(1);
    }
}

/// XORs `data` with the legacy ChaCha20 keystream starting at `counter`.
pub fn apply_legacy(key: &[u8; 32], nonce: &[u8; 8], counter: u64, data: &mut [u8]) {
    let mut block_index = 0u64;
    for chunk in data.chunks_mut(64) {
        let block = legacy_block(key, nonce, counter.wrapping_add(block_index));
        for (byte, mask) in chunk.iter_mut().zip(block.iter()) {
            *byte ^= *mask;
        }
        block_index = block_index.wrapping_add(1);
    }
}

/// HChaCha20 subkey derivation.
pub fn hchacha20(key: &[u8; 32], nonce: &[u8; 16]) -> [u8; 32] {
    let mut state = [0u32; 16];
    state[0] = 0x6170_7865;
    state[1] = 0x3320_646e;
    state[2] = 0x7962_2d32;
    state[3] = 0x6b20_6574;
    for index in 0..8 {
        state[4 + index] = u32::from_le_bytes(key[index * 4..index * 4 + 4].try_into().expect("key"));
    }
    for index in 0..4 {
        state[12 + index] =
            u32::from_le_bytes(nonce[index * 4..index * 4 + 4].try_into().expect("nonce"));
    }
    rounds(&mut state);
    let mut output = [0u8; 32];
    for index in 0..4 {
        output[index * 4..index * 4 + 4].copy_from_slice(&state[index].to_le_bytes());
    }
    for index in 0..4 {
        output[16 + index * 4..16 + index * 4 + 4]
            .copy_from_slice(&state[12 + index].to_le_bytes());
    }
    output
}

/// Derives the XChaCha20 subkey for a 24-byte nonce.
pub fn xchacha20_key(key: &[u8; 32], nonce: &[u8; 24]) -> ([u8; 32], [u8; 12]) {
    let mut prefix = [0u8; 16];
    prefix.copy_from_slice(&nonce[..16]);
    let subkey = hchacha20(key, &prefix);
    let mut ietf_nonce = [0u8; 12];
    ietf_nonce[4..].copy_from_slice(&nonce[16..24]);
    (subkey, ietf_nonce)
}

/// XORs `data` with the XChaCha20 keystream.
pub fn apply_xchacha20(key: &[u8; 32], nonce: &[u8; 24], counter: u32, data: &mut [u8]) {
    let (subkey, ietf_nonce) = xchacha20_key(key, nonce);
    apply_ietf(&subkey, &ietf_nonce, counter, data);
}
