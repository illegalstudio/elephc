//! Purpose:
//! Small AES-128 core for libsodium's IPcrypt family.
//!
//! Called from:
//! - `crate::ipcrypt` for standard, tweak-adjusted, and XEX block operations.
//!
//! Key details:
//! - The standard round functions are used directly by IPcrypt, while ND/NDX
//!   need the same rounds with a fixed tweak XORed into the round keys.
//! - The key schedule is exposed so IPcrypt can reproduce libsodium's
//!   mid-schedule collision check for PFX and NDX.
//! - Verified against the FIPS-197 AES-128 vector in the unit tests.

/// One 128-bit AES block.
pub(crate) type Block = [u8; 16];

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

/// Inverse AES S-box.
const INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

/// Multiplies two bytes in GF(2^8) modulo AES's polynomial.
fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut product = 0u8;
    for _ in 0..8 {
        if b & 1 != 0 {
            product ^= a;
        }
        let high = a & 0x80;
        a <<= 1;
        if high != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    product
}

/// Returns `x * 2` in GF(2^8).
fn xtime(x: u8) -> u8 {
    (x << 1) ^ if x & 0x80 != 0 { 0x1b } else { 0 }
}

/// Expands a 128-bit key into 11 round keys.
pub(crate) fn expand_key(key: &Block) -> [Block; 11] {
    let mut words = [0u8; 176];
    words[..16].copy_from_slice(key);
    let mut rcon = 1u8;
    for i in 4..44 {
        let mut temp = [
            words[(i - 1) * 4],
            words[(i - 1) * 4 + 1],
            words[(i - 1) * 4 + 2],
            words[(i - 1) * 4 + 3],
        ];
        if i % 4 == 0 {
            temp.rotate_left(1);
            for byte in &mut temp {
                *byte = SBOX[usize::from(*byte)];
            }
            temp[0] ^= rcon;
            rcon = xtime(rcon);
        }
        for j in 0..4 {
            words[i * 4 + j] = words[(i - 4) * 4 + j] ^ temp[j];
        }
    }
    let mut keys = [[0u8; 16]; 11];
    for (round, key) in keys.iter_mut().enumerate() {
        key.copy_from_slice(&words[round * 16..(round + 1) * 16]);
    }
    keys
}

/// XORs one block into another in place.
fn xor_block(state: &mut Block, key: &Block) {
    for (byte, key) in state.iter_mut().zip(key) {
        *byte ^= key;
    }
}

/// Applies SubBytes to a block.
fn sub_bytes(state: &mut Block) {
    for byte in state {
        *byte = SBOX[usize::from(*byte)];
    }
}

/// Applies InvSubBytes to a block.
fn inv_sub_bytes(state: &mut Block) {
    for byte in state {
        *byte = INV_SBOX[usize::from(*byte)];
    }
}

/// Applies ShiftRows on the column-major state.
fn shift_rows(state: &mut Block) {
    for row in 1..4 {
        let values = [state[row], state[4 + row], state[8 + row], state[12 + row]];
        for column in 0..4 {
            state[4 * column + row] = values[(column + row) % 4];
        }
    }
}

/// Applies InvShiftRows on the column-major state.
fn inv_shift_rows(state: &mut Block) {
    for row in 1..4 {
        let values = [state[row], state[4 + row], state[8 + row], state[12 + row]];
        for column in 0..4 {
            state[4 * column + row] = values[(column + 4 - row) % 4];
        }
    }
}

/// Applies MixColumns to every column.
fn mix_columns(state: &mut Block) {
    for column in 0..4 {
        let i = column * 4;
        let [a0, a1, a2, a3] = [state[i], state[i + 1], state[i + 2], state[i + 3]];
        state[i] = xtime(a0) ^ (xtime(a1) ^ a1) ^ a2 ^ a3;
        state[i + 1] = a0 ^ xtime(a1) ^ (xtime(a2) ^ a2) ^ a3;
        state[i + 2] = a0 ^ a1 ^ xtime(a2) ^ (xtime(a3) ^ a3);
        state[i + 3] = (xtime(a0) ^ a0) ^ a1 ^ a2 ^ xtime(a3);
    }
}

/// Applies InvMixColumns to every column.
fn inv_mix_columns(state: &mut Block) {
    for column in 0..4 {
        let i = column * 4;
        let [a0, a1, a2, a3] = [state[i], state[i + 1], state[i + 2], state[i + 3]];
        state[i] = gmul(a0, 0x0e) ^ gmul(a1, 0x0b) ^ gmul(a2, 0x0d) ^ gmul(a3, 0x09);
        state[i + 1] = gmul(a0, 0x09) ^ gmul(a1, 0x0e) ^ gmul(a2, 0x0b) ^ gmul(a3, 0x0d);
        state[i + 2] = gmul(a0, 0x0d) ^ gmul(a1, 0x09) ^ gmul(a2, 0x0e) ^ gmul(a3, 0x0b);
        state[i + 3] = gmul(a0, 0x0b) ^ gmul(a1, 0x0d) ^ gmul(a2, 0x09) ^ gmul(a3, 0x0e);
    }
}

/// Applies one full AES encryption round with `key`.
pub(crate) fn encrypt_round(state: &mut Block, key: &Block) {
    sub_bytes(state);
    shift_rows(state);
    mix_columns(state);
    xor_block(state, key);
}

/// Applies the final AES encryption round (no MixColumns) with `key`.
pub(crate) fn encrypt_last(state: &mut Block, key: &Block) {
    sub_bytes(state);
    shift_rows(state);
    xor_block(state, key);
}

/// Applies one inverse round against an already-inverted key schedule.
pub(crate) fn decrypt_inverted_round(state: &mut Block, key: &Block) {
    inv_shift_rows(state);
    inv_sub_bytes(state);
    inv_mix_columns(state);
    xor_block(state, key);
}

/// Applies the final inverse round against an already-inverted key schedule.
pub(crate) fn decrypt_inverted_last(state: &mut Block, key: &Block) {
    inv_shift_rows(state);
    inv_sub_bytes(state);
    xor_block(state, key);
}

/// Encrypts one block with a standard AES-128 round-key schedule.
pub(crate) fn encrypt_block(mut state: Block, keys: &[Block; 11]) -> Block {
    xor_block(&mut state, &keys[0]);
    for key in &keys[1..10] {
        encrypt_round(&mut state, key);
    }
    encrypt_last(&mut state, &keys[10]);
    state
}

/// Decrypts one block with a standard AES-128 round-key schedule.
pub(crate) fn decrypt_block(mut state: Block, keys: &[Block; 11]) -> Block {
    xor_block(&mut state, &keys[10]);
    for key in keys[1..10].iter().rev() {
        inv_shift_rows(&mut state);
        inv_sub_bytes(&mut state);
        xor_block(&mut state, key);
        inv_mix_columns(&mut state);
    }
    inv_shift_rows(&mut state);
    inv_sub_bytes(&mut state);
    xor_block(&mut state, &keys[0]);
    state
}

/// Encrypts with IPcrypt-ND's fixed tweak XORed into rounds 1..10.
pub(crate) fn encrypt_with_tweak(
    mut state: Block,
    tweak: &Block,
    keys: &[Block; 11],
) -> Block {
    let mut first = keys[0];
    xor_block(&mut first, tweak);
    xor_block(&mut state, &first);
    for key in &keys[1..10] {
        let mut tweaked = *key;
        xor_block(&mut tweaked, tweak);
        encrypt_round(&mut state, &tweaked);
    }
    let mut last = keys[10];
    xor_block(&mut last, tweak);
    encrypt_last(&mut state, &last);
    state
}

/// Decrypts IPcrypt-ND's fixed-tweak ciphertext.
pub(crate) fn decrypt_with_tweak(
    mut state: Block,
    tweak: &Block,
    keys: &[Block; 11],
) -> Block {
    let inverted = invert_key_schedule(keys);
    let mut tweak_inverse = *tweak;
    inv_mix_columns(&mut tweak_inverse);
    let mut first = inverted[10];
    xor_block(&mut first, tweak);
    xor_block(&mut state, &first);
    for key in inverted[1..10].iter().rev() {
        let mut tweaked = *key;
        xor_block(&mut tweaked, &tweak_inverse);
        decrypt_inverted_round(&mut state, &tweaked);
    }
    let mut last = inverted[0];
    xor_block(&mut last, tweak);
    decrypt_inverted_last(&mut state, &last);
    state
}

/// Returns libsodium's inverse AES-128 key schedule for the middle rounds.
pub(crate) fn invert_key_schedule(keys: &[Block; 11]) -> [Block; 11] {
    let mut inverted = *keys;
    for key in &mut inverted[1..10] {
        inv_mix_columns(key);
    }
    inverted
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

    /// FIPS-197 AES-128 encryption/decryption vector.
    #[test]
    fn aes128_fips197_vector() {
        let key: Block = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let plaintext: Block = hex("00112233445566778899aabbccddeeff").try_into().unwrap();
        let keys = expand_key(&key);
        let ciphertext = encrypt_block(plaintext, &keys);
        assert_eq!(ciphertext, hex("69c4e0d86a7b0430d8cdb78070b4c55a").as_slice());
        assert_eq!(decrypt_block(ciphertext, &keys), plaintext);
    }

    /// A zero tweak makes the tweaked rounds identical to standard AES.
    #[test]
    fn zero_tweak_matches_standard_rounds() {
        let key: Block = hex("2b7e151628aed2a6abf7158809cf4f3c").try_into().unwrap();
        let plaintext: Block = hex("6bc1bee22e409f96e93d7e117393172a").try_into().unwrap();
        let keys = expand_key(&key);
        let zero = [0u8; 16];
        assert_eq!(
            encrypt_with_tweak(plaintext, &zero, &keys),
            encrypt_block(plaintext, &keys)
        );
        let ciphertext = encrypt_block(plaintext, &keys);
        assert_eq!(
            decrypt_with_tweak(ciphertext, &zero, &keys),
            decrypt_block(ciphertext, &keys)
        );
    }
}
