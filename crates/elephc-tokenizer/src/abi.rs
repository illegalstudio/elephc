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

/// Builds a NUL-terminated message, rendering embedded NUL bytes as the visible `\0` escape.
fn message_c_string(message: String) -> CString {
    CString::new(message.replace('\0', "\\0")).expect("sanitised message has no NUL")
}

/// One caller-owned tokenization result.
pub struct Scan {
    tokens: Vec<Token>,
    warnings: Vec<(CString, i64, i64)>,
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
        let (tokens, warnings, error, failed, error_line) = match crate::tokenize_with_warnings(bytes, flags) {
            Ok(output) => (
                output.tokens,
                output.warnings.into_iter().map(|warning| {
                    (message_c_string(warning.message), warning.line as i64, warning.pos as i64)
                }).collect(),
                String::new(),
                false,
                0,
            ),
            Err(error) => (Vec::new(), Vec::new(), error.message.replace('\0', "\\0"), true, error.line as i64),
        };
        Box::into_raw(Box::new(Scan { tokens, warnings, error: message_c_string(error), failed, error_line }))
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

/// Returns the number of non-fatal TOKEN_PARSE warnings, or zero on failure.
///
/// # Safety
/// `scan` must be null or a live snapshot returned by `elephc_tokenizer_scan`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_warning_count(scan: *const Scan) -> i64 {
    catch_unwind(AssertUnwindSafe(|| scan.as_ref().map_or(0, |scan| scan.warnings.len() as i64)))
        .unwrap_or(0)
}

/// Returns one warning's NUL-terminated message, valid until release.
///
/// # Safety
/// `scan` must be null or a live snapshot returned by `elephc_tokenizer_scan`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_warning_message(scan: *const Scan, index: usize) -> *const c_char {
    catch_unwind(AssertUnwindSafe(|| scan.as_ref().and_then(|scan| scan.warnings.get(index))
        .map_or(c"".as_ptr(), |(message, _, _)| message.as_ptr()))).unwrap_or(c"".as_ptr())
}

/// Returns one warning's one-based source line.
///
/// # Safety
/// `scan` must be null or a live snapshot returned by `elephc_tokenizer_scan`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_warning_line(scan: *const Scan, index: usize) -> i64 {
    catch_unwind(AssertUnwindSafe(|| scan.as_ref().and_then(|scan| scan.warnings.get(index))
        .map_or(0, |(_, line, _)| *line))).unwrap_or(0)
}

/// Returns one warning's zero-based source byte offset.
///
/// # Safety
/// `scan` must be null or a live snapshot returned by `elephc_tokenizer_scan`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tokenizer_warning_pos(scan: *const Scan, index: usize) -> i64 {
    catch_unwind(AssertUnwindSafe(|| scan.as_ref().and_then(|scan| scan.warnings.get(index))
        .map_or(0, |(_, _, pos)| *pos))).unwrap_or(0)
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
