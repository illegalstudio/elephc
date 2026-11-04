//! Purpose:
//! Stable panic-free C ABI for the PHP `ext/sodium` bridge.
//!
//! Called from:
//! - Compiled PHP programs through the `extern "elephc_sodium"` block in
//!   `src/sodium_prelude`, with the managed `libsodium` package supplying the
//!   `crypto_*`/`sodium_*` symbols at final link time.
//! - `cargo test -p elephc-sodium` when `ELEPHC_SODIUM_LIB_DIR` points at a built libsodium.
//!
//! Key details:
//! - PHP strings cross the C boundary as hexadecimal text, because Elephc's `char*`
//!   extern ABI truncates embedded NUL bytes and cryptographic data is binary. The
//!   compiled prelude encodes with `bin2hex` and decodes the packet with `hex2bin`.
//! - The return packet is one of `O<hex>` (success bytes), `M<decimal>` (integer/bool
//!   result), or a single failure letter (`F`, `B`, `H`, `P`, `C`, `I`, `Z`, `E`).
//! - Panics never cross the FFI boundary: they become an `E` packet.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Once;

use crate::ops::Outcome;

mod ffi;
mod ops;

thread_local! {
    /// Packet returned by the most recent call on this thread.
    static PACKET: RefCell<CString> = RefCell::new(CString::default());
}

/// Initializes libsodium exactly once per process.
fn initialize() {
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        let _ = ffi::sodium_init();
    });
}

/// Copies a packet into this thread's cell and returns its pointer.
fn stash(packet: String) -> *const c_char {
    let value = CString::new(packet.replace('\0', "?")).unwrap_or_default();
    PACKET.with(|cell| {
        let mut cell = cell.borrow_mut();
        *cell = value;
        cell.as_ptr()
    })
}

/// Encodes one successful byte result as a packet.
fn bytes_packet(bytes: &[u8]) -> String {
    let mut packet = String::with_capacity(bytes.len() * 2 + 1);
    packet.push('O');
    for byte in bytes {
        packet.push_str(&format!("{byte:02x}"));
    }
    packet
}

/// Encodes one integer or boolean result as a packet.
fn integer_packet(value: i64) -> String {
    format!("M{value}")
}

/// Converts one bridge outcome into its wire packet.
fn outcome_packet(outcome: Outcome) -> String {
    match outcome {
        Outcome::Bytes(bytes) => bytes_packet(&bytes),
        Outcome::Integer(value) => integer_packet(value),
        Outcome::False => "F".to_string(),
        Outcome::InvalidBase64 => "B".to_string(),
        Outcome::InvalidHex => "H".to_string(),
        Outcome::InvalidPadding => "P".to_string(),
        Outcome::ConversionFailed => "C".to_string(),
        Outcome::IdentityElement => "I".to_string(),
        Outcome::ZeroScalar => "Z".to_string(),
        Outcome::Internal => "E".to_string(),
    }
}

/// Runs one bridge operation with panic containment.
fn contained(operation: impl FnOnce() -> Outcome) -> *const c_char {
    initialize();
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(outcome) => stash(outcome_packet(outcome)),
        Err(_) => stash("E".to_string()),
    }
}

/// Reads one hexadecimal argument as raw bytes.
///
/// # Safety
/// `pointer` must be null or point at a NUL-terminated byte sequence valid for the call.
unsafe fn arg_bytes(pointer: *const c_char) -> Vec<u8> {
    if pointer.is_null() {
        return Vec::new();
    }
    let text = CStr::from_ptr(pointer).to_bytes();
    decode_hex(text)
}

/// Decodes uppercase or lowercase hexadecimal, treating an odd trailing byte as invalid.
fn decode_hex(text: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(text.len() / 2);
    let mut index = 0;
    while index + 1 < text.len() {
        let Some(high) = hex_digit(text[index]) else {
            return Vec::new();
        };
        let Some(low) = hex_digit(text[index + 1]) else {
            return Vec::new();
        };
        output.push((high << 4) | low);
        index += 2;
    }
    output
}

/// Returns the numeric value of one hexadecimal digit.
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Runs one named sodium operation over four hexadecimal arguments and three integers.
///
/// # Safety
/// Every pointer must be null or point at a NUL-terminated C string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_call(
    name: *const c_char,
    a0: *const c_char,
    a1: *const c_char,
    a2: *const c_char,
    a3: *const c_char,
    i0: i64,
    i1: i64,
    i2: i64,
) -> *const c_char {
    let name = if name.is_null() {
        String::new()
    } else {
        CStr::from_ptr(name).to_string_lossy().into_owned()
    };
    let arguments = [arg_bytes(a0), arg_bytes(a1), arg_bytes(a2), arg_bytes(a3)];
    contained(|| {
        let slices = [
            arguments[0].as_slice(),
            arguments[1].as_slice(),
            arguments[2].as_slice(),
            arguments[3].as_slice(),
        ];
        ops::dispatch(&name, slices, [i0, i1, i2])
    })
}

/// Derives a key with Argon2 through a dedicated six-register entry point.
///
/// The generic bridge has room for only three integers, while `sodium_crypto_pwhash`
/// needs four; this entry keeps the algorithm as a register instead of encoding it in a
/// string slot.
///
/// # Safety
/// Both pointers must be null or point at a NUL-terminated hexadecimal string.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_pwhash(
    password: *const c_char,
    salt: *const c_char,
    length: i64,
    ops_limit: i64,
    mem_limit: i64,
    algorithm: i64,
) -> *const c_char {
    let password = arg_bytes(password);
    let salt = arg_bytes(salt);
    contained(|| ops::pwhash(&password, &salt, length, ops_limit, mem_limit, algorithm))
}

/// Returns the linked libsodium version string.
#[no_mangle]
pub extern "C" fn elephc_sodium_version() -> *const c_char {
    initialize();
    stash(ops::version())
}

/// Returns the linked libsodium library major version.
#[no_mangle]
pub extern "C" fn elephc_sodium_version_major() -> i64 {
    initialize();
    ops::version_major()
}

/// Returns the linked libsodium library minor version.
#[no_mangle]
pub extern "C" fn elephc_sodium_version_minor() -> i64 {
    initialize();
    ops::version_minor()
}
