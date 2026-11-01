//! Purpose:
//! libsodium-compatible `crypto_box` keypairs and sealed boxes for PHP's `sodium_crypto_box_*`.
//!
//! Called from:
//! - Compiled PHP program assembly through the `_elephc_crypto_sodium_fn` slot.
//! - Magician's eval builtins, which link this crate as an rlib.
//!
//! Key details:
//! - One entry point keyed by `SODIUM_OP_*` keeps the C ABI to a single function-pointer slot.
//! - A keypair is libsodium's layout: 32 secret-key bytes followed by the 32 public-key bytes.
//! - Sealed boxes are X25519 + XSalsa20-Poly1305 with the BLAKE2b-derived nonce, byte-compatible
//!   with libsodium's `crypto_box_seal`, so php and a compiled program read each other's vaults.
//! - Argument-length errors and authentication failures are distinct statuses: PHP throws
//!   `SodiumException` for the first and returns `false` for the second.

use crypto_box::aead::OsRng;
use crypto_box::{PublicKey, SecretKey};

/// `sodium_crypto_box_keypair()`: no inputs, 64 output bytes.
pub const SODIUM_OP_BOX_KEYPAIR: u32 = 1;
/// `sodium_crypto_box_publickey($key_pair)`: 32 output bytes.
pub const SODIUM_OP_BOX_PUBLICKEY: u32 = 2;
/// `sodium_crypto_box_seal($message, $public_key)`: message length + 48 output bytes.
pub const SODIUM_OP_BOX_SEAL: u32 = 3;
/// `sodium_crypto_box_seal_open($ciphertext, $key_pair)`: ciphertext length - 48 output bytes.
pub const SODIUM_OP_BOX_SEAL_OPEN: u32 = 4;

/// The operation succeeded and wrote `*out_len` bytes.
pub const SODIUM_OK: i32 = 0;
/// Argument #1 has the wrong length (PHP throws `SodiumException`).
pub const SODIUM_ERR_ARG1_LENGTH: i32 = -1;
/// Argument #2 has the wrong length (PHP throws `SodiumException`).
pub const SODIUM_ERR_ARG2_LENGTH: i32 = -2;
/// Authentication failed or the ciphertext is too short (PHP returns `false`).
pub const SODIUM_ERR_OPEN_FAILED: i32 = -3;
/// The caller's buffer cannot hold the result.
pub const SODIUM_ERR_OUTPUT_TOO_SMALL: i32 = -4;
/// Unknown operation or the system random source failed.
pub const SODIUM_ERR_INTERNAL: i32 = -5;

const KEY_BYTES: usize = 32;
const KEYPAIR_BYTES: usize = 64;
/// Ephemeral public key plus the Poly1305 tag.
pub const SEAL_BYTES: usize = 48;

/// Runs one sodium operation on raw byte inputs, returning the output bytes or a status.
pub fn sodium_op(op: u32, first: &[u8], second: &[u8]) -> Result<Vec<u8>, i32> {
    match op {
        SODIUM_OP_BOX_KEYPAIR => {
            let secret = SecretKey::generate(&mut OsRng);
            let mut pair = secret.to_bytes().to_vec();
            pair.extend_from_slice(secret.public_key().as_bytes());
            Ok(pair)
        }
        SODIUM_OP_BOX_PUBLICKEY => {
            if first.len() != KEYPAIR_BYTES {
                return Err(SODIUM_ERR_ARG1_LENGTH);
            }
            Ok(first[KEY_BYTES..].to_vec())
        }
        SODIUM_OP_BOX_SEAL => {
            let key: [u8; KEY_BYTES] = second.try_into().map_err(|_| SODIUM_ERR_ARG2_LENGTH)?;
            PublicKey::from(key)
                .seal(&mut OsRng, first)
                .map_err(|_| SODIUM_ERR_INTERNAL)
        }
        SODIUM_OP_BOX_SEAL_OPEN => {
            if second.len() != KEYPAIR_BYTES {
                return Err(SODIUM_ERR_ARG2_LENGTH);
            }
            if first.len() < SEAL_BYTES {
                return Err(SODIUM_ERR_OPEN_FAILED);
            }
            let mut key = [0_u8; KEY_BYTES];
            key.copy_from_slice(&second[..KEY_BYTES]);
            SecretKey::from(key)
                .unseal(first)
                .map_err(|_| SODIUM_ERR_OPEN_FAILED)
        }
        _ => Err(SODIUM_ERR_INTERNAL),
    }
}

/// Returns the output capacity an operation needs for inputs of the given lengths.
#[no_mangle]
pub extern "C" fn elephc_crypto_sodium_capacity(op: u32, first_len: usize) -> usize {
    match op {
        SODIUM_OP_BOX_KEYPAIR => KEYPAIR_BYTES,
        SODIUM_OP_BOX_PUBLICKEY => KEY_BYTES,
        SODIUM_OP_BOX_SEAL => first_len.saturating_add(SEAL_BYTES),
        _ => first_len,
    }
}

/// C ABI over [`sodium_op`]: writes the result into `out_ptr[..out_cap]` and its length to
/// `*out_len`, returning a `SODIUM_*` status.
///
/// # Safety
/// Each pointer/length pair must describe readable memory (a null pointer is accepted for a
/// zero length); `out_ptr` must be writable for `out_cap` bytes and `out_len` must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_sodium(
    op: u32,
    first_ptr: *const u8,
    first_len: usize,
    second_ptr: *const u8,
    second_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    *out_len = 0;
    let result = sodium_op(
        op,
        crate::slice(first_ptr, first_len),
        crate::slice(second_ptr, second_len),
    );
    match result {
        Ok(bytes) if bytes.len() > out_cap => SODIUM_ERR_OUTPUT_TOO_SMALL,
        Ok(bytes) => {
            if !bytes.is_empty() {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, bytes.len());
            }
            *out_len = bytes.len();
            SODIUM_OK
        }
        Err(status) => status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn keypair_round_trips_through_seal() {
        let pair = sodium_op(SODIUM_OP_BOX_KEYPAIR, &[], &[]).unwrap();
        assert_eq!(pair.len(), 64);
        let public = sodium_op(SODIUM_OP_BOX_PUBLICKEY, &pair, &[]).unwrap();
        assert_eq!(public, pair[32..]);
        let sealed = sodium_op(SODIUM_OP_BOX_SEAL, b"probe-value", &public).unwrap();
        assert_eq!(sealed.len(), 11 + SEAL_BYTES);
        let opened = sodium_op(SODIUM_OP_BOX_SEAL_OPEN, &sealed, &pair).unwrap();
        assert_eq!(opened, b"probe-value");
    }

    #[test]
    fn wrong_lengths_and_bad_boxes_are_distinct_statuses() {
        assert_eq!(sodium_op(SODIUM_OP_BOX_PUBLICKEY, b"x", &[]), Err(SODIUM_ERR_ARG1_LENGTH));
        assert_eq!(sodium_op(SODIUM_OP_BOX_SEAL, b"m", b"x"), Err(SODIUM_ERR_ARG2_LENGTH));
        assert_eq!(sodium_op(SODIUM_OP_BOX_SEAL_OPEN, b"c", b"x"), Err(SODIUM_ERR_ARG2_LENGTH));
        let pair = [7_u8; 64];
        assert_eq!(sodium_op(SODIUM_OP_BOX_SEAL_OPEN, b"short", &pair), Err(SODIUM_ERR_OPEN_FAILED));
        assert_eq!(sodium_op(SODIUM_OP_BOX_SEAL_OPEN, &[0_u8; 60], &pair), Err(SODIUM_ERR_OPEN_FAILED));
    }

    /// A keypair and sealed box produced by php 8.5's libsodium open here, and a public key
    /// derived here matches libsodium's: the two sides must read each other's secrets vaults.
    #[test]
    fn opens_a_box_sealed_by_libsodium() {
        let pair = unhex(LIBSODIUM_KEYPAIR);
        let sealed = unhex(LIBSODIUM_SEALED);
        let opened = sodium_op(SODIUM_OP_BOX_SEAL_OPEN, &sealed, &pair).unwrap();
        assert_eq!(opened, b"sealed by libsodium");
        let secret: [u8; 32] = pair[..32].try_into().unwrap();
        assert_eq!(SecretKey::from(secret).public_key().as_bytes(), &pair[32..]);
    }

    const LIBSODIUM_KEYPAIR: &str = "ed734e9903f45f221cb0ce1355425925d7bca73cdb62d60f507d4357d3abc649d2e6e33181889e912ae79956dfe2f7162d7fdbad3bafaee44ce3e764d731f170";
    const LIBSODIUM_SEALED: &str = "293a9115f819046278ff63c323739c0c342cc7b525fbafafbb9141a87973e16971263e1ed24ee377530f27d38ba1dae483433b9c9c93ca343a471affbc3f723e8a13f4";
}
