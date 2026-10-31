//! Purpose:
//! libcrypto-backed OpenSSL cipher bridge staticlib for elephc: the PHP-visible
//! 135-method default-provider catalog plus raw EVP encrypt/decrypt operations.
//!
//! Called from:
//! - Compiled PHP programs through the `__rt_openssl_*` runtime helpers.
//! - `cargo test -p elephc-openssl` with a pinned `ELEPHC_OPENSSL_LIB_DIR`.
//!
//! Key details:
//! - The bridge never links libcrypto itself; the compiler resolves the managed
//!   `openssl-crypto` package and places its `libcrypto.a` after this archive.
//! - PHP's base64/raw output selection and tag by-reference plumbing stay in the
//!   AOT runtime helpers; this ABI always moves raw bytes.
//! - Every public entry point catches Rust panics and maps them to `-8`.

mod abi;
mod catalog;
mod engine;
mod ffi;

pub use abi::{
    elephc_openssl_cipher_count, elephc_openssl_cipher_iv_length, elephc_openssl_cipher_methods,
    elephc_openssl_decrypt, elephc_openssl_encrypt,
};

/// Successful cipher ABI operation.
pub const CIPHER_OK: i32 = 0;
/// The requested cipher name is outside the default-provider catalog.
pub const CIPHER_ERR_UNKNOWN: i32 = -1;
/// A short key was supplied with `OPENSSL_DONT_ZERO_PAD_KEY`.
pub const CIPHER_ERR_BAD_KEY: i32 = -2;
/// The IV length/tag setup was rejected.
pub const CIPHER_ERR_BAD_IV: i32 = -3;
/// Encryption received input EVP could not encode under the selected options.
pub const CIPHER_ERR_BAD_PLAINTEXT_LENGTH: i32 = -4;
/// Decryption failed because of padding, length, or authentication.
pub const CIPHER_ERR_DECRYPT_FAILED: i32 = -5;
/// An AEAD tag length or tag operation was rejected.
pub const CIPHER_ERR_BAD_TAG_LENGTH: i32 = -6;
/// A caller-provided output buffer is too small.
pub const CIPHER_ERR_OUTPUT_TOO_SMALL: i32 = -7;
/// Options or C ABI pointer arguments are invalid.
pub const CIPHER_ERR_INVALID_ARGUMENT: i32 = -8;

/// Error category returned by the EVP engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CipherError {
    UnknownCipher,
    BadKey,
    BadIv,
    BadPlaintextLength,
    DecryptFailed,
    BadTagLength,
    OutputTooSmall,
    InvalidArgument,
}

impl CipherError {
    /// Converts an internal error into the stable negative C ABI status code.
    pub(crate) fn status(self) -> i32 {
        match self {
            Self::UnknownCipher => CIPHER_ERR_UNKNOWN,
            Self::BadKey => CIPHER_ERR_BAD_KEY,
            Self::BadIv => CIPHER_ERR_BAD_IV,
            Self::BadPlaintextLength => CIPHER_ERR_BAD_PLAINTEXT_LENGTH,
            Self::DecryptFailed => CIPHER_ERR_DECRYPT_FAILED,
            Self::BadTagLength => CIPHER_ERR_BAD_TAG_LENGTH,
            Self::OutputTooSmall => CIPHER_ERR_OUTPUT_TOO_SMALL,
            Self::InvalidArgument => CIPHER_ERR_INVALID_ARGUMENT,
        }
    }
}
