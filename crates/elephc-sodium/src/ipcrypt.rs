//! Purpose:
//! Pure-Rust implementation of libsodium 1.0.22's IPcrypt families.
//!
//! Called from:
//! - `crate::` C ABI wrappers for `sodium_crypto_ipcrypt_*`.
//!
//! Key details:
//! - `encrypt`/`decrypt` are plain AES-128; ND adds an 8-byte tweak and uses a
//!   tweaked round-key schedule; NDX uses a 16-byte tweak, a 32-byte key split
//!   into AES-XEX subkeys, and stores the tweak in the output.
//! - PFX performs format-preserving prefix encryption over 128 AES calls and
//!   preserves the IPv4-mapped `/96` prefix.
//! - Every routine is checked against libsodium's `test/default/ipcrypt`
//!   vectors, including the degenerate mid-schedule collision fallbacks.

use crate::aes::{self, Block};

/// `crypto_ipcrypt_BYTES`.
pub(crate) const BYTES: usize = 16;
/// `crypto_ipcrypt_KEYBYTES`.
pub(crate) const KEYBYTES: usize = 16;
/// `crypto_ipcrypt_ND_KEYBYTES`.
pub(crate) const ND_KEYBYTES: usize = 16;
/// `crypto_ipcrypt_ND_TWEAKBYTES`.
pub(crate) const ND_TWEAKBYTES: usize = 8;
/// `crypto_ipcrypt_ND_OUTPUTBYTES`.
pub(crate) const ND_OUTPUTBYTES: usize = 24;
/// `crypto_ipcrypt_NDX_KEYBYTES`.
pub(crate) const NDX_KEYBYTES: usize = 32;
/// `crypto_ipcrypt_NDX_TWEAKBYTES`.
pub(crate) const NDX_TWEAKBYTES: usize = 16;
/// `crypto_ipcrypt_NDX_OUTPUTBYTES`.
pub(crate) const NDX_OUTPUTBYTES: usize = 32;
/// `crypto_ipcrypt_PFX_KEYBYTES`.
pub(crate) const PFX_KEYBYTES: usize = 32;
/// `crypto_ipcrypt_PFX_BYTES`.
pub(crate) const PFX_BYTES: usize = 16;

/// Converts a slice into a fixed 16-byte block.
fn block(bytes: &[u8]) -> Option<Block> {
    bytes.try_into().ok()
}

/// Returns whether a block carries the IPv4-mapped IPv6 prefix.
fn is_ipv4_mapped(block: &Block) -> bool {
    block[..12] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff]
}

/// XORs two blocks.
fn xor(left: &Block, right: &Block) -> Block {
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = left[i] ^ right[i];
    }
    out
}

/// XORs a block in place.
fn xor_in_place(state: &mut Block, other: &Block) {
    for i in 0..16 {
        state[i] ^= other[i];
    }
}

/// Plain AES-128 IPcrypt encryption.
pub(crate) fn encrypt(input: &[u8], key: &[u8]) -> Option<Block> {
    if input.len() != BYTES || key.len() != KEYBYTES {
        return None;
    }
    let input = block(input)?;
    let key: Block = block(key)?;
    Some(aes::encrypt_block(input, &aes::expand_key(&key)))
}

/// Plain AES-128 IPcrypt decryption.
pub(crate) fn decrypt(input: &[u8], key: &[u8]) -> Option<Block> {
    if input.len() != BYTES || key.len() != KEYBYTES {
        return None;
    }
    let input = block(input)?;
    let key: Block = block(key)?;
    Some(aes::decrypt_block(input, &aes::expand_key(&key)))
}

/// Expands libsodium's 8-byte ND tweak into its 32-bit-word block layout.
fn expand_nd_tweak(tweak: &[u8; ND_TWEAKBYTES]) -> Block {
    let mut block = [0u8; 16];
    block[0] = tweak[0];
    block[1] = tweak[1];
    block[4] = tweak[2];
    block[5] = tweak[3];
    block[8] = tweak[4];
    block[9] = tweak[5];
    block[12] = tweak[6];
    block[13] = tweak[7];
    block
}

/// IPcrypt-ND encryption: `tweak || tweaked-AES(input)`.
pub(crate) fn nd_encrypt(input: &[u8], tweak: &[u8], key: &[u8]) -> Option<[u8; ND_OUTPUTBYTES]> {
    if input.len() != BYTES || key.len() != ND_KEYBYTES {
        return None;
    }
    let input = block(input)?;
    let key: Block = block(key)?;
    let tweak: [u8; ND_TWEAKBYTES] = tweak.try_into().ok()?;
    let tweak_block = expand_nd_tweak(&tweak);
    let ciphertext = aes::encrypt_with_tweak(input, &tweak_block, &aes::expand_key(&key));
    let mut out = [0u8; ND_OUTPUTBYTES];
    out[..ND_TWEAKBYTES].copy_from_slice(&tweak);
    out[ND_TWEAKBYTES..].copy_from_slice(&ciphertext);
    Some(out)
}

/// IPcrypt-ND decryption.
pub(crate) fn nd_decrypt(input: &[u8], key: &[u8]) -> Option<Block> {
    if input.len() != ND_OUTPUTBYTES || key.len() != ND_KEYBYTES {
        return None;
    }
    let key: Block = block(key)?;
    let tweak: [u8; ND_TWEAKBYTES] = input[..ND_TWEAKBYTES].try_into().ok()?;
    let tweak_block = expand_nd_tweak(&tweak);
    let ciphertext = block(&input[ND_TWEAKBYTES..])?;
    Some(aes::decrypt_with_tweak(
        ciphertext,
        &tweak_block,
        &aes::expand_key(&key),
    ))
}

/// Returns the NDX key schedules, including libsodium's collision fallback.
fn ndx_keys(key: &[u8; NDX_KEYBYTES]) -> ([Block; 11], [Block; 11]) {
    let tkeys = aes::expand_key(&{
        let mut block = [0u8; 16];
        block.copy_from_slice(&key[16..]);
        block
    });
    let mut rkeys = aes::expand_key(&{
        let mut block = [0u8; 16];
        block.copy_from_slice(&key[..16]);
        block
    });
    if tkeys[5] == rkeys[5] {
        let mut diff = [0u8; 16];
        for i in 0..16 {
            diff[i] = key[i] ^ 0x5a;
        }
        rkeys = aes::expand_key(&diff);
    }
    (tkeys, rkeys)
}

/// IPcrypt-NDX encryption: `tweak || AES-XEX(input)`.
pub(crate) fn ndx_encrypt(
    input: &[u8],
    tweak: &[u8],
    key: &[u8],
) -> Option<[u8; NDX_OUTPUTBYTES]> {
    if input.len() != BYTES || tweak.len() != NDX_TWEAKBYTES || key.len() != NDX_KEYBYTES {
        return None;
    }
    let input = block(input)?;
    let key: [u8; NDX_KEYBYTES] = key.try_into().ok()?;
    let tweak: Block = block(tweak)?;
    let (tkeys, rkeys) = ndx_keys(&key);
    let tt = aes::encrypt_block(tweak, &tkeys);
    let mut state = xor(&input, &tt);
    xor_in_place(&mut state, &rkeys[0]);
    for round in &rkeys[1..10] {
        aes::encrypt_round(&mut state, round);
    }
    let last = xor(&rkeys[10], &tt);
    aes::encrypt_last(&mut state, &last);
    let mut out = [0u8; NDX_OUTPUTBYTES];
    out[..16].copy_from_slice(&tweak);
    out[16..].copy_from_slice(&state);
    Some(out)
}

/// IPcrypt-NDX decryption.
pub(crate) fn ndx_decrypt(input: &[u8], key: &[u8]) -> Option<Block> {
    if input.len() != NDX_OUTPUTBYTES || key.len() != NDX_KEYBYTES {
        return None;
    }
    let key: [u8; NDX_KEYBYTES] = key.try_into().ok()?;
    let tweak = block(&input[..16])?;
    let ciphertext = block(&input[16..])?;
    let (tkeys, rkeys) = ndx_keys(&key);
    let inverted = aes::invert_key_schedule(&rkeys);
    let tt = aes::encrypt_block(tweak, &tkeys);
    let mut state = xor(&ciphertext, &tt);
    xor_in_place(&mut state, &inverted[10]);
    for round in inverted[1..10].iter().rev() {
        aes::decrypt_inverted_round(&mut state, round);
    }
    let last = xor(&inverted[0], &tt);
    aes::decrypt_inverted_last(&mut state, &last);
    Some(state)
}

/// Returns a PFX key schedule pair, including libsodium's collision fallback.
fn pfx_keys(key: &[u8; PFX_KEYBYTES]) -> ([Block; 11], [Block; 11]) {
    let k1 = aes::expand_key(&{
        let mut block = [0u8; 16];
        block.copy_from_slice(&key[..16]);
        block
    });
    let mut k2 = aes::expand_key(&{
        let mut block = [0u8; 16];
        block.copy_from_slice(&key[16..]);
        block
    });
    if k1[5] == k2[5] {
        let mut diff = [0u8; 16];
        for i in 0..16 {
            diff[i] = key[i] ^ 0x5a;
        }
        k2 = aes::expand_key(&diff);
    }
    (k1, k2)
}

/// Reads one PFX bit using libsodium's least-significant-bit-first ordering.
fn get_bit(block: &Block, bit: usize) -> u8 {
    (block[15 - bit / 8] >> (bit % 8)) & 1
}

/// Writes one PFX bit using libsodium's least-significant-bit-first ordering.
fn set_bit(block: &mut Block, bit: usize, value: u8) {
    let byte = 15 - bit / 8;
    let mask = 1u8 << (bit % 8);
    let value = if value & 1 != 0 { 0xff } else { 0 };
    block[byte] = (block[byte] & !mask) | (mask & value);
}

/// Shifts a 16-byte PFX prefix left by one bit.
fn shift_left(block: &mut Block) {
    for i in 0..15 {
        block[i] = (block[i] << 1) | (block[i + 1] >> 7);
    }
    block[15] <<= 1;
}

/// Builds libsodium's padded PFX prefix marker.
fn pad_prefix(prefix_start: usize) -> Block {
    let mut padded = [0u8; 16];
    if prefix_start == 0 {
        padded[15] = 0x01;
    } else {
        padded[3] = 0x01;
        padded[14] = 0xff;
        padded[15] = 0xff;
    }
    padded
}

/// IPcrypt-PFX encryption.
pub(crate) fn pfx_encrypt(input: &[u8], key: &[u8]) -> Option<Block> {
    if input.len() != PFX_BYTES || key.len() != PFX_KEYBYTES {
        return None;
    }
    let input = block(input)?;
    let key: [u8; PFX_KEYBYTES] = key.try_into().ok()?;
    let (k1, k2) = pfx_keys(&key);
    let prefix_start = if is_ipv4_mapped(&input) { 96 } else { 0 };
    let mut padded = pad_prefix(prefix_start);
    let mut encrypted = [0u8; 16];
    if prefix_start == 96 {
        encrypted[10] = 0xff;
        encrypted[11] = 0xff;
    }
    for prefix_len_bits in prefix_start..128 {
        let e1 = aes::encrypt_block(padded, &k1);
        let e2 = aes::encrypt_block(padded, &k2);
        let t = xor(&e1, &e2);
        let cipher_bit = t[15] & 1;
        let bit_pos = 127 - prefix_len_bits;
        let original_bit = get_bit(&input, bit_pos);
        set_bit(&mut encrypted, bit_pos, original_bit ^ cipher_bit);
        shift_left(&mut padded);
        set_bit(&mut padded, 0, original_bit);
    }
    Some(encrypted)
}

/// IPcrypt-PFX decryption.
pub(crate) fn pfx_decrypt(input: &[u8], key: &[u8]) -> Option<Block> {
    if input.len() != PFX_BYTES || key.len() != PFX_KEYBYTES {
        return None;
    }
    let input = block(input)?;
    let key: [u8; PFX_KEYBYTES] = key.try_into().ok()?;
    let (k1, k2) = pfx_keys(&key);
    let prefix_start = if is_ipv4_mapped(&input) { 96 } else { 0 };
    let mut padded = pad_prefix(prefix_start);
    let mut decrypted = [0u8; 16];
    if prefix_start == 96 {
        decrypted[10] = 0xff;
        decrypted[11] = 0xff;
    }
    for prefix_len_bits in prefix_start..128 {
        let e1 = aes::encrypt_block(padded, &k1);
        let e2 = aes::encrypt_block(padded, &k2);
        let t = xor(&e1, &e2);
        let cipher_bit = t[15] & 1;
        let bit_pos = 127 - prefix_len_bits;
        let encrypted_bit = get_bit(&input, bit_pos);
        let original_bit = encrypted_bit ^ cipher_bit;
        set_bit(&mut decrypted, bit_pos, original_bit);
        shift_left(&mut padded);
        set_bit(&mut padded, 0, original_bit);
    }
    Some(decrypted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes a lowercase hex string into bytes.
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

    /// Builds the IPv4-mapped block for `a.b.c.d`.
    fn ipv4(a: u8, b: u8, c: u8, d: u8) -> Vec<u8> {
        let mut out = vec![0u8; 16];
        out[10] = 0xff;
        out[11] = 0xff;
        out[12] = a;
        out[13] = b;
        out[14] = c;
        out[15] = d;
        out
    }

    /// libsodium's deterministic plain-IPcrypt vectors.
    #[test]
    fn ipcrypt_matches_libsodium_vectors() {
        let cases = [
            ("0123456789abcdeffedcba9876543210", ipv4(0, 0, 0, 0), "bde96789d353824cd7c6f58a6bd226eb"),
            ("1032547698badcfeefcdab8967452301", ipv4(255, 255, 255, 255), "aed292f6ea2358c348fd08b874e845d8"),
            ("2b7e151628aed2a6abf7158809cf4f3c", ipv4(192, 0, 2, 1), "1dbdc1b9fff175867d0b67b4e76e4777"),
        ];
        for (key, input, expected) in cases {
            let ciphertext = encrypt(&input, &hex(key)).expect("valid inputs");
            assert_eq!(ciphertext, hex(expected).as_slice());
            assert_eq!(decrypt(&ciphertext, &hex(key)).unwrap(), input.as_slice());
        }
    }

    /// libsodium's IPcrypt-ND vectors.
    #[test]
    fn ipcrypt_nd_matches_libsodium_vectors() {
        let cases = [
            (
                "0123456789abcdeffedcba9876543210",
                "08e0c289bff23b7c",
                ipv4(0, 0, 0, 0),
                "08e0c289bff23b7cb349aadfe3bcef56221c384c7c217b16",
            ),
            (
                "1032547698badcfeefcdab8967452301",
                "21bd1834bc088cd2",
                ipv4(192, 0, 2, 1),
                "21bd1834bc088cd2e5e1fe55f95876e639faae2594a0caad",
            ),
            (
                "2b7e151628aed2a6abf7158809cf4f3c",
                "b4ecbe30b70898d7",
                {
                    let mut out = vec![0u8; 16];
                    out[0] = 0x20;
                    out[1] = 0x01;
                    out[2] = 0x0d;
                    out[3] = 0xb8;
                    out[15] = 0x01;
                    out
                },
                "b4ecbe30b70898d7553ac8974d1b4250eafc4b0aa1f80c96",
            ),
        ];
        for (key, tweak, input, expected) in cases {
            let ciphertext = nd_encrypt(&input, &hex(tweak), &hex(key)).expect("valid inputs");
            assert_eq!(ciphertext, hex(expected).as_slice());
            assert_eq!(
                nd_decrypt(&ciphertext, &hex(key)).unwrap(),
                input.as_slice()
            );
        }
    }

    /// libsodium's IPcrypt-NDX vectors.
    #[test]
    fn ipcrypt_ndx_matches_libsodium_vectors() {
        let cases = [
            (
                "0123456789abcdeffedcba98765432101032547698badcfeefcdab8967452301",
                "21bd1834bc088cd2b4ecbe30b70898d7",
                ipv4(0, 0, 0, 0),
                "21bd1834bc088cd2b4ecbe30b70898d782db0d4125fdace61db35b8339f20ee5",
            ),
            (
                "1032547698badcfeefcdab89674523010123456789abcdeffedcba9876543210",
                "08e0c289bff23b7cb4ecbe30b70898d7",
                ipv4(192, 0, 2, 1),
                "08e0c289bff23b7cb4ecbe30b70898d7766a533392a69edf1ad0d3ce362ba98a",
            ),
            (
                "2b7e151628aed2a6abf7158809cf4f3c3c4fcf098815f7aba6d2ae2816157e2b",
                "21bd1834bc088cd2b4ecbe30b70898d7",
                {
                    let mut out = vec![0u8; 16];
                    out[0] = 0x20;
                    out[1] = 0x01;
                    out[2] = 0x0d;
                    out[3] = 0xb8;
                    out[15] = 0x01;
                    out
                },
                "21bd1834bc088cd2b4ecbe30b70898d76089c7e05ae30c2d10ca149870a263e4",
            ),
        ];
        for (key, tweak, input, expected) in cases {
            let ciphertext = ndx_encrypt(&input, &hex(tweak), &hex(key)).expect("valid inputs");
            assert_eq!(ciphertext, hex(expected).as_slice());
            assert_eq!(
                ndx_decrypt(&ciphertext, &hex(key)).unwrap(),
                input.as_slice()
            );
        }
    }

    /// libsodium's IPcrypt-PFX vectors and IPv4-mapped prefix preservation.
    #[test]
    fn ipcrypt_pfx_matches_libsodium_vectors() {
        let key = "0123456789abcdeffedcba98765432101032547698badcfeefcdab8967452301";
        let cases = [
            (ipv4(0, 0, 0, 0), "00000000000000000000ffff97529b86"),
            (ipv4(255, 255, 255, 255), "00000000000000000000ffff5eb9a959"),
            (ipv4(192, 0, 2, 1), "00000000000000000000ffff64734883"),
            (
                {
                    let mut out = vec![0u8; 16];
                    out[0] = 0x20;
                    out[1] = 0x01;
                    out[2] = 0x0d;
                    out[3] = 0xb8;
                    out[15] = 0x01;
                    out
                },
                "c1805dd42587352430abfa656ab60f88",
            ),
        ];
        for (input, expected) in cases {
            let ciphertext = pfx_encrypt(&input, &hex(key)).expect("valid inputs");
            assert_eq!(ciphertext, hex(expected).as_slice());
            assert_eq!(
                pfx_decrypt(&ciphertext, &hex(key)).unwrap(),
                input.as_slice()
            );
        }
    }
}
