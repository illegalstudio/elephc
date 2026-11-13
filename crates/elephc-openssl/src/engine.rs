//! Purpose:
//! Ports PHP 8.5's libcrypto EVP cipher path: key/IV normalization, AEAD flag
//! handling, padding, tag publication, and raw encrypt/decrypt output.
//!
//! Called from:
//! - `crate::abi` for the `openssl_encrypt`/`openssl_decrypt` C ABI calls.
//!
//! Key details:
//! - The initialization order mirrors `php_openssl_cipher_init`: base init, IV
//!   validation/AEAD IV length, CCM/OCB tag length, decrypt tag, key normalization,
//!   key/IV init, then optional zero-padding.
//! - SIV/GCM-SIV and wrap/CTS/XTS are deliberately not classified as PHP's AEAD
//!   surface, matching the v3 backend's `php_openssl_load_cipher_mode` behavior.

use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use crate::ffi::{
    EVP_CIPHER_CTX, EVP_CIPHER_CTX_ctrl, EVP_CIPHER_CTX_free, EVP_CIPHER_CTX_new,
    EVP_CIPHER_CTX_set_key_length, EVP_CIPHER_CTX_set_padding, EVP_CIPHER_fetch,
    EVP_CIPHER_free, EVP_CIPHER_get_block_size, EVP_CIPHER_get_iv_length,
    EVP_CIPHER_get_key_length, EVP_CIPHER_get_mode, EVP_CIPHER, EVP_CIPH_CCM_MODE,
    EVP_CIPH_GCM_MODE, EVP_CIPH_OCB_MODE, EVP_CipherInit_ex2,
    EVP_CipherUpdate, EVP_DecryptFinal_ex, EVP_EncryptFinal_ex, EVP_CTRL_AEAD_GET_TAG,
    EVP_CTRL_AEAD_SET_IVLEN, EVP_CTRL_AEAD_SET_TAG,
};
use crate::CipherError;

/// PHP's `OPENSSL_RAW_DATA` bit; the bridge always returns raw bytes.
pub(crate) const OPENSSL_RAW_DATA: u32 = 1;
/// PHP's `OPENSSL_ZERO_PADDING` bit.
pub(crate) const OPENSSL_ZERO_PADDING: u32 = 2;
/// PHP's `OPENSSL_DONT_ZERO_PAD_KEY` bit.
pub(crate) const OPENSSL_DONT_ZERO_PAD_KEY: u32 = 4;
const SUPPORTED_OPTIONS: u32 = OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING | OPENSSL_DONT_ZERO_PAD_KEY;

/// Raw cipher output plus the optional AEAD authentication tag.
pub(crate) struct EncryptOutput {
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) tag: Vec<u8>,
}

/// Owns one fetched `EVP_CIPHER` reference.
struct CipherHandle(*mut EVP_CIPHER);

impl Drop for CipherHandle {
    /// Releases the fetched cipher reference.
    fn drop(&mut self) {
        unsafe { EVP_CIPHER_free(self.0) };
    }
}

/// Owns one allocated `EVP_CIPHER_CTX`.
struct CipherContext(*mut EVP_CIPHER_CTX);

impl CipherContext {
    /// Allocates an empty cipher context.
    fn new() -> Result<Self, CipherError> {
        let context = unsafe { EVP_CIPHER_CTX_new() };
        if context.is_null() {
            return Err(CipherError::InvalidArgument);
        }
        Ok(Self(context))
    }

    /// Returns the raw context pointer.
    fn as_ptr(&self) -> *mut EVP_CIPHER_CTX {
        self.0
    }
}

impl Drop for CipherContext {
    /// Frees the cipher context after every operation.
    fn drop(&mut self) {
        unsafe { EVP_CIPHER_CTX_free(self.0) };
    }
}

/// One cipher's fixed shape and PHP mode classification.
struct CipherInfo {
    key_len: usize,
    iv_len: usize,
    block_size: usize,
    is_aead: bool,
    is_single_run_aead: bool,
    set_tag_length_always: bool,
    set_tag_length_when_encrypting: bool,
}

/// One key buffer and the optional `set_key_length` override PHP applies.
struct KeyPlan {
    bytes: Vec<u8>,
    override_length: Option<(c_int, bool)>,
}

/// One IV view, either borrowed from the caller or padded/truncated to the cipher's
/// required length.
enum IvPlan<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl IvPlan<'_> {
    /// Returns the bytes to pass to `EVP_CipherInit_ex2`.
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

/// Fetches one named provider cipher and reads its immutable shape.
fn fetch(name: &[u8]) -> Result<(CipherHandle, CipherInfo), CipherError> {
    let name = std::str::from_utf8(name).map_err(|_| CipherError::UnknownCipher)?;
    let lowered = name.to_ascii_lowercase();
    let is_chacha20_poly1305 = lowered == "chacha20-poly1305";
    let name = CString::new(lowered).map_err(|_| CipherError::UnknownCipher)?;
    let cipher = unsafe { EVP_CIPHER_fetch(std::ptr::null_mut(), name.as_ptr(), std::ptr::null()) };
    if cipher.is_null() {
        return Err(CipherError::UnknownCipher);
    }
    let handle = CipherHandle(cipher);
    let mode = unsafe { EVP_CIPHER_get_mode(cipher) };
    let is_aead = matches!(mode, EVP_CIPH_GCM_MODE | EVP_CIPH_CCM_MODE | EVP_CIPH_OCB_MODE)
        || is_chacha20_poly1305;
    let info = CipherInfo {
        key_len: usize::try_from(unsafe { EVP_CIPHER_get_key_length(cipher) })
            .map_err(|_| CipherError::BadKey)?,
        iv_len: usize::try_from(unsafe { EVP_CIPHER_get_iv_length(cipher) })
            .map_err(|_| CipherError::BadIv)?,
        block_size: usize::try_from(unsafe { EVP_CIPHER_get_block_size(cipher) })
            .unwrap_or(1)
            .max(1),
        is_aead,
        is_single_run_aead: mode == EVP_CIPH_CCM_MODE,
        set_tag_length_always: mode == EVP_CIPH_OCB_MODE,
        set_tag_length_when_encrypting: mode == EVP_CIPH_CCM_MODE,
    };
    Ok((handle, info))
}

/// Rejects unknown option bits while accepting PHP's raw-data bit as a no-op here.
fn validate_options(options: u32) -> Result<(), CipherError> {
    if options & !SUPPORTED_OPTIONS == 0 {
        Ok(())
    } else {
        Err(CipherError::InvalidArgument)
    }
}

/// Applies PHP's short-key zero padding, long-key extension, and DONT_ZERO_PAD rule.
fn key_plan(info: &CipherInfo, key: &[u8], options: u32) -> KeyPlan {
    if info.key_len > key.len() {
        let mut bytes = vec![0; info.key_len];
        bytes[..key.len()].copy_from_slice(key);
        let override_length = (options & OPENSSL_DONT_ZERO_PAD_KEY != 0)
            .then_some(usize_to_int(key.len()))
            .flatten()
            .map(|length| (length, true));
        KeyPlan { bytes, override_length }
    } else if key.len() > info.key_len {
        KeyPlan {
            bytes: key.to_vec(),
            override_length: usize_to_int(key.len()).map(|length| (length, false)),
        }
    } else {
        KeyPlan { bytes: key.to_vec(), override_length: None }
    }
}

/// Applies PHP's IV length rules for AEAD and block ciphers.
fn iv_plan<'a>(
    context: &CipherContext,
    info: &CipherInfo,
    iv: &'a [u8],
) -> Result<IvPlan<'a>, CipherError> {
    if info.is_aead {
        let length = usize_to_int(iv.len()).ok_or(CipherError::BadIv)?;
        let status = unsafe {
            EVP_CIPHER_CTX_ctrl(context.as_ptr(), EVP_CTRL_AEAD_SET_IVLEN, length, std::ptr::null_mut())
        };
        if status <= 0 {
            return Err(CipherError::BadIv);
        }
        return Ok(IvPlan::Borrowed(iv));
    }
    if iv.len() == info.iv_len {
        return Ok(IvPlan::Borrowed(iv));
    }
    let mut padded = vec![0; info.iv_len];
    let copied = iv.len().min(info.iv_len);
    padded[..copied].copy_from_slice(&iv[..copied]);
    Ok(IvPlan::Owned(padded))
}

/// Initializes the cipher and writes its key/IV material.
fn init(
    context: &CipherContext,
    handle: &CipherHandle,
    info: &CipherInfo,
    key: &[u8],
    iv: &[u8],
    options: u32,
    tag: &[u8],
    tag_len: usize,
    enc: bool,
) -> Result<(), CipherError> {
    let enc_flag = c_int::from(enc);
    let base = unsafe {
        EVP_CipherInit_ex2(
            context.as_ptr(),
            handle.0,
            std::ptr::null(),
            std::ptr::null(),
            enc_flag,
            std::ptr::null(),
        )
    };
    if base <= 0 {
        return Err(CipherError::InvalidArgument);
    }

    let iv = iv_plan(context, info, iv)?;
    if info.set_tag_length_always || (enc && info.set_tag_length_when_encrypting) {
        let length = usize_to_int(tag_len).ok_or(CipherError::BadTagLength)?;
        let status = unsafe {
            EVP_CIPHER_CTX_ctrl(context.as_ptr(), EVP_CTRL_AEAD_SET_TAG, length, std::ptr::null_mut())
        };
        if status <= 0 {
            return Err(CipherError::BadTagLength);
        }
    }
    if !enc && tag_len > 0 && info.is_aead {
        let length = usize_to_int(tag_len).ok_or(CipherError::BadTagLength)?;
        let status = unsafe {
            EVP_CIPHER_CTX_ctrl(
                context.as_ptr(),
                EVP_CTRL_AEAD_SET_TAG,
                length,
                tag.as_ptr() as *mut c_void,
            )
        };
        if status <= 0 {
            return Err(CipherError::DecryptFailed);
        }
    }

    let key = key_plan(info, key, options);
    if let Some((length, required)) = key.override_length {
        let status = unsafe { EVP_CIPHER_CTX_set_key_length(context.as_ptr(), length) };
        if status <= 0 && required {
            return Err(CipherError::BadKey);
        }
    }

    let key_ptr = if key.bytes.is_empty() { std::ptr::null() } else { key.bytes.as_ptr() };
    let iv_bytes = iv.as_slice();
    let iv_ptr = if iv_bytes.is_empty() { std::ptr::null() } else { iv_bytes.as_ptr() };
    let status = unsafe {
        EVP_CipherInit_ex2(
            context.as_ptr(),
            std::ptr::null(),
            key_ptr,
            iv_ptr,
            enc_flag,
            std::ptr::null(),
        )
    };
    if status <= 0 {
        return Err(if enc { CipherError::BadKey } else { CipherError::DecryptFailed });
    }
    if options & OPENSSL_ZERO_PADDING != 0 {
        unsafe { EVP_CIPHER_CTX_set_padding(context.as_ptr(), 0) };
    }
    Ok(())
}

/// Runs one raw EVP encryption or decryption.
fn operate(
    name: &[u8],
    data: &[u8],
    key: &[u8],
    iv: &[u8],
    options: u32,
    aad: &[u8],
    tag: &[u8],
    tag_len: usize,
    enc: bool,
) -> Result<EncryptOutput, CipherError> {
    validate_options(options)?;
    let (handle, info) = fetch(name)?;
    if data.len() > i32::MAX as usize || aad.len() > i32::MAX as usize || key.len() > i32::MAX as usize
        || iv.len() > i32::MAX as usize || tag.len() > i32::MAX as usize || tag_len > 16
    {
        return Err(CipherError::InvalidArgument);
    }
    // PHP's GET_TAG path requires a 1..=16-byte tag; a zero or oversized
    // requested length is a failed call even when a by-reference target exists.
    if enc && info.is_aead && !(1..=16).contains(&tag_len) {
        return Err(CipherError::BadTagLength);
    }
    let context = CipherContext::new()?;
    init(&context, &handle, &info, key, iv, options, tag, tag_len, enc)?;

    if info.is_single_run_aead {
        let mut length = 0;
        let status = unsafe {
            EVP_CipherUpdate(
                context.as_ptr(),
                std::ptr::null_mut(),
                &mut length,
                std::ptr::null(),
                data.len() as c_int,
            )
        };
        if status <= 0 {
            return Err(CipherError::DecryptFailed);
        }
    }
    if info.is_aead {
        let mut length = 0;
        let status = unsafe {
            EVP_CipherUpdate(
                context.as_ptr(),
                std::ptr::null_mut(),
                &mut length,
                aad.as_ptr(),
                aad.len() as c_int,
            )
        };
        if status <= 0 {
            return Err(CipherError::DecryptFailed);
        }
    }

    let mut output = vec![0u8; data.len().saturating_add(info.block_size).saturating_add(16)];
    let mut written = 0;
    let status = unsafe {
        EVP_CipherUpdate(
            context.as_ptr(),
            output.as_mut_ptr(),
            &mut written,
            data.as_ptr(),
            data.len() as c_int,
        )
    };
    if status <= 0 {
        return Err(if enc { CipherError::BadPlaintextLength } else { CipherError::DecryptFailed });
    }
    let mut total = usize::try_from(written).map_err(|_| CipherError::InvalidArgument)?;
    if enc {
        let mut final_len = 0;
        let status = unsafe {
            EVP_EncryptFinal_ex(context.as_ptr(), output.as_mut_ptr().add(total), &mut final_len)
        };
        if status <= 0 {
            return Err(CipherError::BadPlaintextLength);
        }
        total += usize::try_from(final_len).map_err(|_| CipherError::InvalidArgument)?;
    } else if !info.is_single_run_aead {
        let mut final_len = 0;
        let status = unsafe {
            EVP_DecryptFinal_ex(context.as_ptr(), output.as_mut_ptr().add(total), &mut final_len)
        };
        if status <= 0 {
            return Err(CipherError::DecryptFailed);
        }
        total += usize::try_from(final_len).map_err(|_| CipherError::InvalidArgument)?;
    }
    output.truncate(total);

    let tag = if enc && info.is_aead && tag_len > 0 {
        let mut tag = vec![0u8; tag_len];
        let status = unsafe {
            EVP_CIPHER_CTX_ctrl(
                context.as_ptr(),
                EVP_CTRL_AEAD_GET_TAG,
                tag_len as c_int,
                tag.as_mut_ptr() as *mut c_void,
            )
        };
        if status <= 0 {
            return Err(CipherError::BadTagLength);
        }
        tag
    } else {
        Vec::new()
    };
    Ok(EncryptOutput { ciphertext: output, tag })
}

/// Encrypts raw bytes under one provider cipher.
pub(crate) fn encrypt(
    name: &[u8],
    data: &[u8],
    key: &[u8],
    iv: &[u8],
    options: u32,
    aad: &[u8],
    tag_len: usize,
) -> Result<EncryptOutput, CipherError> {
    operate(name, data, key, iv, options, aad, &[], tag_len, true)
}

/// Decrypts raw bytes under one provider cipher.
pub(crate) fn decrypt(
    name: &[u8],
    data: &[u8],
    key: &[u8],
    iv: &[u8],
    options: u32,
    aad: &[u8],
    tag: &[u8],
) -> Result<Vec<u8>, CipherError> {
    operate(name, data, key, iv, options, aad, tag, tag.len(), false).map(|output| output.ciphertext)
}

/// Converts a length into a non-negative C `int`, rejecting overflow.
fn usize_to_int(length: usize) -> Option<c_int> {
    c_int::try_from(length).ok()
}
