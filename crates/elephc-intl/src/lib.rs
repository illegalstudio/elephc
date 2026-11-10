//! Purpose:
//! Rust staticlib boundary between generated PHP programs and the managed ICU shim.
//!
//! Called from:
//! - `elephc_intl_*` C ABI calls emitted by the Intl synthetic class/function metadata.
//! - `cargo test -p elephc-intl` for handle, hex, and error-state unit tests.
//!
//! Key details:
//! - Handles are opaque ICU objects owned by `libelephc_icu_intl_shim.a`; this crate never
//!   dereferences them.
//! - Binary payloads cross the C ABI as lowercase hex, because `char *` is NUL-terminated.
//! - Returned text lives in a per-thread `CString`; a global cell would be a cross-thread
//!   use-after-free in compiled web programs.
//! - The shim is linked by the managed `icu` native package, not by this crate's build.

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};

/// Maximum encoded payload accepted from generated PHP code (64 MiB).
const MAX_PAYLOAD: usize = 64 * 1024 * 1024;

/// Result buffer used for the single (non-sizing) bridge invocation.
///
/// A sizing call would execute a mutating ICU method twice; 1 MiB is far larger than any
/// encoded Intl result (the full locale/zone/transliterator enumerations are tens of KiB).
const CALL_RESULT_CAPACITY: usize = 1024 * 1024;

/// ICU's `U_BUFFER_OVERFLOW_ERROR`: the shim cached the full payload for a fetch.
const U_BUFFER_OVERFLOW_ERROR: i32 = 15;

extern "C" {
    /// Opens one ICU-backed object and returns its handle, or zero on failure.
    fn elephc_icu_intl_open(
        kind: *const c_char,
        spec: *const u8,
        spec_len: u64,
        error: *mut i32,
    ) -> i64;
    /// Releases one handle.
    fn elephc_icu_intl_close(handle: i64);
    /// Clones one handle.
    fn elephc_icu_intl_clone(handle: i64, error: *mut i32) -> i64;
    /// Runs one instance method against the binary value stream.
    fn elephc_icu_intl_call(
        handle: i64,
        method: *const c_char,
        args: *const u8,
        args_len: u64,
        out: *mut u8,
        out_cap: u64,
        needed: *mut u64,
        error: *mut i32,
    ) -> i32;
    /// Runs one class-level operation.
    fn elephc_icu_intl_static(
        kind: *const c_char,
        method: *const c_char,
        args: *const u8,
        args_len: u64,
        out: *mut u8,
        out_cap: u64,
        needed: *mut u64,
        error: *mut i32,
    ) -> i32;
    /// Returns one handle's last ICU status code.
    fn elephc_icu_intl_error_code(handle: i64) -> i32;
    /// Copies one handle's last ICU error name.
    fn elephc_icu_intl_error_message(
        handle: i64,
        buffer: *mut c_char,
        capacity: u64,
        needed: *mut u64,
    ) -> i32;
    /// Copies the symbolic name of one ICU status code.
    fn elephc_icu_intl_error_name(
        code: i32,
        buffer: *mut c_char,
        capacity: u64,
        needed: *mut u64,
    ) -> i32;
    /// Copies the payload cached by the last overflowed bridge call.
    fn elephc_icu_intl_fetch_result(out: *mut u8, capacity: u64, needed: *mut u64) -> i32;
    /// Returns the number of live handles.
    fn elephc_icu_intl_live_count() -> i64;
    /// Copies the ICU version string.
    fn elephc_icu_intl_version(buffer: *mut c_char, capacity: u64, needed: *mut u64) -> i32;
    /// Copies the ICU data version string.
    fn elephc_icu_intl_data_version(buffer: *mut c_char, capacity: u64, needed: *mut u64) -> i32;
}

/// Converts one shim result, fetching the cached full payload when the fixed buffer overflowed.
///
/// # Safety
/// Calls the shim fetch entry point, which is only valid on the thread that made the call.
unsafe fn finish_result(status: i32, written: u64, payload: &[u8]) -> String {
    if status == U_BUFFER_OVERFLOW_ERROR {
        let mut full = vec![0u8; written as usize];
        let mut needed = 0u64;
        let fetched = elephc_icu_intl_fetch_result(
            full.as_mut_ptr(),
            full.len() as u64,
            &mut needed,
        );
        if fetched == 0 {
            full.truncate(needed as usize);
            return encode_hex(&full);
        }
        return String::new();
    }
    if status != 0 && written == 0 {
        return String::new();
    }
    let length = (written as usize).min(payload.len());
    encode_hex(&payload[..length])
}

thread_local! {
    /// Last value returned through a string-returning C ABI call on this thread.
    static RETURNED: RefCell<CString> = RefCell::new(CString::default());
}

/// Thread-local last error for class-level (static) Intl operations.
struct StaticError {
    /// ICU status code, zero when the last operation succeeded.
    code: i32,
    /// Symbolic ICU error name for [`Self::code`].
    message: String,
}

thread_local! {
    /// Last static-operation error on this thread, mirroring php-src's global intl error.
    static STATIC_ERROR: RefCell<StaticError> = RefCell::new(StaticError { code: 0, message: String::new() });
}

/// Replaces the thread's returned string and hands out a stable pointer for the call.
fn stash(value: String) -> *const c_char {
    RETURNED.with(|cell| {
        let bytes = CString::new(value).unwrap_or_default();
        *cell.borrow_mut() = bytes;
        cell.borrow().as_ptr()
    })
}

/// Decodes a lowercase or uppercase hexadecimal payload, rejecting odd or oversized input.
fn decode_hex(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.len() % 2 != 0 || bytes.len() / 2 > MAX_PAYLOAD {
        return None;
    }
    let digit = |byte: u8| -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        out.push((digit(pair[0])? << 4) | digit(pair[1])?);
    }
    Some(out)
}

/// Encodes bytes as lowercase hexadecimal for the generated PHP side.
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[(byte >> 4) as usize]));
        out.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    out
}

/// Borrows one NUL-terminated argument, returning the empty string for a null pointer.
fn argument(pointer: *const c_char) -> &'static str {
    if pointer.is_null() {
        return "";
    }
    // SAFETY: generated code only passes NUL-terminated literals or PHP strings, and the
    // bridge copies out of the slice before returning.
    unsafe { CStr::from_ptr(pointer) }.to_str().unwrap_or("")
}

/// Sizes and copies one shim string into a Rust `String`.
fn copy_string(
    operation: impl Fn(*mut c_char, u64, *mut u64) -> i32,
) -> String {
    let mut needed: u64 = 0;
    let status = operation(std::ptr::null_mut(), 0, &mut needed);
    if status != 0 || needed == 0 || needed as usize > MAX_PAYLOAD {
        return String::new();
    }
    let mut buffer = vec![0u8; needed as usize];
    let mut written: u64 = 0;
    let status = operation(buffer.as_mut_ptr().cast(), needed, &mut written);
    if status != 0 {
        return String::new();
    }
    buffer.truncate(written.min(needed) as usize);
    String::from_utf8(buffer).unwrap_or_default()
}

/// Reads the symbolic ICU name for one status code.
///
/// Unit tests link this crate without the managed shim, so the test build uses a small
/// fallback for the statuses those tests exercise.
#[cfg(test)]
fn error_name(code: i32) -> String {
    match code {
        0 => "U_ZERO_ERROR".to_string(),
        1 => "U_ILLEGAL_ARGUMENT_ERROR".to_string(),
        other => format!("U_ERROR_{other}"),
    }
}

/// Reads the symbolic ICU name for one status code.
#[cfg(not(test))]
fn error_name(code: i32) -> String {
    copy_string(|buffer, capacity, needed| {
        // SAFETY: buffer/capacity/needed obey the shim's output contract.
        unsafe { elephc_icu_intl_error_name(code, buffer, capacity, needed) }
    })
}

/// Records a class-level operation failure for `intl_get_error_code()`/`_message()`.
fn record_static_error(code: i32) {
    let message = if code == 0 { String::new() } else { error_name(code) };
    STATIC_ERROR.with(|cell| {
        *cell.borrow_mut() = StaticError { code, message };
    });
}

/// Opens one Intl object. `spec` is a hexadecimal value stream.
///
/// # Safety
/// `kind` and `spec` must be NUL-terminated strings supplied by generated code.
#[no_mangle]
pub unsafe extern "C" fn elephc_intl_open(kind: *const c_char, spec: *const c_char) -> i64 {
    let family = argument(kind);
    let Some(spec) = decode_hex(argument(spec)) else {
        record_static_error(1);
        return 0;
    };
    let kind = CString::new(family).unwrap_or_default();
    let mut error = 0i32;
    let handle = elephc_icu_intl_open(kind.as_ptr(), spec.as_ptr(), spec.len() as u64, &mut error);
    record_static_error(error);
    handle
}

/// Releases one Intl object handle.
#[no_mangle]
pub extern "C" fn elephc_intl_close(handle: i64) {
    // SAFETY: close ignores unknown and already released handles.
    unsafe { elephc_icu_intl_close(handle) }
}

/// Clones one Intl object handle, returning zero on failure.
#[no_mangle]
pub extern "C" fn elephc_intl_clone(handle: i64) -> i64 {
    let mut error = 0i32;
    // SAFETY: the shim validates the handle.
    let clone = unsafe { elephc_icu_intl_clone(handle, &mut error) };
    record_static_error(error);
    clone
}

/// Runs one instance method against the hexadecimal value stream and returns the
/// hexadecimal result. A shim error leaves an empty result; callers consult
/// `elephc_intl_error_code`.
///
/// # Safety
/// `method` and `args` must be NUL-terminated strings supplied by generated code.
#[no_mangle]
pub unsafe extern "C" fn elephc_intl_call(
    handle: i64,
    method: *const c_char,
    args: *const c_char,
) -> *const c_char {
    let Some(args) = decode_hex(argument(args)) else {
        return stash(String::new());
    };
    let method = CString::new(argument(method)).unwrap_or_default();
    // ONE call only: ICU methods mutate the calendar/formatter, so a sizing query would run
    // `add`, `roll`, `set`, and every other mutator twice. The buffer is sized generously
    // instead; an oversized result fails closed rather than replaying the mutation.
    let mut payload = vec![0u8; CALL_RESULT_CAPACITY];
    let mut written: u64 = 0;
    let mut error = 0i32;
    let status = elephc_icu_intl_call(
        handle,
        method.as_ptr(),
        args.as_ptr(),
        args.len() as u64,
        payload.as_mut_ptr(),
        payload.len() as u64,
        &mut written,
        &mut error,
    );
    stash(finish_result(status, written, &payload))
}

/// Runs one class-level Intl operation and returns the hexadecimal result.
///
/// # Safety
/// `kind`, `method`, and `args` must be NUL-terminated strings supplied by generated code.
#[no_mangle]
pub unsafe extern "C" fn elephc_intl_static(
    kind: *const c_char,
    method: *const c_char,
    args: *const c_char,
) -> *const c_char {
    let Some(args) = decode_hex(argument(args)) else {
        record_static_error(1);
        return stash(String::new());
    };
    let kind = CString::new(argument(kind)).unwrap_or_default();
    let method = CString::new(argument(method)).unwrap_or_default();
    // Static operations can also allocate handles (`collator_create`) or mutate process state
    // (`locale_set_default`), so they must run once for the same reason as instance calls.
    let mut payload = vec![0u8; CALL_RESULT_CAPACITY];
    let mut written: u64 = 0;
    let mut error = 0i32;
    let status = elephc_icu_intl_static(
        kind.as_ptr(),
        method.as_ptr(),
        args.as_ptr(),
        args.len() as u64,
        payload.as_mut_ptr(),
        payload.len() as u64,
        &mut written,
        &mut error,
    );
    record_static_error(if status == 0 { error } else { status });
    stash(finish_result(status, written, &payload))
}

/// Returns one handle's last ICU error code.
#[no_mangle]
pub extern "C" fn elephc_intl_error_code(handle: i64) -> i64 {
    // SAFETY: the shim validates the handle.
    i64::from(unsafe { elephc_icu_intl_error_code(handle) })
}

/// Returns one handle's last ICU error name.
#[no_mangle]
pub extern "C" fn elephc_intl_error_message(handle: i64) -> *const c_char {
    let message = copy_string(|buffer, capacity, needed| {
        // SAFETY: buffer/capacity/needed obey the shim's output contract.
        unsafe { elephc_icu_intl_error_message(handle, buffer, capacity, needed) }
    });
    stash(message)
}

/// Returns the class-level Intl error code.
#[no_mangle]
pub extern "C" fn elephc_intl_global_error_code() -> i64 {
    STATIC_ERROR.with(|cell| i64::from(cell.borrow().code))
}

/// Returns the class-level Intl error name.
#[no_mangle]
pub extern "C" fn elephc_intl_global_error_message() -> *const c_char {
    stash(STATIC_ERROR.with(|cell| cell.borrow().message.clone()))
}

/// Returns the number of live Intl handles.
#[no_mangle]
pub extern "C" fn elephc_intl_live_count() -> i64 {
    // SAFETY: the shim only reads its handle table.
    unsafe { elephc_icu_intl_live_count() }
}

/// Returns the managed ICU version string.
#[no_mangle]
pub extern "C" fn elephc_intl_version() -> *const c_char {
    stash(copy_string(|buffer, capacity, needed| {
        // SAFETY: buffer/capacity/needed obey the shim's output contract.
        unsafe { elephc_icu_intl_version(buffer, capacity, needed) }
    }))
}

/// Returns the managed ICU data version string.
#[no_mangle]
pub extern "C" fn elephc_intl_data_version() -> *const c_char {
    stash(copy_string(|buffer, capacity, needed| {
        // SAFETY: buffer/capacity/needed obey the shim's output contract.
        unsafe { elephc_icu_intl_data_version(buffer, capacity, needed) }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies hexadecimal decoding accepts both cases and rejects malformed input.
    #[test]
    fn hex_round_trips_and_rejects_malformed_input() {
        assert_eq!(decode_hex("00ffA0").as_deref(), Some(&[0u8, 0xff, 0xa0][..]));
        assert_eq!(decode_hex("0f").as_deref(), Some(&[0x0f][..]));
        assert_eq!(decode_hex("0"), None);
        assert_eq!(decode_hex("zz"), None);
        assert_eq!(encode_hex(&[0, 0xff, 0xa0]), "00ffa0");
    }

    /// Verifies the static error state stores the symbolic ICU name.
    #[test]
    fn static_error_records_symbolic_names() {
        record_static_error(1);
        assert_eq!(elephc_intl_global_error_code(), 1);
        let message = unsafe { CStr::from_ptr(elephc_intl_global_error_message()) }
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(message, "U_ILLEGAL_ARGUMENT_ERROR");
        record_static_error(0);
        assert_eq!(elephc_intl_global_error_code(), 0);
    }
}
