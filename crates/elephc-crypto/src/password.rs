//! Purpose:
//! bcrypt password hashing and verification behind PHP's `password_hash()` and
//! `password_verify()`.
//!
//! Called from:
//! - Compiled PHP programs through `crate::password_prelude` (`src/password_prelude.rs` in the
//!   compiler), which declares the two `extern "elephc_crypto"` functions below.
//! - `cargo test -p elephc-crypto`.
//!
//! Key details:
//! - Only the bcrypt family is recognised (`$2y$`, and the `$2a$`/`$2b$`/`$2x$` spellings php's
//!   `crypt()` also verifies). Any other hash verifies as false: elephc does not ship Argon2, and
//!   php reports that the same way, through `PASSWORD_ARGON2*` being undefined.
//! - Strings cross the boundary NUL-terminated, like every `CType::Str` extern. bcrypt reads a
//!   password only up to its first NUL (php's does too), so nothing is lost at the boundary.
//! - The hash string returned to the caller lives in a THREAD-LOCAL buffer, valid until the next
//!   call on the same thread (see `tests/ffi_buffer_hygiene.rs` in the compiler).

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};

thread_local! {
    /// The most recent `elephc_crypto_password_hash_bcrypt` result on this thread.
    static HASH_RESULT: RefCell<CString> = RefCell::new(CString::default());
}

/// Returns whether `hash` names the bcrypt family php's `password_verify()` accepts.
fn is_bcrypt_hash(hash: &str) -> bool {
    ["$2y$", "$2a$", "$2b$", "$2x$"]
        .iter()
        .any(|prefix| hash.starts_with(prefix))
}

/// Verifies `password` against a bcrypt `hash`; any other or malformed hash is a mismatch.
pub fn verify(password: &[u8], hash: &str) -> bool {
    is_bcrypt_hash(hash) && bcrypt::verify(password, hash).unwrap_or(false)
}

/// Hashes `password` with bcrypt at `cost` and a random salt, in php's `$2y$` spelling.
/// Returns `None` for a cost outside 4..=31.
pub fn hash(password: &[u8], cost: u32) -> Option<String> {
    bcrypt::hash_with_result(password, cost)
        .ok()
        .map(|parts| parts.format_for_version(bcrypt::Version::TwoY))
}

/// Reads a NUL-terminated argument, treating null as the empty string.
///
/// # Safety
/// `value` must be null or point at a readable NUL-terminated string.
unsafe fn c_bytes<'a>(value: *const c_char) -> &'a [u8] {
    if value.is_null() {
        return b"";
    }
    CStr::from_ptr(value).to_bytes()
}

/// C ABI of `password_verify()`: 1 when `password` matches the bcrypt `hash`, else 0.
///
/// # Safety
/// Both pointers must be null or point at readable NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_password_verify(
    password: *const c_char,
    hash: *const c_char,
) -> i64 {
    let hash = String::from_utf8_lossy(c_bytes(hash));
    i64::from(verify(c_bytes(password), &hash))
}

/// C ABI of `password_hash()` for bcrypt: the `$2y$` hash of `password` at `cost`, or the empty
/// string when the cost is out of range (the caller validated it first).
///
/// # Safety
/// `password` must be null or point at a readable NUL-terminated string. The returned pointer
/// is valid until the next call on the same thread.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_password_hash_bcrypt(
    password: *const c_char,
    cost: i64,
) -> *const c_char {
    let result = u32::try_from(cost)
        .ok()
        .and_then(|cost| hash(c_bytes(password), cost))
        .unwrap_or_default();
    HASH_RESULT.with(|cell| {
        *cell.borrow_mut() = CString::new(result).unwrap_or_default();
        cell.borrow().as_ptr()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hash php produced (`password_hash('kitten', PASSWORD_BCRYPT, ['cost' => 13])`, from the
    /// Symfony demo's fixtures) verifies, and a wrong password does not.
    #[test]
    fn verifies_a_php_bcrypt_hash() {
        let hash = "$2y$13$3EJxNVreeSXjCJooqMEDsu8jjTtCWVQWKs5QaCRZ2VcrXK1ATq0E.";
        assert!(verify(b"kitten", hash));
        assert!(!verify(b"kitten!", hash));
        assert!(!verify(b"kitten", "$argon2id$v=19$m=65536,t=4,p=1$c2FsdA$aGFzaA"));
        assert!(!verify(b"kitten", "not a hash"));
    }

    /// A fresh hash uses php's `$2y$` spelling, the requested cost, and verifies.
    #[test]
    fn hashes_in_php_spelling_and_round_trips() {
        let hashed = hash(b"secret", 4).expect("cost 4 is valid");
        assert!(hashed.starts_with("$2y$04$"));
        assert_eq!(hashed.len(), 60);
        assert!(verify(b"secret", &hashed));
        assert!(hash(b"secret", 3).is_none());
    }
}
