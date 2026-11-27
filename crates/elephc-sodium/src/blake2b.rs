//! Purpose:
//! Pure-Rust BLAKE2b with libsodium-compatible keyed, salted and personalised
//! modes, exposing a serialisable streaming state for PHP's
//! `sodium_crypto_generichash_*` state strings.
//!
//! Called from:
//! - `crate::hashes`, `crate::kx` and `crate::secretstream` helpers.
//!
//! Key details:
//! - The state is serialised into 384 bytes so `sodium_crypto_generichash_init()`
//!   returns an opaque value that survives PHP copy-by-assignment.
//! - BLAKE2b counts compressed bytes in a 128-bit counter and keeps the last
//!   full block buffered until finalisation, matching the reference algorithm.

use crate::hashes::GH_STATE_BYTES;

/// BLAKE2b IV.
const IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

/// Message schedule permutations.
const SIGMA: [[usize; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];

/// Streaming BLAKE2b state with a fixed output length.
#[derive(Clone)]
pub struct Blake2b {
    h: [u64; 8],
    t: u128,
    buffer: [u8; 128],
    buffered: usize,
    output_len: usize,
}

impl Blake2b {
    /// Creates a state for `output_len` bytes with optional key, salt and personalisation.
    pub fn new(output_len: usize, key: &[u8], salt: &[u8], personal: &[u8]) -> Option<Self> {
        if output_len == 0 || output_len > 64 || key.len() > 64 || salt.len() > 16 || personal.len() > 16
        {
            return None;
        }
        let mut h = IV;
        h[0] ^= 0x0101_0000 ^ ((key.len() as u64) << 8) ^ output_len as u64;
        let mut salt_block = [0u8; 16];
        salt_block[..salt.len()].copy_from_slice(salt);
        let mut personal_block = [0u8; 16];
        personal_block[..personal.len()].copy_from_slice(personal);
        h[4] ^= u64::from_le_bytes(salt_block[..8].try_into().ok()?);
        h[5] ^= u64::from_le_bytes(salt_block[8..].try_into().ok()?);
        h[6] ^= u64::from_le_bytes(personal_block[..8].try_into().ok()?);
        h[7] ^= u64::from_le_bytes(personal_block[8..].try_into().ok()?);
        let mut state = Self {
            h,
            t: 0,
            buffer: [0u8; 128],
            buffered: 0,
            output_len,
        };
        if !key.is_empty() {
            let mut block = [0u8; 128];
            block[..key.len()].copy_from_slice(key);
            state.update(&block);
        }
        Some(state)
    }

    /// Absorbs `data` into the state.
    pub fn update(&mut self, mut data: &[u8]) {
        if self.buffered > 0 {
            let needed = 128 - self.buffered;
            if data.len() <= needed {
                self.buffer[self.buffered..self.buffered + data.len()].copy_from_slice(data);
                self.buffered += data.len();
                return;
            }
            let (head, tail) = data.split_at(needed);
            self.buffer[self.buffered..].copy_from_slice(head);
            self.t += 128;
            let block = self.buffer;
            self.compress(&block, false);
            self.buffered = 0;
            data = tail;
        }
        while data.len() > 128 {
            self.t += 128;
            let block: &[u8; 128] = data[..128].try_into().expect("block length");
            self.compress(block, false);
            data = &data[128..];
        }
        self.buffer[..data.len()].copy_from_slice(data);
        self.buffered = data.len();
    }

    /// Finalises the hash and returns all 64 state bytes.
    ///
    /// `crypto_generichash_final` may request a length different from the one
    /// passed to `init`; libsodium returns a prefix of this 64-byte state.
    pub fn finalize(mut self) -> Vec<u8> {
        self.t += self.buffered as u128;
        for byte in &mut self.buffer[self.buffered..] {
            *byte = 0;
        }
        let block = self.buffer;
        self.compress(&block, true);
        let mut output = Vec::with_capacity(64);
        for word in self.h {
            output.extend_from_slice(&word.to_le_bytes());
        }
        output
    }

    /// Serialises the state into PHP's 384-byte opaque string.
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = vec![0u8; GH_STATE_BYTES];
        let mut position = 0;
        for word in self.h {
            out[position..position + 8].copy_from_slice(&word.to_le_bytes());
            position += 8;
        }
        out[position..position + 16].copy_from_slice(&self.t.to_le_bytes());
        position += 16;
        out[position..position + 128].copy_from_slice(&self.buffer);
        position += 128;
        out[position] = self.buffered as u8;
        out[position + 1] = self.output_len as u8;
        out
    }

    /// Restores a state produced by [`Blake2b::serialize`].
    pub fn deserialize(data: &[u8]) -> Option<Self> {
        if data.len() < GH_STATE_BYTES {
            return None;
        }
        let mut position = 0;
        let mut h = [0u64; 8];
        for word in &mut h {
            *word = u64::from_le_bytes(data[position..position + 8].try_into().ok()?);
            position += 8;
        }
        let t = u128::from_le_bytes(data[position..position + 16].try_into().ok()?);
        position += 16;
        let mut buffer = [0u8; 128];
        buffer.copy_from_slice(&data[position..position + 128]);
        position += 128;
        let buffered = data[position] as usize;
        let output_len = data[position + 1] as usize;
        if buffered > 128 || output_len == 0 || output_len > 64 {
            return None;
        }
        Some(Self {
            h,
            t,
            buffer,
            buffered,
            output_len,
        })
    }

    /// Compresses one 128-byte block.
    fn compress(&mut self, block: &[u8; 128], last: bool) {
        let mut message = [0u64; 16];
        for (index, word) in message.iter_mut().enumerate() {
            *word = u64::from_le_bytes(block[index * 8..index * 8 + 8].try_into().expect("word"));
        }
        let mut v = [0u64; 16];
        v[..8].copy_from_slice(&self.h);
        v[8..].copy_from_slice(&IV);
        v[12] ^= self.t as u64;
        v[13] ^= (self.t >> 64) as u64;
        if last {
            v[14] = !v[14];
        }
        for round in 0..12 {
            let s = SIGMA[round];
            quarter(&mut v, 0, 4, 8, 12, message[s[0]], message[s[1]]);
            quarter(&mut v, 1, 5, 9, 13, message[s[2]], message[s[3]]);
            quarter(&mut v, 2, 6, 10, 14, message[s[4]], message[s[5]]);
            quarter(&mut v, 3, 7, 11, 15, message[s[6]], message[s[7]]);
            quarter(&mut v, 0, 5, 10, 15, message[s[8]], message[s[9]]);
            quarter(&mut v, 1, 6, 11, 12, message[s[10]], message[s[11]]);
            quarter(&mut v, 2, 7, 8, 13, message[s[12]], message[s[13]]);
            quarter(&mut v, 3, 4, 9, 14, message[s[14]], message[s[15]]);
        }
        for index in 0..8 {
            self.h[index] ^= v[index] ^ v[index + 8];
        }
    }
}

/// BLAKE2b mixing function.
fn quarter(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

/// One-shot BLAKE2b helper.
pub fn hash(
    output_len: usize,
    message: &[u8],
    key: &[u8],
    salt: &[u8],
    personal: &[u8],
) -> Option<Vec<u8>> {
    let mut state = Blake2b::new(output_len, key, salt, personal)?;
    state.update(message);
    let mut output = state.finalize();
    output.truncate(output_len);
    Some(output)
}
