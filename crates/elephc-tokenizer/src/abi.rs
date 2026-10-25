//! Purpose:
//! Panic-contained C ABI for the tokenizer bridge's owned token snapshots.
//!
//! Called from:
//! - The compiled tokenizer prelude through ordinary target-aware extern calls.
//!
//! Key details:
//! - Each snapshot owns its token metadata and error. It never borrows the PHP source.
//! - No returned pointer refers to a mutable global return buffer.
//! - The prelude releases snapshots in a finally block, including ParseError paths.

use std::ffi::{c_char, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;
use crate::Token;

/// One caller-owned tokenization result.
pub struct Scan {
    tokens: Vec<Token>,
    error: CString,
    failed: bool,
    error_line: i64,
}

/// Creates a snapshot from a readable byte buffer. The caller must release it once.
///
/// # Safety
/// A nonempty source must point to `len` readable bytes for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_scan(source: *const u8, len: usize, flags: i64) -> *mut Scan {
    catch_unwind(AssertUnwindSafe(|| {
        if source.is_null() && len != 0 { return std::ptr::null_mut(); }
        let bytes = if len == 0 { &[] } else { std::slice::from_raw_parts(source, len) };
        let (tokens, error, failed, error_line) = match crate::tokenize(bytes, flags) {
            Ok(tokens) => (tokens, String::new(), false, 0),
            Err(error) => (Vec::new(), error.message.replace('\0', "\\0"), true, error.line as i64),
        };
        Box::into_raw(Box::new(Scan { tokens, error: CString::new(error).unwrap_or_default(), failed, error_line }))
    })).unwrap_or(std::ptr::null_mut())
}

/// Returns the token count, or -1 when the scan failed.
///
/// # Safety
/// `scan` must be null or a live snapshot returned by `elephc_tokenizer_scan`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_count(scan: *const Scan) -> i64 {
    catch_unwind(AssertUnwindSafe(|| match scan.as_ref() {
        Some(scan) if !scan.failed => scan.tokens.len() as i64,
        _ => -1,
    })).unwrap_or(-1)
}

/// Reads a token field: 0 = ID, 1 = byte position, 2 = byte length, 3 = source line.
///
/// # Safety
/// `scan` must be null or a live snapshot returned by `elephc_tokenizer_scan`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_field(scan: *const Scan, index: usize, field: i64) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(token) = scan.as_ref().and_then(|s| s.tokens.get(index)) else { return 0; };
        match field { 0 => token.id, 1 => token.pos as i64, 2 => token.len as i64, 3 => token.line as i64, _ => 0 }
    })).unwrap_or(0)
}

/// Returns a snapshot-owned, NUL-terminated error message, valid until release.
///
/// # Safety
/// `scan` must be null or a live snapshot returned by `elephc_tokenizer_scan`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_error(scan: *const Scan) -> *const c_char {
    catch_unwind(AssertUnwindSafe(|| scan.as_ref().map_or(c"Tokenizer could not allocate a result".as_ptr(), |s| s.error.as_ptr())))
        .unwrap_or(c"Tokenizer failed".as_ptr())
}

/// Returns the failing source-buffer line, or zero when no parser location is available.
///
/// # Safety
/// `scan` must be null or a live snapshot returned by `elephc_tokenizer_scan`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_error_line(scan: *const Scan) -> i64 {
    catch_unwind(AssertUnwindSafe(|| scan.as_ref().map_or(0, |s| s.error_line))).unwrap_or(0)
}

/// Releases exactly one snapshot and its complete owned metadata.
///
/// # Safety
/// `scan` must be null or a live snapshot that has not already been released.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_release(scan: *mut Scan) {
    let _ = catch_unwind(AssertUnwindSafe(|| { if !scan.is_null() { drop(Box::from_raw(scan)); } }));
}

/// Returns an immutable process-lifetime token name or the PHP string UNKNOWN.
#[no_mangle]
pub extern "C" fn elephc_tokenizer_name(id: i64) -> *const c_char {
    static NAMES: OnceLock<Vec<(i64, CString)>> = OnceLock::new();
    catch_unwind(|| {
        let names = NAMES.get_or_init(|| elephc_builtin_contract::constants().iter().filter_map(|c| {
            if c.module != elephc_builtin_contract::PhpModule::Tokenizer || !c.name.starts_with("T_") { return None; }
            let elephc_builtin_contract::ConstValue::Int(value) = c.value else { return None; };
            if crate::token_name(value) != Some(c.name) { return None; }
            Some((value, CString::new(c.name).expect("constant name has no NUL")))
        }).collect());
        names.iter().find(|(value, _)| *value == id).map_or(c"UNKNOWN".as_ptr(), |(_, name)| name.as_ptr())
    }).unwrap_or(c"UNKNOWN".as_ptr())
}
