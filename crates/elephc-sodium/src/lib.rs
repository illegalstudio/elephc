//! Purpose:
//! C ABI over the pure-Rust NaCl `crypto_secretbox` implementation for
//! elephc's `sodium_*` surface.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - `crypto_secretbox` is XSalsa20-Poly1305, the exact construction behind
//!   libsodium's `crypto_secretbox_easy`/`crypto_secretbox_open_easy`, so
//!   ciphertexts are byte-compatible with PHP's ext/sodium.
//! - Output buffers are thread-local and valid until the next bridge call on
//!   the same thread; the prelude copies them immediately through
//!   `__elephc_ptr_read_string`.
//! - Invalid key/nonce lengths fail closed with null; the PHP wrapper raises
//!   `SodiumException` before reaching this ABI.

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

mod aes;
mod ipcrypt;
mod kem;
mod pwhash;
mod shake;
use shake::ShakeKind;

use crypto_secretbox::{
    aead::{Aead, KeyInit},
    Nonce, XSalsa20Poly1305,
};


/// `crypto_secretbox_KEYBYTES`.
const KEY_BYTES: usize = 32;
/// `crypto_secretbox_NONCEBYTES`.
const NONCE_BYTES: usize = 24;
/// `crypto_secretbox_MACBYTES`.
const MAC_BYTES: usize = 16;

thread_local! {
    /// Last string returned to PHP. It is replaced on the next call and the
    /// prelude copies it immediately.
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Stores `bytes` in the thread-local output buffer with a trailing NUL and
/// returns the buffer pointer. The pointer stays valid until the next publish.
fn publish(bytes: &[u8]) -> *const u8 {
    OUTPUT.with(|slot| {
        let mut buffer = slot.borrow_mut();
        buffer.clear();
        buffer.extend_from_slice(bytes);
        buffer.push(0);
        buffer.as_ptr()
    })
}

/// Converts a raw C pointer/length pair into bytes.
///
/// # Safety
/// `data` must reference `len` readable bytes when `len` is nonzero.
unsafe fn bytes<'a>(data: *const u8, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        Some(&[])
    } else if data.is_null() {
        None
    } else {
        Some(std::slice::from_raw_parts(data, len))
    }
}

/// Runs `f`, converting a panic into `fallback`.
fn guarded<T: Copy>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// Builds the XSalsa20-Poly1305 cipher from a raw key, rejecting wrong lengths.
fn cipher(key: &[u8]) -> Option<XSalsa20Poly1305> {
    (key.len() == KEY_BYTES)
        .then(|| XSalsa20Poly1305::new_from_slice(key).ok())
        .flatten()
}

/// C ABI: `crypto_secretbox_easy` over the XSalsa20-Poly1305 construction.
///
/// Returns `tag || ciphertext`, or null on a wrong key/nonce length or panic.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_secretbox(
    key: *const u8,
    key_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    message: *const u8,
    message_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(key) = bytes(key, key_len) else {
            return ptr::null();
        };
        let Some(nonce) = bytes(nonce, nonce_len) else {
            return ptr::null();
        };
        let Some(message) = bytes(message, message_len) else {
            return ptr::null();
        };
        if nonce.len() != NONCE_BYTES {
            return ptr::null();
        }
        let Some(cipher) = cipher(key) else {
            return ptr::null();
        };
        match cipher.encrypt(Nonce::from_slice(nonce), message) {
            Ok(ciphertext) => publish(&ciphertext),
            Err(_) => ptr::null(),
        }
    })
}

/// C ABI: `crypto_secretbox_open_easy` over the XSalsa20-Poly1305 construction.
///
/// Returns the plaintext (empty strings included) or null on authentication
/// failure, a short ciphertext, a wrong key/nonce length, or a panic.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_secretbox_open(
    key: *const u8,
    key_len: usize,
    nonce: *const u8,
    nonce_len: usize,
    ciphertext: *const u8,
    ciphertext_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(key) = bytes(key, key_len) else {
            return ptr::null();
        };
        let Some(nonce) = bytes(nonce, nonce_len) else {
            return ptr::null();
        };
        let Some(ciphertext) = bytes(ciphertext, ciphertext_len) else {
            return ptr::null();
        };
        if nonce.len() != NONCE_BYTES || ciphertext.len() < MAC_BYTES {
            return ptr::null();
        }
        let Some(cipher) = cipher(key) else {
            return ptr::null();
        };
        match cipher.decrypt(Nonce::from_slice(nonce), ciphertext) {
            Ok(plaintext) => publish(&plaintext),
            Err(_) => ptr::null(),
        }
    })
}

/// C ABI: SHAKE128 over `message`, returning `length` output bytes.
///
/// # Safety
/// `message` must reference `message_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_shake128(
    length: usize,
    message: *const u8,
    message_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(message) = bytes(message, message_len) else {
            return ptr::null();
        };
        match shake::shake128(length, message) {
            Some(output) => publish(&output),
            None => ptr::null(),
        }
    })
}

/// C ABI: SHAKE256 over `message`, returning `length` output bytes.
///
/// # Safety
/// `message` must reference `message_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_shake256(
    length: usize,
    message: *const u8,
    message_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(message) = bytes(message, message_len) else {
            return ptr::null();
        };
        match shake::shake256(length, message) {
            Some(output) => publish(&output),
            None => ptr::null(),
        }
    })
}

/// C ABI: starts a SHAKE128 state; `domain = -1` selects the standard domain.
#[no_mangle]
pub extern "C" fn elephc_sodium_shake128_init(domain: i64) -> i64 {
    guarded(-1, || shake::init(ShakeKind::Shake128, domain))
}

/// C ABI: starts a SHAKE256 state; `domain = -1` selects the standard domain.
#[no_mangle]
pub extern "C" fn elephc_sodium_shake256_init(domain: i64) -> i64 {
    guarded(-1, || shake::init(ShakeKind::Shake256, domain))
}

/// C ABI: absorbs bytes into a SHAKE128 state; returns 0 or -1.
///
/// # Safety
/// `message` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_shake128_update(
    handle: i64,
    message: *const u8,
    len: usize,
) -> i64 {
    guarded(-1, || {
        let Some(message) = bytes(message, len) else {
            return -1;
        };
        shake::update(handle, message)
    })
}

/// C ABI: absorbs bytes into a SHAKE256 state; returns 0 or -1.
///
/// # Safety
/// `message` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_shake256_update(
    handle: i64,
    message: *const u8,
    len: usize,
) -> i64 {
    guarded(-1, || {
        let Some(message) = bytes(message, len) else {
            return -1;
        };
        shake::update(handle, message)
    })
}

/// C ABI: squeezes bytes from a SHAKE128 state, or null on failure.
#[no_mangle]
pub extern "C" fn elephc_sodium_shake128_squeeze(handle: i64, length: usize) -> *const u8 {
    guarded(ptr::null(), || {
        shake::squeeze(handle, length)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: squeezes bytes from a SHAKE256 state, or null on failure.
#[no_mangle]
pub extern "C" fn elephc_sodium_shake256_squeeze(handle: i64, length: usize) -> *const u8 {
    guarded(ptr::null(), || {
        shake::squeeze(handle, length)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: drops a SHAKE128 state.
#[no_mangle]
pub extern "C" fn elephc_sodium_shake128_drop(handle: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| shake::drop_state(handle)));
}

/// C ABI: drops a SHAKE256 state.
#[no_mangle]
pub extern "C" fn elephc_sodium_shake256_drop(handle: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| shake::drop_state(handle)));
}

/// C ABI: TurboSHAKE128 over `message`, returning `length` output bytes.
///
/// # Safety
/// `message` must reference `message_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_turboshake128(
    length: usize,
    message: *const u8,
    message_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(message) = bytes(message, message_len) else {
            return ptr::null();
        };
        match shake::turboshake128(length, message) {
            Some(output) => publish(&output),
            None => ptr::null(),
        }
    })
}

/// C ABI: TurboSHAKE256 over `message`, returning `length` output bytes.
///
/// # Safety
/// `message` must reference `message_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_turboshake256(
    length: usize,
    message: *const u8,
    message_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(message) = bytes(message, message_len) else {
            return ptr::null();
        };
        match shake::turboshake256(length, message) {
            Some(output) => publish(&output),
            None => ptr::null(),
        }
    })
}

/// C ABI: starts a TurboSHAKE128 state; `domain = -1` selects the standard domain.
#[no_mangle]
pub extern "C" fn elephc_sodium_turboshake128_init(domain: i64) -> i64 {
    guarded(-1, || shake::init(ShakeKind::TurboShake128, domain))
}

/// C ABI: starts a TurboSHAKE256 state; `domain = -1` selects the standard domain.
#[no_mangle]
pub extern "C" fn elephc_sodium_turboshake256_init(domain: i64) -> i64 {
    guarded(-1, || shake::init(ShakeKind::TurboShake256, domain))
}

/// C ABI: absorbs bytes into a TurboSHAKE128 state; returns 0 or -1.
///
/// # Safety
/// `message` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_turboshake128_update(
    handle: i64,
    message: *const u8,
    len: usize,
) -> i64 {
    guarded(-1, || {
        let Some(message) = bytes(message, len) else {
            return -1;
        };
        shake::update(handle, message)
    })
}

/// C ABI: absorbs bytes into a TurboSHAKE256 state; returns 0 or -1.
///
/// # Safety
/// `message` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_turboshake256_update(
    handle: i64,
    message: *const u8,
    len: usize,
) -> i64 {
    guarded(-1, || {
        let Some(message) = bytes(message, len) else {
            return -1;
        };
        shake::update(handle, message)
    })
}

/// C ABI: squeezes bytes from a TurboSHAKE128 state, or null on failure.
#[no_mangle]
pub extern "C" fn elephc_sodium_turboshake128_squeeze(handle: i64, length: usize) -> *const u8 {
    guarded(ptr::null(), || {
        shake::squeeze(handle, length)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: squeezes bytes from a TurboSHAKE256 state, or null on failure.
#[no_mangle]
pub extern "C" fn elephc_sodium_turboshake256_squeeze(handle: i64, length: usize) -> *const u8 {
    guarded(ptr::null(), || {
        shake::squeeze(handle, length)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: drops a TurboSHAKE128 state.
#[no_mangle]
pub extern "C" fn elephc_sodium_turboshake128_drop(handle: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| shake::drop_state(handle)));
}

/// C ABI: drops a TurboSHAKE256 state.
#[no_mangle]
pub extern "C" fn elephc_sodium_turboshake256_drop(handle: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| shake::drop_state(handle)));
}

/// C ABI: formats a 16-byte IPv6 address in canonical textual form.
///
/// # Safety
/// `binary` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_bin2ip(binary: *const u8, len: usize) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(binary) = bytes(binary, len) else {
            return ptr::null();
        };
        let Ok(address) = <[u8; 16]>::try_from(binary) else {
            return ptr::null();
        };
        // libsodium renders the IPv4-mapped prefix as dotted IPv4, not `::ffff:a.b.c.d`.
        if address[..12] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff] {
            return publish(
                format!("{}.{}.{}.{}", address[12], address[13], address[14], address[15])
                    .as_bytes(),
            );
        }
        publish(std::net::Ipv6Addr::from(address).to_string().as_bytes())
    })
}

/// C ABI: parses an IPv4 or IPv6 address into its 16-byte IPv4-mapped IPv6 form.
///
/// # Safety
/// `text` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ip2bin(text: *const u8, len: usize) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(text) = bytes(text, len) else {
            return ptr::null();
        };
        let Ok(text) = std::str::from_utf8(text) else {
            return ptr::null();
        };
        // libsodium accepts an IPv6 zone suffix and drops it from the binary form.
        let (address, zone) = match text.split_once('%') {
            Some((address, zone)) => (address, Some(zone)),
            None => (text, None),
        };
        if let Some(zone) = zone {
            if address.contains(':') == false
                || zone.is_empty()
                || !zone
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            {
                return ptr::null();
            }
        }
        let address = if let Ok(ipv4) = address.parse::<std::net::Ipv4Addr>() {
            ipv4.to_ipv6_mapped().octets()
        } else if let Ok(ipv6) = address.parse::<std::net::Ipv6Addr>() {
            ipv6.octets()
        } else {
            return ptr::null();
        };
        publish(&address)
    })
}

/// C ABI: IPcrypt plain AES-128 encryption.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ipcrypt_encrypt(
    input: *const u8,
    input_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(input), Some(key)) = (bytes(input, input_len), bytes(key, key_len)) else {
            return ptr::null();
        };
        ipcrypt::encrypt(input, key)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: IPcrypt plain AES-128 decryption.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ipcrypt_decrypt(
    input: *const u8,
    input_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(input), Some(key)) = (bytes(input, input_len), bytes(key, key_len)) else {
            return ptr::null();
        };
        ipcrypt::decrypt(input, key)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: IPcrypt-ND encryption, publishing `tweak || ciphertext`.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ipcrypt_nd_encrypt(
    input: *const u8,
    input_len: usize,
    tweak: *const u8,
    tweak_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(input), Some(tweak), Some(key)) = (
            bytes(input, input_len),
            bytes(tweak, tweak_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        ipcrypt::nd_encrypt(input, tweak, key)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: IPcrypt-ND decryption.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ipcrypt_nd_decrypt(
    input: *const u8,
    input_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(input), Some(key)) = (bytes(input, input_len), bytes(key, key_len)) else {
            return ptr::null();
        };
        ipcrypt::nd_decrypt(input, key)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: IPcrypt-NDX encryption, publishing `tweak || ciphertext`.
///
/// # Safety
/// All pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ipcrypt_ndx_encrypt(
    input: *const u8,
    input_len: usize,
    tweak: *const u8,
    tweak_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(input), Some(tweak), Some(key)) = (
            bytes(input, input_len),
            bytes(tweak, tweak_len),
            bytes(key, key_len),
        ) else {
            return ptr::null();
        };
        ipcrypt::ndx_encrypt(input, tweak, key)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: IPcrypt-NDX decryption.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ipcrypt_ndx_decrypt(
    input: *const u8,
    input_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(input), Some(key)) = (bytes(input, input_len), bytes(key, key_len)) else {
            return ptr::null();
        };
        ipcrypt::ndx_decrypt(input, key)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: IPcrypt-PFX format-preserving encryption.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ipcrypt_pfx_encrypt(
    input: *const u8,
    input_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(input), Some(key)) = (bytes(input, input_len), bytes(key, key_len)) else {
            return ptr::null();
        };
        ipcrypt::pfx_encrypt(input, key)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: IPcrypt-PFX format-preserving decryption.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ipcrypt_pfx_decrypt(
    input: *const u8,
    input_len: usize,
    key: *const u8,
    key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(input), Some(key)) = (bytes(input, input_len), bytes(key, key_len)) else {
            return ptr::null();
        };
        ipcrypt::pfx_decrypt(input, key)
            .map(|output| publish(&output))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: generates an X-Wing keypair (`secret || public`).
#[no_mangle]
pub extern "C" fn elephc_sodium_kem_keypair() -> *const u8 {
    guarded(ptr::null(), || publish(&kem::xwing_keypair_random()))
}

/// C ABI: derives an X-Wing keypair from a 32-byte seed.
///
/// # Safety
/// `seed` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_seed_keypair(
    seed: *const u8,
    len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(seed) = bytes(seed, len) else {
            return ptr::null();
        };
        kem::xwing_seed_keypair(seed)
            .map(|keypair| publish(&keypair))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: extracts the X-Wing secret key from a keypair.
///
/// # Safety
/// `keypair` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_secretkey(
    keypair: *const u8,
    len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(keypair) = bytes(keypair, len) else {
            return ptr::null();
        };
        kem::xwing_secretkey(keypair)
            .map(|key| publish(&key))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: extracts the X-Wing public key from a keypair.
///
/// # Safety
/// `keypair` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_publickey(
    keypair: *const u8,
    len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(keypair) = bytes(keypair, len) else {
            return ptr::null();
        };
        kem::xwing_publickey(keypair)
            .map(|key| publish(&key))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: X-Wing encapsulation, publishing `ciphertext || shared_secret`.
///
/// # Safety
/// `public_key` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_enc(
    public_key: *const u8,
    len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(public_key) = bytes(public_key, len) else {
            return ptr::null();
        };
        kem::xwing_enc(public_key)
            .map(|pair| publish(&pair))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: X-Wing decapsulation.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_dec(
    ciphertext: *const u8,
    ciphertext_len: usize,
    secret_key: *const u8,
    secret_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(ciphertext), Some(secret_key)) = (
            bytes(ciphertext, ciphertext_len),
            bytes(secret_key, secret_key_len),
        ) else {
            return ptr::null();
        };
        kem::xwing_dec(ciphertext, secret_key)
            .map(|shared| publish(&shared))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: generates an ML-KEM-768 keypair (`secret || public`).
#[no_mangle]
pub extern "C" fn elephc_sodium_kem_mlkem768_keypair() -> *const u8 {
    guarded(ptr::null(), || publish(&kem::mlkem_keypair_random()))
}

/// C ABI: derives an ML-KEM-768 keypair from a 64-byte seed.
///
/// # Safety
/// `seed` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_mlkem768_seed_keypair(
    seed: *const u8,
    len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(seed) = bytes(seed, len) else {
            return ptr::null();
        };
        kem::mlkem_seed_keypair(seed)
            .map(|keypair| publish(&keypair))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: extracts the ML-KEM-768 expanded secret key from a keypair.
///
/// # Safety
/// `keypair` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_mlkem768_secretkey(
    keypair: *const u8,
    len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(keypair) = bytes(keypair, len) else {
            return ptr::null();
        };
        kem::mlkem_secretkey(keypair)
            .map(|key| publish(&key))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: extracts the ML-KEM-768 public key from a keypair.
///
/// # Safety
/// `keypair` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_mlkem768_publickey(
    keypair: *const u8,
    len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(keypair) = bytes(keypair, len) else {
            return ptr::null();
        };
        kem::mlkem_publickey(keypair)
            .map(|key| publish(&key))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: ML-KEM-768 encapsulation, publishing `ciphertext || shared_secret`.
///
/// # Safety
/// `public_key` must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_mlkem768_enc(
    public_key: *const u8,
    len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(public_key) = bytes(public_key, len) else {
            return ptr::null();
        };
        kem::mlkem_enc(public_key)
            .map(|pair| publish(&pair))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: ML-KEM-768 decapsulation.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_kem_mlkem768_dec(
    ciphertext: *const u8,
    ciphertext_len: usize,
    secret_key: *const u8,
    secret_key_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(ciphertext), Some(secret_key)) = (
            bytes(ciphertext, ciphertext_len),
            bytes(secret_key, secret_key_len),
        ) else {
            return ptr::null();
        };
        kem::mlkem_dec(ciphertext, secret_key)
            .map(|shared| publish(&shared))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: Argon2i/Argon2id password hashing with libsodium's parameter mapping.
///
/// # Safety
/// Both string pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_pwhash(
    output_len: usize,
    password: *const u8,
    password_len: usize,
    salt: *const u8,
    salt_len: usize,
    opslimit: i64,
    memlimit: i64,
    algorithm: i64,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(password), Some(salt)) = (bytes(password, password_len), bytes(salt, salt_len))
        else {
            return ptr::null();
        };
        let (Ok(opslimit), Ok(memlimit)) = (u64::try_from(opslimit), u64::try_from(memlimit)) else {
            return ptr::null();
        };
        pwhash::hash(output_len, password, salt, opslimit, memlimit, algorithm)
            .map(|hash| publish(&hash))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: builds a PHC Argon2id password hash string.
///
/// # Safety
/// Both string pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_pwhash_str(
    password: *const u8,
    password_len: usize,
    salt: *const u8,
    salt_len: usize,
    opslimit: i64,
    memlimit: i64,
    algorithm: i64,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(password), Some(salt)) = (bytes(password, password_len), bytes(salt, salt_len))
        else {
            return ptr::null();
        };
        let (Ok(opslimit), Ok(memlimit)) = (u64::try_from(opslimit), u64::try_from(memlimit)) else {
            return ptr::null();
        };
        pwhash::hash_str(password, salt, opslimit, memlimit, algorithm)
            .map(|hash| publish(hash.as_bytes()))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: verifies a PHC Argon2 password hash, returning 1 or 0.
///
/// # Safety
/// Both string pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_pwhash_str_verify(
    password: *const u8,
    password_len: usize,
    hash: *const u8,
    hash_len: usize,
) -> i64 {
    guarded(0, || {
        let (Some(password), Some(hash)) = (bytes(password, password_len), bytes(hash, hash_len))
        else {
            return 0;
        };
        let Ok(hash) = std::str::from_utf8(hash) else {
            return 0;
        };
        if pwhash::verify_str(password, hash) { 1 } else { 0 }
    })
}

/// C ABI: scrypt-salsa20/poly1305 derivation with libsodium's parameter chooser.
///
/// # Safety
/// Both string pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_pwhash_scryptsalsa208sha256(
    output_len: usize,
    password: *const u8,
    password_len: usize,
    salt: *const u8,
    salt_len: usize,
    opslimit: i64,
    memlimit: i64,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(password), Some(salt)) = (bytes(password, password_len), bytes(salt, salt_len))
        else {
            return ptr::null();
        };
        let (Ok(opslimit), Ok(memlimit)) = (u64::try_from(opslimit), u64::try_from(memlimit)) else {
            return ptr::null();
        };
        pwhash::scrypt_hash(output_len, password, salt, opslimit, memlimit)
            .map(|hash| publish(&hash))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: builds a `$7$` scrypt string.
///
/// # Safety
/// Both string pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_pwhash_scryptsalsa208sha256_str(
    password: *const u8,
    password_len: usize,
    salt: *const u8,
    salt_len: usize,
    opslimit: i64,
    memlimit: i64,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(password), Some(salt)) = (bytes(password, password_len), bytes(salt, salt_len))
        else {
            return ptr::null();
        };
        let (Ok(opslimit), Ok(memlimit)) = (u64::try_from(opslimit), u64::try_from(memlimit)) else {
            return ptr::null();
        };
        pwhash::scrypt_hash_str(password, salt, opslimit, memlimit)
            .map(|hash| publish(hash.as_bytes()))
            .unwrap_or(ptr::null())
    })
}

/// C ABI: verifies a `$7$` scrypt string, returning 1 or 0.
///
/// # Safety
/// Both string pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_pwhash_scryptsalsa208sha256_str_verify(
    password: *const u8,
    password_len: usize,
    hash: *const u8,
    hash_len: usize,
) -> i64 {
    guarded(0, || {
        let (Some(password), Some(hash)) = (bytes(password, password_len), bytes(hash, hash_len))
        else {
            return 0;
        };
        let Ok(hash) = std::str::from_utf8(hash) else {
            return 0;
        };
        if pwhash::scrypt_verify_str(password, hash) { 1 } else { 0 }
    })
}

/// C ABI: returns the byte length of the last published output, excluding NUL.
#[no_mangle]
pub extern "C" fn elephc_sodium_last_len() -> usize {
    OUTPUT.with(|slot| slot.borrow().len().saturating_sub(1))
}

/// C ABI: reports the bridge version.
#[no_mangle]
pub extern "C" fn elephc_sodium_version() -> i32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NaCl's `tests/secretbox` XSalsa20-Poly1305 compatibility vector.
    const KEY: &str = "1b27556473e985d462cd51197a9a46c76009549eac6474f206c4ee0844f68389";
    const NONCE: &str = "69696ee955b62b73cd62bda875fc73d68219e0036b7a0b37";
    const PLAINTEXT: &str = concat!(
        "be075fc53c81f2d5cf141316ebeb0c7b5228c52a4c62cbd44b66849b64244ffce5ecbaaf33bd751a",
        "1ac728d45e6c61296cdc3c01233561f41db66cce314adb310e3be8250c46f06dceea3a7fa1348057",
        "e2f6556ad6b1318a024a838f21af1fde048977eb48f59ffd4924ca1c60902e52f0a089bc76897040",
        "e082f937763848645e0705"
    );
    const CIPHERTEXT: &str = concat!(
        "f3ffc7703f9400e52a7dfb4b3d3305d98e993b9f48681273c29650ba32fc76ce48332ea7164d96a4",
        "476fb8c531a1186ac0dfc17c98dce87b4da7f011ec48c97271d2c20f9b928fe2270d6fb863d51738",
        "b48eeee314a7cc8ab932164548e526ae90224368517acfeabd6bb3732bc0e9da99832b61ca01b6de",
        "56244a9e88d5f9b37973f622a43d14a6599b1f654cb45a74e355a5"
    );

    /// Decodes a lowercase hex string into bytes.
    fn hex(value: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(value.len() / 2);
        let bytes = value.as_bytes();
        for pair in bytes.chunks_exact(2) {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => panic!("invalid hex"),
            };
            out.push(digit(pair[0]) << 4 | digit(pair[1]));
        }
        out
    }

    /// Ciphertext bytes match NaCl/libsodium for the committed vector.
    #[test]
    fn secretbox_matches_nacl_vector() {
        let key = hex(KEY);
        let nonce = hex(NONCE);
        let plaintext = hex(PLAINTEXT);
        let expected = hex(CIPHERTEXT);
        let pointer = unsafe {
            elephc_sodium_secretbox(
                key.as_ptr(),
                key.len(),
                nonce.as_ptr(),
                nonce.len(),
                plaintext.as_ptr(),
                plaintext.len(),
            )
        };
        assert!(!pointer.is_null());
        let output = unsafe { std::slice::from_raw_parts(pointer, elephc_sodium_last_len()) };
        assert_eq!(output, expected.as_slice());
    }

    /// The same vector decrypts, and tampering rejects the authentication tag.
    #[test]
    fn secretbox_open_round_trip_and_tamper() {
        let key = hex(KEY);
        let nonce = hex(NONCE);
        let plaintext = hex(PLAINTEXT);
        let ciphertext = hex(CIPHERTEXT);
        let pointer = unsafe {
            elephc_sodium_secretbox_open(
                key.as_ptr(),
                key.len(),
                nonce.as_ptr(),
                nonce.len(),
                ciphertext.as_ptr(),
                ciphertext.len(),
            )
        };
        assert!(!pointer.is_null());
        let output = unsafe { std::slice::from_raw_parts(pointer, elephc_sodium_last_len()) };
        assert_eq!(output, plaintext.as_slice());

        let mut tampered = ciphertext;
        tampered[0] ^= 0xaa;
        let pointer = unsafe {
            elephc_sodium_secretbox_open(
                key.as_ptr(),
                key.len(),
                nonce.as_ptr(),
                nonce.len(),
                tampered.as_ptr(),
                tampered.len(),
            )
        };
        assert!(pointer.is_null());
    }

    /// Empty plaintext produces a tag-only ciphertext and decrypts to empty.
    #[test]
    fn secretbox_handles_empty_plaintext() {
        let key = vec![7u8; KEY_BYTES];
        let nonce = vec![9u8; NONCE_BYTES];
        let pointer = unsafe {
            elephc_sodium_secretbox(
                key.as_ptr(),
                key.len(),
                nonce.as_ptr(),
                nonce.len(),
                ptr::null(),
                0,
            )
        };
        assert!(!pointer.is_null());
        assert_eq!(elephc_sodium_last_len(), MAC_BYTES);
        let ciphertext = unsafe {
            std::slice::from_raw_parts(pointer, elephc_sodium_last_len()).to_vec()
        };
        let pointer = unsafe {
            elephc_sodium_secretbox_open(
                key.as_ptr(),
                key.len(),
                nonce.as_ptr(),
                nonce.len(),
                ciphertext.as_ptr(),
                ciphertext.len(),
            )
        };
        assert!(!pointer.is_null());
        assert_eq!(elephc_sodium_last_len(), 0);
    }

    /// SHAKE128/SHAKE256 match FIPS 202's empty-message vectors.
    #[test]
    fn shake_matches_fips202_vectors() {
        let expected128 = hex("7f9c2ba4e88f827d616045507605853ed73b8093f6efbc88eb1a6eacfa66ef26");
        let pointer = unsafe { elephc_sodium_shake128(32, ptr::null(), 0) };
        assert!(!pointer.is_null());
        let output = unsafe { std::slice::from_raw_parts(pointer, elephc_sodium_last_len()) };
        assert_eq!(output, expected128.as_slice());

        let expected256 = hex(concat!(
            "46b9dd2b0ba88d13233b3feb743eeb243fcd52ea62b81b82b50c27646ed5762f",
            "d75dc4ddd8c0f200cb05019d67b592f6fc821c49479ab48640292eacb3b7c4be"
        ));
        let pointer = unsafe { elephc_sodium_shake256(64, ptr::null(), 0) };
        assert!(!pointer.is_null());
        let output = unsafe { std::slice::from_raw_parts(pointer, elephc_sodium_last_len()) };
        assert_eq!(output, expected256.as_slice());
    }

    /// Incremental SHAKE states match the one-shot output and stream squeezes.
    #[test]
    fn shake_state_matches_one_shot_and_streams() {
        let message = b"the quick brown fox";
        let expected = shake::shake128(48, message).expect("one-shot");
        let handle = elephc_sodium_shake128_init(-1);
        assert!(handle > 0);
        assert_eq!(
            unsafe { elephc_sodium_shake128_update(handle, message[..5].as_ptr(), 5) },
            0
        );
        assert_eq!(
            unsafe {
                elephc_sodium_shake128_update(
                    handle,
                    message[5..].as_ptr(),
                    message.len() - 5,
                )
            },
            0
        );
        let first = elephc_sodium_shake128_squeeze(handle, 20);
        assert!(!first.is_null());
        let first = unsafe {
            std::slice::from_raw_parts(first, elephc_sodium_last_len()).to_vec()
        };
        let second = elephc_sodium_shake128_squeeze(handle, 28);
        assert!(!second.is_null());
        let second = unsafe {
            std::slice::from_raw_parts(second, elephc_sodium_last_len()).to_vec()
        };
        assert_eq!([first, second].concat(), expected);
        assert_eq!(
            unsafe { elephc_sodium_shake128_update(handle, message.as_ptr(), message.len()) },
            -1
        );
        elephc_sodium_shake128_drop(handle);
    }

    /// Invalid SHAKE domains are rejected before a state is created.
    #[test]
    fn shake_state_rejects_invalid_domain() {
        assert_eq!(elephc_sodium_shake128_init(0), -1);
        assert_eq!(elephc_sodium_shake256_init(0x80), -1);
        let handle = elephc_sodium_shake256_init(0x7f);
        assert!(handle > 0);
        elephc_sodium_shake256_drop(handle);
    }

    /// TurboSHAKE128/256 match libsodium's 12-round vectors.
    #[test]
    fn turboshake_matches_libsodium_vectors() {
        let expected128 = hex("1e415f1c5983aff2169217277d17bb538cd945a397ddec541f1ce41af2c1b74c");
        let pointer = unsafe { elephc_sodium_turboshake128(32, ptr::null(), 0) };
        assert!(!pointer.is_null());
        let output = unsafe { std::slice::from_raw_parts(pointer, elephc_sodium_last_len()) };
        assert_eq!(output, expected128.as_slice());

        let expected256 = hex("367a329dafea871c7802ec67f905ae13c57695dc2c6663c61035f59a18f8e7db");
        let pointer = unsafe { elephc_sodium_turboshake256(32, ptr::null(), 0) };
        assert!(!pointer.is_null());
        let output = unsafe { std::slice::from_raw_parts(pointer, elephc_sodium_last_len()) };
        assert_eq!(output, expected256.as_slice());
    }

    /// IP conversion round-trips canonical IPv6 and IPv4-mapped forms.
    #[test]
    fn ip_conversion_round_trips() {
        let text = b"2001:db8::1";
        let pointer = unsafe { elephc_sodium_ip2bin(text.as_ptr(), text.len()) };
        assert!(!pointer.is_null());
        assert_eq!(elephc_sodium_last_len(), 16);
        let binary = unsafe {
            std::slice::from_raw_parts(pointer, elephc_sodium_last_len()).to_vec()
        };
        let pointer = unsafe { elephc_sodium_bin2ip(binary.as_ptr(), binary.len()) };
        assert!(!pointer.is_null());
        let rendered = unsafe {
            std::slice::from_raw_parts(pointer, elephc_sodium_last_len())
        };
        assert_eq!(rendered, text);

        let text = b"192.0.2.1";
        let pointer = unsafe { elephc_sodium_ip2bin(text.as_ptr(), text.len()) };
        assert!(!pointer.is_null());
        let binary = unsafe {
            std::slice::from_raw_parts(pointer, elephc_sodium_last_len()).to_vec()
        };
        assert_eq!(binary.len(), 16);
        assert_eq!(&binary[..12], &[0u8; 10].iter().chain(&[0xff, 0xff]).copied().collect::<Vec<_>>()[..]);
        let pointer = unsafe { elephc_sodium_bin2ip(binary.as_ptr(), binary.len()) };
        assert!(!pointer.is_null());
        let rendered = unsafe {
            std::slice::from_raw_parts(pointer, elephc_sodium_last_len())
        };
        assert_eq!(rendered, b"192.0.2.1");
    }

    /// Wrong key/nonce lengths and short ciphertexts fail closed.
    #[test]
    fn secretbox_rejects_invalid_lengths() {
        let nonce = vec![0u8; NONCE_BYTES];
        let key = vec![0u8; KEY_BYTES];
        let short_key = vec![0u8; KEY_BYTES - 1];
        assert!(unsafe {
            elephc_sodium_secretbox(
                short_key.as_ptr(),
                short_key.len(),
                nonce.as_ptr(),
                nonce.len(),
                ptr::null(),
                0,
            )
        }
        .is_null());
        assert!(unsafe {
            elephc_sodium_secretbox(
                key.as_ptr(),
                key.len(),
                nonce.as_ptr(),
                NONCE_BYTES - 1,
                ptr::null(),
                0,
            )
        }
        .is_null());
        let short = vec![0u8; MAC_BYTES - 1];
        assert!(unsafe {
            elephc_sodium_secretbox_open(
                key.as_ptr(),
                key.len(),
                nonce.as_ptr(),
                nonce.len(),
                short.as_ptr(),
                short.len(),
            )
        }
        .is_null());
    }
}
