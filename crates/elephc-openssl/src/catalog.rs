//! Purpose:
//! Reproduces PHP 8.5's `openssl_get_cipher_methods()` catalog from the OpenSSL
//! 3.6.4 default provider: `EVP_CIPHER_do_all_provided`, lowercase names, sorted.
//!
//! Called from:
//! - `crate::abi` for the C method-inventory and IV-length entry points.
//!
//! Key details:
//! - The provider is loaded once; the packed catalog and every default IV length
//!   are cached together, so repeated `openssl_cipher_iv_length()` calls are map
//!   lookups rather than provider fetches.
//! - No legacy provider is loaded, matching PHP's default 135-name list. Loading
//!   legacy would add BF/CAST/DES/IDEA/RC2/RC4/SEED names PHP does not report.
//! - Names outside the default catalog (aliases such as `aes128`) fall back to a
//!   single `EVP_CIPHER_fetch` so PHP's case-insensitive alias support remains.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::sync::OnceLock;

use crate::ffi::{
    EVP_CIPHER_do_all_provided, EVP_CIPHER_fetch, EVP_CIPHER_free, EVP_CIPHER_get0_name,
    EVP_CIPHER_get_iv_length, OSSL_PROVIDER_load,
};
use crate::CipherError;

/// One immutable default-provider snapshot.
struct Catalog {
    packed: Vec<u8>,
    count: usize,
    iv_lengths: HashMap<Vec<u8>, i32>,
}

/// Names and IV lengths collected during the one provider enumeration.
struct CatalogBuilder {
    names: Vec<String>,
    iv_lengths: HashMap<Vec<u8>, i32>,
}

/// The process-wide catalog snapshot.
static CATALOG: OnceLock<Catalog> = OnceLock::new();

/// Loads the default provider once for this process.
fn ensure_default_provider() {
    static PROVIDER: OnceLock<()> = OnceLock::new();
    PROVIDER.get_or_init(|| {
        let default = CString::new("default").expect("static provider name");
        // A loaded static provider is intentionally retained for the process lifetime.
        let _ = unsafe { OSSL_PROVIDER_load(std::ptr::null_mut(), default.as_ptr()) };
    });
}

/// Collects one provider cipher name and its default IV length.
extern "C" fn collect_name(cipher: *mut crate::ffi::EVP_CIPHER, arg: *mut c_void) {
    if cipher.is_null() || arg.is_null() {
        return;
    }
    let name = unsafe { EVP_CIPHER_get0_name(cipher) };
    if name.is_null() {
        return;
    }
    let name = unsafe { CStr::from_ptr(name) };
    let Ok(name) = name.to_str() else {
        return;
    };
    let name = name.to_ascii_lowercase();
    let iv_length = unsafe { EVP_CIPHER_get_iv_length(cipher) };
    let builder = unsafe { &mut *(arg as *mut CatalogBuilder) };
    builder.iv_lengths.insert(name.as_bytes().to_vec(), iv_length);
    builder.names.push(name);
}

/// Builds the lowercase, sorted, deduplicated default-provider catalog.
fn build_catalog() -> Catalog {
    ensure_default_provider();
    let mut builder = CatalogBuilder { names: Vec::new(), iv_lengths: HashMap::new() };
    unsafe {
        EVP_CIPHER_do_all_provided(
            std::ptr::null_mut(),
            collect_name,
            &mut builder as *mut CatalogBuilder as *mut c_void,
        );
    }
    builder.names.sort();
    builder.names.dedup();
    let mut packed = Vec::with_capacity(builder.names.iter().map(|name| name.len() + 1).sum());
    for name in &builder.names {
        packed.extend_from_slice(name.as_bytes());
        packed.push(0);
    }
    Catalog { packed, count: builder.names.len(), iv_lengths: builder.iv_lengths }
}

/// Returns the cached default-provider snapshot.
fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(build_catalog)
}

/// Returns the cached trailing-NUL packed catalog.
pub(crate) fn catalog_packed() -> &'static [u8] {
    &catalog().packed
}

/// Returns the number of catalog names.
pub(crate) fn catalog_count() -> usize {
    catalog().count
}

/// Returns the default IV length for a cipher name, using the cached catalog first.
pub(crate) fn cipher_iv_length(name: &[u8]) -> Result<usize, CipherError> {
    let name = std::str::from_utf8(name).map_err(|_| CipherError::UnknownCipher)?;
    if let Some(length) = catalog().iv_lengths.get(name.as_bytes()).copied() {
        return usize::try_from(length).map_err(|_| CipherError::UnknownCipher);
    }
    // Uppercase aliases and names outside the default catalog resolve through a
    // lowercase retry, then one provider fetch.
    let lowered = name.to_ascii_lowercase();
    if lowered != name {
        if let Some(length) = catalog().iv_lengths.get(lowered.as_bytes()).copied() {
            return usize::try_from(length).map_err(|_| CipherError::UnknownCipher);
        }
    }
    let name = CString::new(lowered).map_err(|_| CipherError::UnknownCipher)?;
    let cipher = unsafe {
        EVP_CIPHER_fetch(std::ptr::null_mut(), name.as_ptr(), std::ptr::null())
    };
    if cipher.is_null() {
        return Err(CipherError::UnknownCipher);
    }
    let length = unsafe { EVP_CIPHER_get_iv_length(cipher) };
    unsafe { EVP_CIPHER_free(cipher) };
    usize::try_from(length).map_err(|_| CipherError::UnknownCipher)
}
