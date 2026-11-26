//! Purpose:
//! libssl-backed `Openssl\Session` import/export and metadata bridge for the OpenSSL
//! staticlib.
//!
//! Called from:
//! - The `Openssl\Session` prelude through the `elephc_openssl_session_*` C ABI.
//! - `cargo test -p elephc-openssl` when a managed OpenSSL `lib/` directory provides
//!   both `libssl.a` and `libcrypto.a`.
//!
//! Key details:
//! - Sessions cross the PHP boundary as canonical DER hex; DER and raw session ids can
//!   contain NUL, so every binary hand-off is hex-encoded and decoded here.
//! - `import` delegates validation and cipher lookup to libssl's `d2i`/`PEM_read`, so an
//!   unknown cipher or malformed ASN.1 fails exactly where php-src's import fails.
//! - Every session is parsed and freed inside one ABI call; only thread-local C strings
//!   outlive a call, matching the bridge's FFI buffer-hygiene contract.

#![allow(non_camel_case_types)]

use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

/// Opaque `SSL_SESSION` value owned by libssl.
#[repr(C)]
pub(crate) struct SSL_SESSION {
    _private: [u8; 0],
}

/// Opaque `SSL_CIPHER` catalog entry owned by libssl.
#[repr(C)]
pub(crate) struct SSL_CIPHER {
    _private: [u8; 0],
}

/// Opaque memory BIO.
#[repr(C)]
pub(crate) struct BIO {
    _private: [u8; 0],
}

/// Opaque BIO method table returned by `BIO_s_mem`.
#[repr(C)]
pub(crate) struct BIO_METHOD {
    _private: [u8; 0],
}

/// `BIO_CTRL_INFO`, which returns the memory BIO's byte length and data pointer.
const BIO_CTRL_INFO: c_int = 3;
/// `OPENSSL_ENCODING_DER`.
const OPENSSL_ENCODING_DER: i64 = 0;
/// `OPENSSL_ENCODING_PEM`.
const OPENSSL_ENCODING_PEM: i64 = 2;
/// `TLS1_VERSION`.
const TLS1_VERSION: c_int = 0x0301;
/// `TLS1_1_VERSION`.
const TLS1_1_VERSION: c_int = 0x0302;
/// `TLS1_2_VERSION`.
const TLS1_2_VERSION: c_int = 0x0303;
/// `TLS1_3_VERSION`.
const TLS1_3_VERSION: c_int = 0x0304;
/// Bridge field selector for `SSL_SESSION_get_timeout`.
const FIELD_TIMEOUT: i64 = 0;
/// Bridge field selector for `SSL_SESSION_get_time_ex`.
const FIELD_CREATED_AT: i64 = 1;
/// Bridge field selector for `SSL_SESSION_is_resumable`.
const FIELD_RESUMABLE: i64 = 2;
/// Bridge field selector for `SSL_SESSION_has_ticket`.
const FIELD_HAS_TICKET: i64 = 3;
/// Bridge field selector for `SSL_SESSION_get_ticket_lifetime_hint`.
const FIELD_TICKET_LIFETIME_HINT: i64 = 4;
/// Error sentinel for the integer metadata entry point.
const INVALID_INT: i64 = i64::MIN;
/// NUL pointer used when an entry point cannot answer.
static EMPTY_C_STRING: [u8; 1] = [0];

extern "C" {
    fn BIO_new_mem_buf(buf: *const c_void, len: c_int) -> *mut BIO;
    fn BIO_new(method: *const BIO_METHOD) -> *mut BIO;
    fn BIO_s_mem() -> *const BIO_METHOD;
    fn BIO_free(bio: *mut BIO) -> c_int;
    fn BIO_ctrl(bio: *mut BIO, cmd: c_int, larg: i64, parg: *mut c_void) -> i64;
    fn PEM_read_bio_SSL_SESSION(
        bio: *mut BIO,
        session: *mut *mut SSL_SESSION,
        callback: Option<unsafe extern "C" fn(*mut c_char, c_int, c_int, *mut c_void) -> c_int>,
        userdata: *mut c_void,
    ) -> *mut SSL_SESSION;
    fn PEM_write_bio_SSL_SESSION(bio: *mut BIO, session: *const SSL_SESSION) -> c_int;
    fn d2i_SSL_SESSION(
        session: *mut *mut SSL_SESSION,
        bytes: *mut *const u8,
        length: i64,
    ) -> *mut SSL_SESSION;
    fn i2d_SSL_SESSION(session: *const SSL_SESSION, bytes: *mut *mut u8) -> c_int;
    fn SSL_SESSION_get_id(session: *const SSL_SESSION, length: *mut u32) -> *const u8;
    fn SSL_SESSION_get_time_ex(session: *const SSL_SESSION) -> i64;
    fn SSL_SESSION_get_timeout(session: *const SSL_SESSION) -> i64;
    fn SSL_SESSION_get_protocol_version(session: *const SSL_SESSION) -> c_int;
    fn SSL_SESSION_get0_cipher(session: *const SSL_SESSION) -> *const SSL_CIPHER;
    fn SSL_CIPHER_get_name(cipher: *const SSL_CIPHER) -> *const c_char;
    fn SSL_SESSION_has_ticket(session: *const SSL_SESSION) -> c_int;
    fn SSL_SESSION_get_ticket_lifetime_hint(session: *const SSL_SESSION) -> u64;
    fn SSL_SESSION_is_resumable(session: *const SSL_SESSION) -> c_int;
    fn SSL_SESSION_free(session: *mut SSL_SESSION);
}

thread_local! {
    static SESSION_TEXT: RefCell<CString> = RefCell::new(CString::default());
}

/// Runs `body` under `catch_unwind`, answering `fallback` when it panics.
fn guarded<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(fallback)
}

/// Stores ASCII `bytes` in this thread's session stash and returns a C pointer to it.
fn stash(bytes: Vec<u8>) -> *const c_char {
    let owned = CString::new(bytes).unwrap_or_default();
    SESSION_TEXT
        .try_with(|slot| {
            *slot.borrow_mut() = owned;
            slot.borrow().as_ptr()
        })
        .unwrap_or(EMPTY_C_STRING.as_ptr() as *const c_char)
}

/// Returns a pointer to the empty C string used by every failure path.
fn empty_string() -> *const c_char {
    EMPTY_C_STRING.as_ptr() as *const c_char
}

/// Reads one C pointer/length pair as a byte slice.
///
/// # Safety
/// `pointer` must reference `length` readable bytes when `length` is positive.
unsafe fn input_bytes<'a>(pointer: *const c_char, length: i64) -> Option<&'a [u8]> {
    if length < 0 {
        return None;
    }
    if length == 0 {
        return Some(&[]);
    }
    if pointer.is_null() {
        return None;
    }
    Some(slice::from_raw_parts(pointer as *const u8, length as usize))
}

/// Encodes bytes as lowercase hexadecimal ASCII.
fn hex_encode(bytes: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize]);
        output.push(DIGITS[(byte & 0x0f) as usize]);
    }
    output
}

/// Decodes lowercase or uppercase hexadecimal ASCII into bytes.
fn hex_decode(text: &[u8]) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let nibble = |byte: u8| -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    };
    let mut output = Vec::with_capacity(text.len() / 2);
    for pair in text.chunks_exact(2) {
        output.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    Some(output)
}

/// Parses one DER `SSL_SESSION` blob.
///
/// # Safety
/// Calls libssl's `d2i_SSL_SESSION`; the returned session is owned by the caller.
unsafe fn parse_der(bytes: &[u8]) -> Option<*mut SSL_SESSION> {
    if bytes.is_empty() || bytes.len() > i64::MAX as usize {
        return None;
    }
    let mut cursor = bytes.as_ptr();
    let session = d2i_SSL_SESSION(ptr::null_mut(), &mut cursor, bytes.len() as i64);
    if session.is_null() {
        None
    } else {
        Some(session)
    }
}

/// Parses one PEM `SSL SESSION PARAMETERS` blob.
///
/// # Safety
/// Calls libssl's `PEM_read_bio_SSL_SESSION`; the returned session is owned by the caller.
unsafe fn parse_pem(bytes: &[u8]) -> Option<*mut SSL_SESSION> {
    if bytes.is_empty() || bytes.len() > c_int::MAX as usize {
        return None;
    }
    let bio = BIO_new_mem_buf(bytes.as_ptr() as *const c_void, bytes.len() as c_int);
    if bio.is_null() {
        return None;
    }
    let session = PEM_read_bio_SSL_SESSION(bio, ptr::null_mut(), None, ptr::null_mut());
    BIO_free(bio);
    if session.is_null() {
        None
    } else {
        Some(session)
    }
}

/// Parses a session in the PHP `OPENSSL_ENCODING_DER`/`OPENSSL_ENCODING_PEM` format.
///
/// # Safety
/// The returned session is owned by the caller and must be freed with `SSL_SESSION_free`.
unsafe fn parse_session(bytes: &[u8], format: i64) -> Option<*mut SSL_SESSION> {
    match format {
        OPENSSL_ENCODING_DER => parse_der(bytes),
        OPENSSL_ENCODING_PEM => parse_pem(bytes),
        _ => None,
    }
}

/// Encodes one session as canonical DER.
///
/// # Safety
/// `session` must be a valid pointer returned by libssl.
unsafe fn encode_der(session: *const SSL_SESSION) -> Option<Vec<u8>> {
    let length = i2d_SSL_SESSION(session, ptr::null_mut());
    if length <= 0 {
        return None;
    }
    let mut buffer = vec![0u8; length as usize];
    let mut cursor = buffer.as_mut_ptr();
    let written = i2d_SSL_SESSION(session, &mut cursor);
    if written != length {
        return None;
    }
    Some(buffer)
}

/// Encodes one session as PEM.
///
/// # Safety
/// `session` must be a valid pointer returned by libssl.
unsafe fn encode_pem(session: *const SSL_SESSION) -> Option<Vec<u8>> {
    let bio = BIO_new(BIO_s_mem());
    if bio.is_null() {
        return None;
    }
    if PEM_write_bio_SSL_SESSION(bio, session) != 1 {
        BIO_free(bio);
        return None;
    }
    let mut data: *mut c_char = ptr::null_mut();
    let length = BIO_ctrl(
        bio,
        BIO_CTRL_INFO,
        0,
        &mut data as *mut *mut c_char as *mut c_void,
    );
    let output = if length > 0 && !data.is_null() {
        Some(slice::from_raw_parts(data as *const u8, length as usize).to_vec())
    } else {
        None
    };
    BIO_free(bio);
    output
}

/// Returns the session id as lowercase hexadecimal ASCII.
///
/// # Safety
/// `session` must be a valid pointer returned by libssl.
unsafe fn id_hex(session: *const SSL_SESSION) -> Vec<u8> {
    let mut length: u32 = 0;
    let id = SSL_SESSION_get_id(session, &mut length);
    if id.is_null() || length == 0 {
        return Vec::new();
    }
    hex_encode(slice::from_raw_parts(id, length as usize))
}

/// Returns PHP's protocol label for a session, or empty for an unknown/DTLS version.
///
/// # Safety
/// `session` must be a valid pointer returned by libssl.
unsafe fn protocol_name(session: *const SSL_SESSION) -> &'static [u8] {
    match SSL_SESSION_get_protocol_version(session) {
        TLS1_3_VERSION => b"TLSv1.3",
        TLS1_2_VERSION => b"TLSv1.2",
        TLS1_1_VERSION => b"TLSv1.1",
        TLS1_VERSION => b"TLSv1.0",
        _ => b"",
    }
}

/// Returns the session cipher name, or empty when libssl has no cipher.
///
/// # Safety
/// `session` must be a valid pointer returned by libssl.
unsafe fn cipher_name(session: *const SSL_SESSION) -> Vec<u8> {
    let cipher = SSL_SESSION_get0_cipher(session);
    if cipher.is_null() {
        return Vec::new();
    }
    let name = SSL_CIPHER_get_name(cipher);
    if name.is_null() {
        return Vec::new();
    }
    CStr::from_ptr(name).to_bytes().to_vec()
}

/// Reads one integer metadata field, or `None` for an unknown selector.
///
/// # Safety
/// `session` must be a valid pointer returned by libssl.
unsafe fn session_field(session: *const SSL_SESSION, field: i64) -> Option<i64> {
    match field {
        FIELD_TIMEOUT => Some(SSL_SESSION_get_timeout(session)),
        FIELD_CREATED_AT => Some(SSL_SESSION_get_time_ex(session)),
        FIELD_RESUMABLE => Some(i64::from(SSL_SESSION_is_resumable(session) != 0)),
        FIELD_HAS_TICKET => Some(i64::from(SSL_SESSION_has_ticket(session) != 0)),
        FIELD_TICKET_LIFETIME_HINT => {
            Some(SSL_SESSION_get_ticket_lifetime_hint(session) as i64)
        }
        _ => None,
    }
}

/// Parses one DER hex payload, or returns `None` when the hex or session is malformed.
///
/// # Safety
/// The returned session is owned by the caller and must be freed with `SSL_SESSION_free`.
unsafe fn session_from_der_hex(text: &[u8]) -> Option<*mut SSL_SESSION> {
    let der = hex_decode(text)?;
    parse_der(&der)
}

/// C ABI: imports a DER or PEM session and returns its canonical DER as hex, or "".
#[no_mangle]
pub unsafe extern "C" fn elephc_openssl_session_import(
    data: *const c_char,
    length: i64,
    format: i64,
) -> *const c_char {
    guarded(empty_string(), || {
        let Some(bytes) = input_bytes(data, length) else {
            return empty_string();
        };
        let Some(session) = parse_session(bytes, format) else {
            return empty_string();
        };
        let encoded = encode_der(session).map(|der| hex_encode(&der));
        SSL_SESSION_free(session);
        encoded.map(stash).unwrap_or_else(empty_string)
    })
}

/// C ABI: exports one DER-hex session as DER hex or PEM, or "" on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_openssl_session_export(
    der_hex: *const c_char,
    length: i64,
    format: i64,
) -> *const c_char {
    guarded(empty_string(), || {
        let Some(text) = input_bytes(der_hex, length) else {
            return empty_string();
        };
        let Some(session) = session_from_der_hex(text) else {
            return empty_string();
        };
        let output = match format {
            OPENSSL_ENCODING_DER => encode_der(session).map(|der| hex_encode(&der)),
            OPENSSL_ENCODING_PEM => encode_pem(session),
            _ => None,
        };
        SSL_SESSION_free(session);
        output.map(stash).unwrap_or_else(empty_string)
    })
}

/// C ABI: returns one DER-hex session's id as hex, or "" on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_openssl_session_id(
    der_hex: *const c_char,
    length: i64,
) -> *const c_char {
    guarded(empty_string(), || {
        let Some(text) = input_bytes(der_hex, length) else {
            return empty_string();
        };
        let Some(session) = session_from_der_hex(text) else {
            return empty_string();
        };
        let id = id_hex(session);
        SSL_SESSION_free(session);
        stash(id)
    })
}

/// C ABI: returns one DER-hex session's protocol label, or "" for unknown versions.
#[no_mangle]
pub unsafe extern "C" fn elephc_openssl_session_protocol(
    der_hex: *const c_char,
    length: i64,
) -> *const c_char {
    guarded(empty_string(), || {
        let Some(text) = input_bytes(der_hex, length) else {
            return empty_string();
        };
        let Some(session) = session_from_der_hex(text) else {
            return empty_string();
        };
        let protocol = protocol_name(session).to_vec();
        SSL_SESSION_free(session);
        stash(protocol)
    })
}

/// C ABI: returns one DER-hex session's cipher name, or "" when libssl has none.
#[no_mangle]
pub unsafe extern "C" fn elephc_openssl_session_cipher(
    der_hex: *const c_char,
    length: i64,
) -> *const c_char {
    guarded(empty_string(), || {
        let Some(text) = input_bytes(der_hex, length) else {
            return empty_string();
        };
        let Some(session) = session_from_der_hex(text) else {
            return empty_string();
        };
        let cipher = cipher_name(session);
        SSL_SESSION_free(session);
        stash(cipher)
    })
}

/// C ABI: returns one integer session metadata field, or `i64::MIN` on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_openssl_session_field(
    der_hex: *const c_char,
    length: i64,
    field: i64,
) -> i64 {
    guarded(INVALID_INT, || {
        let Some(text) = input_bytes(der_hex, length) else {
            return INVALID_INT;
        };
        let Some(session) = session_from_der_hex(text) else {
            return INVALID_INT;
        };
        let value = session_field(session, field).unwrap_or(INVALID_INT);
        SSL_SESSION_free(session);
        value
    })
}

#[cfg(all(test, elephc_openssl_ssl_native))]
mod tests {
    use super::*;

    /// The committed TLS 1.2 session generated against a local OpenSSL 3.6.4 server.
    const SESSION_PEM: &str = include_str!("../tests/php_session_tls12.pem");

    /// Verifies import, every metadata field, and DER/PEM round trips against libssl.
    #[test]
    fn tls12_session_metadata_and_round_trip() {
        unsafe {
            let session = parse_pem(SESSION_PEM.as_bytes()).expect("PEM session parses");
            assert_eq!(
                id_hex(session),
                b"6012d1b555d36c328ec54c03b97e845db7608edf748ff5972948fad22a202d26"
            );
            assert_eq!(protocol_name(session), b"TLSv1.2");
            assert_eq!(cipher_name(session), b"ECDHE-RSA-AES256-GCM-SHA384");
            assert_eq!(session_field(session, FIELD_TIMEOUT), Some(7200));
            assert_eq!(session_field(session, FIELD_RESUMABLE), Some(1));
            assert_eq!(session_field(session, FIELD_HAS_TICKET), Some(1));
            assert_eq!(session_field(session, FIELD_TICKET_LIFETIME_HINT), Some(7200));
            assert!(session_field(session, FIELD_CREATED_AT).unwrap() > 0);

            let der = encode_der(session).expect("DER encodes");
            let from_der = parse_der(&der).expect("DER parses");
            assert_eq!(id_hex(from_der), id_hex(session));
            assert_eq!(protocol_name(from_der), protocol_name(session));
            assert_eq!(cipher_name(from_der), cipher_name(session));
            SSL_SESSION_free(from_der);

            let pem = encode_pem(session).expect("PEM encodes");
            assert!(pem.starts_with(b"-----BEGIN SSL SESSION PARAMETERS-----"));
            assert_eq!(pem, SESSION_PEM.as_bytes());

            let from_pem = parse_pem(&pem).expect("exported PEM parses");
            assert_eq!(id_hex(from_pem), id_hex(session));
            SSL_SESSION_free(from_pem);

            SSL_SESSION_free(session);
        }
    }

    /// Verifies malformed DER/PEM, unknown formats and unknown fields fail closed.
    #[test]
    fn malformed_sessions_and_unknown_fields_fail() {
        unsafe {
            assert!(parse_der(b"").is_none());
            assert!(parse_der(b"not a session").is_none());
            assert!(parse_pem(b"").is_none());
            assert!(parse_pem(b"not a session").is_none());
            assert!(parse_session(b"x", 1).is_none());
        }
        assert!(hex_decode(b"abc").is_none());
        assert!(hex_decode(b"zz").is_none());
        assert_eq!(hex_decode(b"0aff"), Some(vec![0x0a, 0xff]));
    }
}
