//! Purpose:
//! Declares the OpenSSL 3.6.4 libcrypto EVP/provider C ABI used by the cipher bridge.
//!
//! Called from:
//! - `crate::catalog` and `crate::engine`.
//!
//! Key details:
//! - Only the opaque provider/EVP surface is declared; no bindgen, headers, or
//!   OpenSSL `-sys` crate is involved in the Rust build.
//! - Every pointer is owned by the caller unless the function documents a new reference.

#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int, c_void};

/// Opaque OpenSSL library context.
#[repr(C)]
pub(crate) struct OSSL_LIB_CTX {
    _private: [u8; 0],
}

/// Opaque loaded provider handle.
#[repr(C)]
pub(crate) struct OSSL_PROVIDER {
    _private: [u8; 0],
}

/// Opaque `EVP_CIPHER` implementation.
#[repr(C)]
pub(crate) struct EVP_CIPHER {
    _private: [u8; 0],
}

/// Opaque `EVP_CIPHER_CTX` operation state.
#[repr(C)]
pub(crate) struct EVP_CIPHER_CTX {
    _private: [u8; 0],
}

/// `EVP_CIPH_GCM_MODE`.
pub(crate) const EVP_CIPH_GCM_MODE: c_int = 0x6;
/// `EVP_CIPH_CCM_MODE`.
pub(crate) const EVP_CIPH_CCM_MODE: c_int = 0x7;
/// `EVP_CIPH_OCB_MODE`.
pub(crate) const EVP_CIPH_OCB_MODE: c_int = 0x10003;
/// `EVP_CTRL_AEAD_SET_IVLEN`.
pub(crate) const EVP_CTRL_AEAD_SET_IVLEN: c_int = 0x9;
/// `EVP_CTRL_AEAD_GET_TAG`.
pub(crate) const EVP_CTRL_AEAD_GET_TAG: c_int = 0x10;
/// `EVP_CTRL_AEAD_SET_TAG`.
pub(crate) const EVP_CTRL_AEAD_SET_TAG: c_int = 0x11;

extern "C" {
    /// Loads one built-in provider by name.
    pub(crate) fn OSSL_PROVIDER_load(
        ctx: *mut OSSL_LIB_CTX,
        name: *const c_char,
    ) -> *mut OSSL_PROVIDER;
    /// Enumerates every cipher implementation provided by the loaded providers.
    pub(crate) fn EVP_CIPHER_do_all_provided(
        ctx: *mut OSSL_LIB_CTX,
        callback: extern "C" fn(*mut EVP_CIPHER, *mut c_void),
        arg: *mut c_void,
    );
    /// Fetches one named cipher implementation, returning a new reference.
    pub(crate) fn EVP_CIPHER_fetch(
        ctx: *mut OSSL_LIB_CTX,
        algorithm: *const c_char,
        properties: *const c_char,
    ) -> *mut EVP_CIPHER;
    /// Releases a cipher reference returned by `EVP_CIPHER_fetch`.
    pub(crate) fn EVP_CIPHER_free(cipher: *mut EVP_CIPHER);
    /// Returns the canonical provider name of a fetched cipher.
    pub(crate) fn EVP_CIPHER_get0_name(cipher: *const EVP_CIPHER) -> *const c_char;
    /// Returns the cipher's key length in bytes.
    pub(crate) fn EVP_CIPHER_get_key_length(cipher: *const EVP_CIPHER) -> c_int;
    /// Returns the cipher's default IV length in bytes.
    pub(crate) fn EVP_CIPHER_get_iv_length(cipher: *const EVP_CIPHER) -> c_int;
    /// Returns the cipher's block size in bytes.
    pub(crate) fn EVP_CIPHER_get_block_size(cipher: *const EVP_CIPHER) -> c_int;
    /// Returns the cipher's implementation mode.
    pub(crate) fn EVP_CIPHER_get_mode(cipher: *const EVP_CIPHER) -> c_int;
    /// Allocates an empty cipher operation context.
    pub(crate) fn EVP_CIPHER_CTX_new() -> *mut EVP_CIPHER_CTX;
    /// Frees a cipher operation context.
    pub(crate) fn EVP_CIPHER_CTX_free(ctx: *mut EVP_CIPHER_CTX);
    /// Enables or disables EVP block padding.
    pub(crate) fn EVP_CIPHER_CTX_set_padding(ctx: *mut EVP_CIPHER_CTX, padding: c_int) -> c_int;
    /// Overrides the context key length for variable-key ciphers.
    pub(crate) fn EVP_CIPHER_CTX_set_key_length(ctx: *mut EVP_CIPHER_CTX, len: c_int) -> c_int;
    /// Applies one AEAD control command.
    pub(crate) fn EVP_CIPHER_CTX_ctrl(
        ctx: *mut EVP_CIPHER_CTX,
        command: c_int,
        arg: c_int,
        ptr: *mut c_void,
    ) -> c_int;
    /// Initializes a cipher context for encryption or decryption.
    pub(crate) fn EVP_CipherInit_ex2(
        ctx: *mut EVP_CIPHER_CTX,
        cipher: *const EVP_CIPHER,
        key: *const u8,
        iv: *const u8,
        enc: c_int,
        params: *const c_void,
    ) -> c_int;
    /// Feeds AAD or payload bytes through the cipher.
    pub(crate) fn EVP_CipherUpdate(
        ctx: *mut EVP_CIPHER_CTX,
        out: *mut u8,
        out_len: *mut c_int,
        input: *const u8,
        input_len: c_int,
    ) -> c_int;
    /// Finishes an encryption operation and emits the final block.
    pub(crate) fn EVP_EncryptFinal_ex(
        ctx: *mut EVP_CIPHER_CTX,
        out: *mut u8,
        out_len: *mut c_int,
    ) -> c_int;
    /// Finishes a decryption operation after padding/authentication checks.
    pub(crate) fn EVP_DecryptFinal_ex(
        ctx: *mut EVP_CIPHER_CTX,
        out: *mut u8,
        out_len: *mut c_int,
    ) -> c_int;
}
