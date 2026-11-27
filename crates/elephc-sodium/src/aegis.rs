//! Purpose:
//! Pure-Rust AEGIS-128L and AEGIS-256 AEAD implementations for PHP 8.6's
//! `sodium_crypto_aead_aegis*` families.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - AEGIS uses the AES round function (SubBytes, ShiftRows, MixColumns, XOR
//!   round key) rather than full AES encryption; `aes_round` is local so the
//!   construction stays target-independent.
//! - Ciphertexts are `ciphertext || 32-byte tag`, matching libsodium's
//!   `crypto_aead_aegis128l_ABYTES`/`aegis256_ABYTES`.

use std::ptr;

use crate::{bytes, guarded, publish};

/// AES S-box.
const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

/// Initialisation constant c0.
const C0: [u8; 16] = [
    0x00, 0x01, 0x01, 0x02, 0x03, 0x05, 0x08, 0x0d, 0x15, 0x22, 0x37, 0x59, 0x90, 0xe9, 0x79, 0x62,
];

/// Initialisation constant c1.
const C1: [u8; 16] = [
    0xdb, 0x3d, 0x18, 0x55, 0x6d, 0xc2, 0x2f, 0xf1, 0x20, 0x11, 0x31, 0x42, 0x73, 0xb5, 0x28, 0xdd,
];

/// One AES encryption round with `key` as the round key.
fn aes_round(input: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
    let mut shifted = [0u8; 16];
    for column in 0..4 {
        for row in 0..4 {
            shifted[column * 4 + row] = SBOX[input[((column + row) % 4) * 4 + row] as usize];
        }
    }
    let mut output = [0u8; 16];
    for column in 0..4 {
        let a0 = shifted[column * 4];
        let a1 = shifted[column * 4 + 1];
        let a2 = shifted[column * 4 + 2];
        let a3 = shifted[column * 4 + 3];
        let x0 = xtime(a0);
        let x1 = xtime(a1);
        let x2 = xtime(a2);
        let x3 = xtime(a3);
        output[column * 4] = x0 ^ (x1 ^ a1) ^ a2 ^ a3 ^ key[column * 4];
        output[column * 4 + 1] = a0 ^ x1 ^ (x2 ^ a2) ^ a3 ^ key[column * 4 + 1];
        output[column * 4 + 2] = a0 ^ a1 ^ x2 ^ (x3 ^ a3) ^ key[column * 4 + 2];
        output[column * 4 + 3] = (x0 ^ a0) ^ a1 ^ a2 ^ x3 ^ key[column * 4 + 3];
    }
    output
}

/// Multiplies by two in AES's GF(2^8).
fn xtime(value: u8) -> u8 {
    (value << 1) ^ (0x1b & (0u8.wrapping_sub(value >> 7)))
}

/// XORs two 16-byte blocks.
fn xor(left: &[u8; 16], right: &[u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for index in 0..16 {
        out[index] = left[index] ^ right[index];
    }
    out
}

/// Bitwise AND of two 16-byte blocks.
fn and(left: &[u8; 16], right: &[u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for index in 0..16 {
        out[index] = left[index] & right[index];
    }
    out
}

/// Loads a 16-byte block from `data` at `offset`, zero-padding short input.
fn load(data: &[u8], offset: usize) -> [u8; 16] {
    let mut block = [0u8; 16];
    let end = (offset + 16).min(data.len());
    if offset < end {
        block[..end - offset].copy_from_slice(&data[offset..end]);
    }
    block
}

/// Builds a block from two little-endian 64-bit halves.
fn load64x2(first: u64, second: u64) -> [u8; 16] {
    let mut block = [0u8; 16];
    block[..8].copy_from_slice(&first.to_le_bytes());
    block[8..].copy_from_slice(&second.to_le_bytes());
    block
}

/// Constant-time comparison.
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

// -- AEGIS-128L -----------------------------------------------------------

/// AEGIS-128L state update.
fn update128l(state: &mut [[u8; 16]; 8], d1: &[u8; 16], d2: &[u8; 16]) {
    let tmp = state[7];
    for index in (1..8).rev() {
        state[index] = aes_round(&state[index - 1], &state[index]);
    }
    state[0] = aes_round(&tmp, &state[0]);
    state[0] = xor(&state[0], d1);
    state[4] = xor(&state[4], d2);
}

/// AEGIS-128L initialisation.
fn init128l(key: &[u8], nonce: &[u8]) -> Option<[[u8; 16]; 8]> {
    if key.len() != 16 || nonce.len() != 16 {
        return None;
    }
    let k = load(key, 0);
    let n = load(nonce, 0);
    let mut state = [
        xor(&k, &n),
        C1,
        C0,
        C1,
        xor(&k, &n),
        xor(&k, &C0),
        xor(&k, &C1),
        xor(&k, &C0),
    ];
    for _ in 0..10 {
        update128l(&mut state, &n, &k);
    }
    Some(state)
}

/// AEGIS-128L absorb one 32-byte block.
fn absorb128l(state: &mut [[u8; 16]; 8], data: &[u8]) {
    let msg0 = load(data, 0);
    let msg1 = load(data, 16);
    update128l(state, &msg0, &msg1);
}

/// AEGIS-128L encrypt one 32-byte block.
fn encrypt128l(state: &mut [[u8; 16]; 8], message: &[u8], output: &mut [u8], length: usize) {
    let msg0 = load(message, 0);
    let msg1 = load(message, 16);
    let tmp0 = xor(
        &xor(&xor(&msg0, &state[6]), &state[1]),
        &and(&state[2], &state[3]),
    );
    let tmp1 = xor(
        &xor(&xor(&msg1, &state[5]), &state[2]),
        &and(&state[6], &state[7]),
    );
    output[..16].copy_from_slice(&tmp0);
    output[16..32].copy_from_slice(&tmp1);
    let _ = length;
    update128l(state, &msg0, &msg1);
}

/// AEGIS-128L decrypt one 32-byte block.
fn decrypt128l(state: &mut [[u8; 16]; 8], ciphertext: &[u8], output: &mut [u8]) {
    let msg0 = load(ciphertext, 0);
    let msg1 = load(ciphertext, 16);
    let plain0 = xor(
        &xor(&xor(&msg0, &state[6]), &state[1]),
        &and(&state[2], &state[3]),
    );
    let plain1 = xor(
        &xor(&xor(&msg1, &state[5]), &state[2]),
        &and(&state[6], &state[7]),
    );
    output[..16].copy_from_slice(&plain0);
    output[16..32].copy_from_slice(&plain1);
    update128l(state, &plain0, &plain1);
}

/// AEGIS-128L decrypt the final partial block, zero-padding the plaintext
/// before updating the state like libsodium's `aegis128l_declast`.
fn decrypt_last128l(
    state: &mut [[u8; 16]; 8],
    ciphertext: &[u8],
    output: &mut [u8],
    length: usize,
) {
    let msg0 = load(ciphertext, 0);
    let msg1 = load(ciphertext, 16);
    let plain0 = xor(
        &xor(&xor(&msg0, &state[6]), &state[1]),
        &and(&state[2], &state[3]),
    );
    let plain1 = xor(
        &xor(&xor(&msg1, &state[5]), &state[2]),
        &and(&state[6], &state[7]),
    );
    let mut padded = [0u8; 32];
    padded[..16].copy_from_slice(&plain0);
    padded[16..].copy_from_slice(&plain1);
    for byte in &mut padded[length..] {
        *byte = 0;
    }
    output[..length].copy_from_slice(&padded[..length]);
    let p0: [u8; 16] = padded[..16].try_into().expect("block");
    let p1: [u8; 16] = padded[16..].try_into().expect("block");
    update128l(state, &p0, &p1);
}

/// AEGIS-128L finalisation tag.
fn mac128l(state: &mut [[u8; 16]; 8], adlen: u64, mlen: u64) -> [u8; 32] {
    let mut tmp = load64x2(adlen << 3, mlen << 3);
    tmp = xor(&tmp, &state[2]);
    for _ in 0..7 {
        update128l(state, &tmp, &tmp);
    }
    let first = xor(
        &xor(&xor(&state[3], &state[2]), &xor(&state[1], &state[0])),
        &[0u8; 16],
    );
    let second = xor(
        &xor(&xor(&state[7], &state[6]), &xor(&state[5], &state[4])),
        &[0u8; 16],
    );
    let mut tag = [0u8; 32];
    tag[..16].copy_from_slice(&first);
    tag[16..].copy_from_slice(&second);
    tag
}

// -- AEGIS-256 ------------------------------------------------------------

/// AEGIS-256 state update.
fn update256(state: &mut [[u8; 16]; 6], d: &[u8; 16]) {
    let tmp = state[5];
    for index in (1..6).rev() {
        state[index] = aes_round(&state[index - 1], &state[index]);
    }
    state[0] = aes_round(&tmp, &state[0]);
    state[0] = xor(&state[0], d);
}

/// AEGIS-256 initialisation.
fn init256(key: &[u8], nonce: &[u8]) -> Option<[[u8; 16]; 6]> {
    if key.len() != 32 || nonce.len() != 32 {
        return None;
    }
    let k0 = load(key, 0);
    let k1 = load(key, 16);
    let n0 = load(nonce, 0);
    let n1 = load(nonce, 16);
    let k0n0 = xor(&k0, &n0);
    let k1n1 = xor(&k1, &n1);
    let mut state = [
        k0n0,
        k1n1,
        C1,
        C0,
        xor(&k0, &C0),
        xor(&k1, &C1),
    ];
    for _ in 0..4 {
        update256(&mut state, &k0);
        update256(&mut state, &k1);
        update256(&mut state, &k0n0);
        update256(&mut state, &k1n1);
    }
    Some(state)
}

/// AEGIS-256 absorb one 16-byte block.
fn absorb256(state: &mut [[u8; 16]; 6], data: &[u8]) {
    let msg = load(data, 0);
    update256(state, &msg);
}

/// AEGIS-256 encrypt one 16-byte block.
fn encrypt256(state: &mut [[u8; 16]; 6], message: &[u8], output: &mut [u8]) {
    let msg = load(message, 0);
    let tmp = xor(
        &xor(&xor(&xor(&msg, &state[5]), &state[4]), &state[1]),
        &and(&state[2], &state[3]),
    );
    output[..16].copy_from_slice(&tmp);
    update256(state, &msg);
}

/// AEGIS-256 decrypt one 16-byte block.
fn decrypt256(state: &mut [[u8; 16]; 6], ciphertext: &[u8], output: &mut [u8]) {
    let msg = load(ciphertext, 0);
    let plain = xor(
        &xor(&xor(&xor(&msg, &state[5]), &state[4]), &state[1]),
        &and(&state[2], &state[3]),
    );
    output[..16].copy_from_slice(&plain);
    update256(state, &plain);
}

/// AEGIS-256 decrypt the final partial block, zero-padding the plaintext
/// before updating the state like libsodium's `aegis256_declast`.
fn decrypt_last256(
    state: &mut [[u8; 16]; 6],
    ciphertext: &[u8],
    output: &mut [u8],
    length: usize,
) {
    let msg = load(ciphertext, 0);
    let plain = xor(
        &xor(&xor(&xor(&msg, &state[5]), &state[4]), &state[1]),
        &and(&state[2], &state[3]),
    );
    let mut padded = [0u8; 16];
    padded.copy_from_slice(&plain);
    for byte in &mut padded[length..] {
        *byte = 0;
    }
    output[..length].copy_from_slice(&padded[..length]);
    update256(state, &padded);
}

/// AEGIS-256 finalisation tag.
fn mac256(state: &mut [[u8; 16]; 6], adlen: u64, mlen: u64) -> [u8; 32] {
    let mut tmp = load64x2(adlen << 3, mlen << 3);
    tmp = xor(&tmp, &state[3]);
    for _ in 0..7 {
        update256(state, &tmp);
    }
    let first = xor(&xor(&state[2], &state[1]), &state[0]);
    let second = xor(&xor(&state[5], &state[4]), &state[3]);
    let mut tag = [0u8; 32];
    tag[..16].copy_from_slice(&first);
    tag[16..].copy_from_slice(&second);
    tag
}

/// AEGIS-128L seal.
fn seal128l(key: &[u8], nonce: &[u8], aad: &[u8], message: &[u8]) -> Option<Vec<u8>> {
    let mut state = init128l(key, nonce)?;
    let mut offset = 0;
    while offset + 64 <= aad.len() {
        absorb128l(&mut state, &aad[offset..]);
        absorb128l(&mut state, &aad[offset + 32..]);
        offset += 64;
    }
    while offset + 32 <= aad.len() {
        absorb128l(&mut state, &aad[offset..]);
        offset += 32;
    }
    if offset < aad.len() {
        let mut padded = [0u8; 32];
        padded[..aad.len() - offset].copy_from_slice(&aad[offset..]);
        absorb128l(&mut state, &padded);
    }
    let mut output = Vec::with_capacity(message.len() + 32);
    let mut position = 0;
    while position + 32 <= message.len() {
        let mut block = [0u8; 32];
        encrypt128l(&mut state, &message[position..], &mut block, 32);
        output.extend_from_slice(&block);
        position += 32;
    }
    if position < message.len() {
        let mut padded = [0u8; 32];
        padded[..message.len() - position].copy_from_slice(&message[position..]);
        let mut block = [0u8; 32];
        encrypt128l(&mut state, &padded, &mut block, 32);
        output.extend_from_slice(&block[..message.len() - position]);
    }
    output.extend_from_slice(&mac128l(&mut state, aad.len() as u64, message.len() as u64));
    Some(output)
}

/// AEGIS-128L open.
fn open128l(key: &[u8], nonce: &[u8], aad: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    if ciphertext.len() < 32 {
        return None;
    }
    let body = &ciphertext[..ciphertext.len() - 32];
    let tag = &ciphertext[ciphertext.len() - 32..];
    let mut state = init128l(key, nonce)?;
    let mut offset = 0;
    while offset + 64 <= aad.len() {
        absorb128l(&mut state, &aad[offset..]);
        absorb128l(&mut state, &aad[offset + 32..]);
        offset += 64;
    }
    while offset + 32 <= aad.len() {
        absorb128l(&mut state, &aad[offset..]);
        offset += 32;
    }
    if offset < aad.len() {
        let mut padded = [0u8; 32];
        padded[..aad.len() - offset].copy_from_slice(&aad[offset..]);
        absorb128l(&mut state, &padded);
    }
    let mut output = Vec::with_capacity(body.len());
    let mut position = 0;
    while position + 32 <= body.len() {
        let mut block = [0u8; 32];
        decrypt128l(&mut state, &body[position..], &mut block);
        output.extend_from_slice(&block);
        position += 32;
    }
    if position < body.len() {
        let remaining = body.len() - position;
        let mut padded = [0u8; 32];
        padded[..remaining].copy_from_slice(&body[position..]);
        let mut block = [0u8; 32];
        decrypt_last128l(&mut state, &padded, &mut block, remaining);
        output.extend_from_slice(&block[..remaining]);
    }
    let computed = mac128l(&mut state, aad.len() as u64, body.len() as u64);
    if !constant_time_eq(&computed, tag) {
        return None;
    }
    Some(output)
}

/// AEGIS-256 seal.
fn seal256(key: &[u8], nonce: &[u8], aad: &[u8], message: &[u8]) -> Option<Vec<u8>> {
    let mut state = init256(key, nonce)?;
    let mut offset = 0;
    while offset + 32 <= aad.len() {
        absorb256(&mut state, &aad[offset..]);
        absorb256(&mut state, &aad[offset + 16..]);
        offset += 32;
    }
    while offset + 16 <= aad.len() {
        absorb256(&mut state, &aad[offset..]);
        offset += 16;
    }
    if offset < aad.len() {
        let mut padded = [0u8; 16];
        padded[..aad.len() - offset].copy_from_slice(&aad[offset..]);
        absorb256(&mut state, &padded);
    }
    let mut output = Vec::with_capacity(message.len() + 32);
    let mut position = 0;
    while position + 16 <= message.len() {
        let mut block = [0u8; 16];
        encrypt256(&mut state, &message[position..], &mut block);
        output.extend_from_slice(&block);
        position += 16;
    }
    if position < message.len() {
        let mut padded = [0u8; 16];
        padded[..message.len() - position].copy_from_slice(&message[position..]);
        let mut block = [0u8; 16];
        encrypt256(&mut state, &padded, &mut block);
        output.extend_from_slice(&block[..message.len() - position]);
    }
    output.extend_from_slice(&mac256(&mut state, aad.len() as u64, message.len() as u64));
    Some(output)
}

/// AEGIS-256 open.
fn open256(key: &[u8], nonce: &[u8], aad: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    if ciphertext.len() < 32 {
        return None;
    }
    let body = &ciphertext[..ciphertext.len() - 32];
    let tag = &ciphertext[ciphertext.len() - 32..];
    let mut state = init256(key, nonce)?;
    let mut offset = 0;
    while offset + 32 <= aad.len() {
        absorb256(&mut state, &aad[offset..]);
        absorb256(&mut state, &aad[offset + 16..]);
        offset += 32;
    }
    while offset + 16 <= aad.len() {
        absorb256(&mut state, &aad[offset..]);
        offset += 16;
    }
    if offset < aad.len() {
        let mut padded = [0u8; 16];
        padded[..aad.len() - offset].copy_from_slice(&aad[offset..]);
        absorb256(&mut state, &padded);
    }
    let mut output = Vec::with_capacity(body.len());
    let mut position = 0;
    while position + 16 <= body.len() {
        let mut block = [0u8; 16];
        decrypt256(&mut state, &body[position..], &mut block);
        output.extend_from_slice(&block);
        position += 16;
    }
    if position < body.len() {
        let remaining = body.len() - position;
        let mut padded = [0u8; 16];
        padded[..remaining].copy_from_slice(&body[position..]);
        let mut block = [0u8; 16];
        decrypt_last256(&mut state, &padded, &mut block, remaining);
        output.extend_from_slice(&block[..remaining]);
    }
    let computed = mac256(&mut state, aad.len() as u64, body.len() as u64);
    if !constant_time_eq(&computed, tag) {
        return None;
    }
    Some(output)
}

/// C ABI: AEGIS-128L encryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_aegis128l_encrypt(
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
        seal128l(key, nonce, aad, message)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: AEGIS-128L decryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_aegis128l_decrypt(
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
        open128l(key, nonce, aad, ciphertext)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: AEGIS-256 encryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_aegis256_encrypt(
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
        seal256(key, nonce, aad, message)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: AEGIS-256 decryption.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_aead_aegis256_decrypt(
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
        open256(key, nonce, aad, ciphertext)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}
