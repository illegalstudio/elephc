//! Purpose:
//! Traditional PKWARE ZipCrypto state, encryption, decryption, and password storage.
//!
//! Called from:
//! - ZIP PHAR reading and writing.
//!
//! Key details:
//! - Compatibility encryption is thread-local and is not treated as secure confidentiality.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Traditional-PKWARE (ZipCrypto) cipher state: three 32-bit keys advanced per
/// plaintext byte. Drives both reading and writing of encrypted entries.
/// Cryptographically weak — kept only for compatibility with legacy ZipCrypto
/// archives, not as a real confidentiality mechanism.
struct ZipCryptoKeys {
    k0: u32,
    k1: u32,
    k2: u32,
}

impl ZipCryptoKeys {
    /// Seeds the keys from the password (PKWARE's fixed initial constants).
    fn new(password: &[u8]) -> Self {
        let mut keys = Self {
            k0: 0x1234_5678,
            k1: 0x2345_6789,
            k2: 0x3456_7890,
        };
        for &byte in password {
            keys.update(byte);
        }
        keys
    }

    /// Advances the three keys with one plaintext byte.
    fn update(&mut self, byte: u8) {
        self.k0 = crc32_byte(self.k0, byte);
        self.k1 = self.k1.wrapping_add(self.k0 & 0xff);
        self.k1 = self.k1.wrapping_mul(134_775_813).wrapping_add(1);
        self.k2 = crc32_byte(self.k2, (self.k1 >> 24) as u8);
    }

    /// Returns the next keystream byte (derived from `k2`).
    fn keystream(&self) -> u8 {
        let temp = (self.k2 | 2) & 0xffff;
        ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
    }

    /// Decrypts one ciphertext byte and advances the keys with the plaintext.
    fn decrypt(&mut self, cipher: u8) -> u8 {
        let plain = cipher ^ self.keystream();
        self.update(plain);
        plain
    }

    /// Encrypts one plaintext byte and advances the keys with that plaintext.
    fn encrypt(&mut self, plain: u8) -> u8 {
        let cipher = plain ^ self.keystream();
        self.update(plain);
        cipher
    }
}

/// One-byte CRC32 step (poly 0xEDB88320) used by the ZipCrypto key schedule.
pub(super) fn crc32_byte(crc: u32, byte: u8) -> u32 {
    let mut t = (crc ^ byte as u32) & 0xff;
    for _ in 0..8 {
        t = if t & 1 != 0 { (t >> 1) ^ 0xedb8_8320 } else { t >> 1 };
    }
    (crc >> 8) ^ t
}

/// Decrypts a ZipCrypto entry payload (12-byte header + ciphertext) with
/// `password`, returning the post-header plaintext. Returns `None` when the data
/// is too short or the header's check byte rejects the password.
pub(super) fn zipcrypto_decrypt(password: &[u8], data: &[u8], check_byte: u8) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let mut keys = ZipCryptoKeys::new(password);
    let mut header_last = 0u8;
    for &byte in &data[..12] {
        header_last = keys.decrypt(byte);
    }
    if header_last != check_byte {
        return None;
    }
    Some(data[12..].iter().map(|&c| keys.decrypt(c)).collect())
}

/// Encrypts `data` as a traditional-PKWARE (ZipCrypto) entry payload with
/// `password`: prepends a 12-byte encryption header (11 pseudo-random filler bytes
/// plus `check_byte` at index 11) and returns the encrypted `header ++ data`, which
/// is 12 bytes longer than `data`. `check_byte` must be the byte the reader will
/// verify (the CRC's high byte when no data descriptor is used). The first 11 header
/// bytes are never read back, so their randomness affects only resistance to attack,
/// not round-trip correctness.
pub(super) fn zipcrypto_encrypt(password: &[u8], data: &[u8], check_byte: u8) -> Vec<u8> {
    let mut header = [0u8; 12];
    header[..11].copy_from_slice(&zipcrypto_header_filler());
    header[11] = check_byte;
    let mut keys = ZipCryptoKeys::new(password);
    let mut out = Vec::with_capacity(data.len() + 12);
    for &plain in header.iter().chain(data) {
        out.push(keys.encrypt(plain));
    }
    out
}

/// Produces 11 non-constant filler bytes for a ZipCrypto encryption header, mixing
/// a per-call atomic nonce with the current time through an xorshift64* step.
/// Dependency-free; only needs to avoid an all-constant header, since the bytes are
/// discarded on read.
pub(super) fn zipcrypto_header_filler() -> [u8; 11] {
    static NONCE: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = now ^ NONCE.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut filler = [0u8; 11];
    for byte in filler.iter_mut() {
        // xorshift64* advance, then take a high byte of the scrambled state.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        *byte = (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) as u8;
    }
    filler
}

/// Returns the password currently set for reading and writing encrypted ZIP
/// entries, if any.
pub(super) fn current_zip_password() -> Option<Vec<u8>> {
    ZIP_PASSWORD.with(|slot| slot.borrow().clone())
}

/// Sets (or, when empty, clears) the password used to read and write encrypted
/// ZIP entries.
pub(super) fn set_zip_password(password: &[u8]) {
    ZIP_PASSWORD.with(|slot| {
        *slot.borrow_mut() = if password.is_empty() {
            None
        } else {
            Some(password.to_vec())
        };
    });
}
