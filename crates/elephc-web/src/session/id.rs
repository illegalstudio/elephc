//! Purpose:
//! Owns PHP session ID generation and validation for `--web` mode: random ID
//! creation from `/dev/urandom` via PHP's `bin_to_readable` charset encoding
//! (`session.sid_length` / `session.sid_bits_per_character`), and the charset/
//! length rules used to accept or reject session IDs coming from the
//! filesystem or the client cookie.
//!
//! Called from:
//! - The compiled `--web` web prelude via the `elephc_web_session_create_id`
//!   C-ABI symbol.
//! - `session::file_io`, which calls `validate_session_id` before touching the
//!   filesystem for read/write/destroy/file_exists/touch, and `read_random`
//!   for `elephc_web_session_should_gc`'s probability sampling.
//!
//! Key details:
//! - One process per prefork worker, single-threaded, so the `RET_STRING`
//!   return buffer (owned by `state`) is race-free across calls.
//! - `/dev/urandom` is the sole entropy source; a time-based fallback is used
//!   only if the device is unavailable (never on a supported target in normal
//!   operation).
//! - `bin_to_readable` mirrors PHP's low-endian bit-accumulation encoder
//!   (`ext/session/session.c`): `sid_bits_per_character` selects a charset —
//!   4 → `0-9a-f`, 5 → `0-9a-v`, 6 → `0-9a-zA-Z,-` — read `bits` LSB-first out
//!   of the random byte stream.

use super::state::{opt_ptr, set_cstr, RET_STRING, SID_BITS_PER_CHARACTER, SID_LENGTH};
use std::ffi::c_char;
use std::fs::File;
use std::io::Read;

/// PHP's session-ID digit table (`hexconvtab` in `ext/session/session.c`):
/// index `0..16` is the 4-bit charset, `0..32` the 5-bit charset, and the
/// full 64 entries the 6-bit charset.
const ID_CHARSET_TABLE: &[u8; 64] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ,-";

/// php-src's independent upper bound for a complete incoming session ID.
/// `session.sid_length` controls only the generated random suffix length.
const MAX_SESSION_ID_LENGTH: usize = 256;

/// Returns true if every byte in `s` is in PHP's session-ID/prefix charset:
/// `[a-zA-Z0-9,-]`. Shared by prefix validation (`create_id`) and incoming-ID
/// validation (`validate_session_id`). Does not check emptiness — callers
/// decide whether an empty string is acceptable in their context (an empty
/// prefix is valid; an empty incoming ID is not).
fn has_valid_id_charset(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b',' || b == b'-')
}

/// Fills `buf` with random bytes from `/dev/urandom`, falling back to a
/// time-based seed if the device is unavailable (never expected on a
/// supported target in normal operation). Shared by `create_id` (ID entropy)
/// and `file_io::elephc_web_session_should_gc` (probability sampling) — the
/// single `/dev/urandom` primitive the spec asks every random consumer to
/// reuse rather than pulling in a new crate.
pub(super) fn read_random(buf: &mut [u8]) {
    match File::open("/dev/urandom").and_then(|mut f| f.read_exact(buf)) {
        Ok(()) => {}
        Err(_) => {
            // Fallback: seed from a monotonically-changing time value if
            // /dev/urandom is unavailable.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let seed = now.to_le_bytes();
            for (i, b) in buf.iter_mut().enumerate() {
                *b = seed[i % seed.len()];
            }
        }
    }
}

/// Converts raw random bytes to PHP's session-ID charset using low-endian bit
/// accumulation, mirroring PHP's `bin_to_readable` (`ext/session/session.c`).
/// `bits` (4, 5, or 6) selects how many low bits of the accumulator are
/// consumed per output character: 4 → `0-9a-f`, 5 → `0-9a-v`,
/// 6 → `0-9a-zA-Z,-`.
pub(super) fn bin_to_readable(data: &[u8], bits: u32) -> String {
    let mask: u32 = (1u32 << bits) - 1;
    // Worst case emits one character per `bits` bits, plus one trailing
    // partial character.
    let mut out = String::with_capacity(data.len() * 8 / bits as usize + 1);
    let mut acc: u32 = 0;
    let mut have: u32 = 0;
    let mut iter = data.iter();
    loop {
        if have < bits {
            match iter.next() {
                Some(&byte) => {
                    // Accumulate the next byte above the bits already held.
                    acc |= (byte as u32) << have;
                    have += 8;
                }
                // Input exhausted: stop before emitting a character for this
                // iteration (matches PHP's `break` inside the refill branch).
                None => break,
            }
        }
        out.push(ID_CHARSET_TABLE[(acc & mask) as usize] as char);
        acc >>= bits;
        have -= bits;
    }
    // Emit the final partial character left over from the last full byte.
    if have > 0 {
        out.push(ID_CHARSET_TABLE[(acc & mask) as usize] as char);
    }
    out
}

/// Reads 16 bytes from `/dev/urandom` and converts them to 32 lowercase hex
/// characters by default (per the configured `sid_length`/
/// `sid_bits_per_character`, defaults 32/4). An optional `prefix` is
/// prepended to the generated suffix. The prefix must use the session-ID
/// charset `[a-zA-Z0-9,-]`; version-specific length and NUL validation happens
/// in the PHP prelude before crossing this C-string ABI. The produced ID is not
/// capped here because PHP 8.2 and 8.3 accepted prefixes longer than 256 bytes,
/// while PHP 8.4 introduced the cap. Returns the new ID as a NUL-terminated C string,
/// valid until the next session C-ABI call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_create_id(
    prefix_ptr: *const c_char,
) -> *const c_char {
    let prefix: String = if prefix_ptr.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(prefix_ptr)
            .to_string_lossy()
            .into_owned()
    };

    if !has_valid_id_charset(&prefix) {
        set_cstr(core::ptr::addr_of_mut!(RET_STRING), "");
        return opt_ptr(core::ptr::addr_of!(RET_STRING));
    }

    let sid_length = (*core::ptr::addr_of!(SID_LENGTH)).max(1) as usize;
    let sid_bits = *core::ptr::addr_of!(SID_BITS_PER_CHARACTER) as u32;

    // nbytes = ceil(sid_length * sid_bits / 8): enough entropy to produce at
    // least sid_length readable characters.
    let nbytes = (sid_length * sid_bits as usize + 7) / 8;
    let mut random_bytes = vec![0u8; nbytes.max(1)];
    read_random(&mut random_bytes);

    let mut readable = bin_to_readable(&random_bytes, sid_bits);
    // bin_to_readable can emit one character beyond sid_length on some
    // bits/length combinations (ceiling-division rounding); truncate to the
    // configured length so the output always matches sid_length exactly.
    readable.truncate(sid_length);

    let mut id = String::with_capacity(prefix.len() + readable.len());
    id.push_str(&prefix);
    id.push_str(&readable);

    set_cstr(core::ptr::addr_of_mut!(RET_STRING), &id);
    opt_ptr(core::ptr::addr_of!(RET_STRING))
}

/// Validates a complete incoming session ID: length must be 1 through php-src's
/// fixed 256-byte maximum and every character must be in `a-zA-Z0-9,-`.
/// `session.sid_length` is deliberately not consulted because it controls only
/// the generated random suffix; `session_create_id($prefix)` prepends to it.
pub(super) fn validate_session_id(id: &str) -> bool {
    let len = id.len();
    if len == 0 || len > MAX_SESSION_ID_LENGTH {
        return false;
    }
    has_valid_id_charset(id)
}

#[cfg(test)]
mod tests {
    use super::super::state::test_lock as lock;
    use super::*;

    /// Verifies session ID generation produces a 32-hex-char string by
    /// default (sid_length=32, sid_bits_per_character=4 — unchanged from
    /// before the bin_to_readable rewrite).
    #[test]
    fn session_create_id_is_32_hex() {
        let _g = lock();
        unsafe {
            super::super::state::elephc_web_session_reset();
            let id = std::ffi::CStr::from_ptr(elephc_web_session_create_id(std::ptr::null()));
            let s = id.to_str().unwrap();
            assert_eq!(s.len(), 32, "expected 32 hex chars, got {s}");
            assert!(s.bytes().all(|b| b.is_ascii_hexdigit()), "not hex: {s}");
        }
    }

    /// Verifies session ID generation with a prefix.
    #[test]
    fn session_create_id_with_prefix() {
        let _g = lock();
        unsafe {
            super::super::state::elephc_web_session_reset();
            let prefix = std::ffi::CString::new("abc-").unwrap();
            let id = std::ffi::CStr::from_ptr(elephc_web_session_create_id(
                prefix.as_ptr(),
            ));
            let s = id.to_str().unwrap();
            assert!(s.starts_with("abc-"));
            assert_eq!(s.len(), 36); // 4 prefix + 32 hex
            assert!(validate_session_id(s));
        }
    }

    /// Verifies complete session IDs use php-src's fixed 256-byte cap rather
    /// than the generated suffix length.
    #[test]
    fn session_id_validation() {
        assert!(validate_session_id("abc123"));
        assert!(validate_session_id("a,b-c,d"));
        assert!(validate_session_id("0123456789abcdef0123456789abcdef")); // 32 chars
        assert!(validate_session_id(&"x".repeat(256)));
        assert!(!validate_session_id(""));
        assert!(!validate_session_id("with space"));
        assert!(!validate_session_id("with;semicolon"));
        assert!(!validate_session_id(&"x".repeat(257)));
    }

    /// Verifies changing the random suffix length does not change validation of
    /// complete IDs, including IDs longer than the configured suffix.
    #[test]
    fn validate_session_id_is_independent_of_sid_length() {
        let _g = lock();
        unsafe {
            super::super::state::elephc_web_session_reset();
            let ok32 = "a".repeat(32);
            let prefixed35 = "a".repeat(35);
            assert!(validate_session_id(&ok32));
            assert!(validate_session_id(&prefixed35));

            assert_eq!(super::super::state::elephc_web_session_set_sid_length(200), 1);
            let ok200 = "a".repeat(200);
            let prefixed203 = "a".repeat(203);
            assert!(validate_session_id(&ok200));
            assert!(validate_session_id(&prefixed203));

            super::super::state::elephc_web_session_reset();
        }
    }

    /// Verifies the bridge accepts long valid prefixes for PHP 8.2/8.3 and
    /// rejects bad charset bytes; the prelude applies PHP 8.4+'s length cap.
    #[test]
    fn create_id_rejects_invalid_prefix() {
        let _g = lock();
        unsafe {
            super::super::state::elephc_web_session_reset();

            // The bridge accepts a long valid prefix because the versioned
            // PHP prelude owns PHP 8.4+'s 256-byte ValueError boundary.
            let long_prefix = std::ffi::CString::new("a".repeat(257)).unwrap();
            let id = std::ffi::CStr::from_ptr(elephc_web_session_create_id(long_prefix.as_ptr()));
            assert_eq!(id.to_str().unwrap().len(), 257 + 32);

            // Prefix with a disallowed character is rejected -> empty string.
            let bad_prefix = std::ffi::CString::new("abc!def").unwrap();
            let id2 = std::ffi::CStr::from_ptr(elephc_web_session_create_id(bad_prefix.as_ptr()));
            assert_eq!(id2.to_str().unwrap(), "");

            // Prefix exactly at the 256-char boundary is accepted, and the
            // produced ID is prefix (256) + default suffix (32) = 288 total,
            // i.e. NOT capped at 256.
            let ok_prefix = "p".repeat(256);
            let okc = std::ffi::CString::new(ok_prefix.clone()).unwrap();
            let id3 = std::ffi::CStr::from_ptr(elephc_web_session_create_id(okc.as_ptr()));
            let s3 = id3.to_str().unwrap();
            assert!(s3.starts_with(&ok_prefix));
            assert_eq!(s3.len(), 256 + 32);
        }
    }

    /// §2.7: create_id's output charset and length track
    /// `sid_bits_per_character` (4 -> 0-9a-f, 5 -> 0-9a-v, 6 -> 0-9a-zA-Z,-),
    /// and the default (bits=4, length=32) stays 32 lowercase hex chars.
    #[test]
    fn create_id_charset_and_length_track_sid_bits() {
        let _g = lock();
        unsafe {
            super::super::state::elephc_web_session_reset();

            // Default: bits=4 -> charset 0-9a-f, 32 chars.
            let id = std::ffi::CStr::from_ptr(elephc_web_session_create_id(std::ptr::null()));
            let s = id.to_str().unwrap();
            assert_eq!(s.len(), 32);
            assert!(
                s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
                "not 4-bit charset: {s}"
            );

            // bits=5 -> charset 0-9a-v.
            assert_eq!(super::super::state::elephc_web_session_set_sid_bits_per_character(5), 1);
            let id5 = std::ffi::CStr::from_ptr(elephc_web_session_create_id(std::ptr::null()));
            let s5 = id5.to_str().unwrap();
            assert_eq!(s5.len(), 32);
            assert!(
                s5.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'v')),
                "not 5-bit charset: {s5}"
            );

            // bits=6 -> charset 0-9a-zA-Z,-.
            assert_eq!(super::super::state::elephc_web_session_set_sid_bits_per_character(6), 1);
            let id6 = std::ffi::CStr::from_ptr(elephc_web_session_create_id(std::ptr::null()));
            let s6 = id6.to_str().unwrap();
            assert_eq!(s6.len(), 32);
            assert!(
                s6.bytes().all(|b| b.is_ascii_alphanumeric() || b == b',' || b == b'-'),
                "not 6-bit charset: {s6}"
            );

            // Out-of-range bits are rejected (PHP only defines 4/5/6).
            assert_eq!(super::super::state::elephc_web_session_set_sid_bits_per_character(3), 0);
            assert_eq!(super::super::state::elephc_web_session_set_sid_bits_per_character(7), 0);

            super::super::state::elephc_web_session_reset();
        }
    }
}
