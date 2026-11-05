//! Purpose:
//! Small engine-symbol shims for PHP code that names the running Zend engine
//! (`get_zend_version`, `zend_printf`, `zend_hash_func`, `zend_str_tolower`) plus the
//! `stdout`/`stdin`/`stderr` and `zend_write` hooks ext/ffi exposes.
//!
//! Called from:
//! - `crate::context::FfiContext::symbol` when a cdef context asks for one of these names.
//!
//! Key details:
//! - Elephc is a standalone binary with no Zend engine, so these functions implement the
//!   observable contract PHP code expects rather than exposing a real engine.
//! - `zend_printf` is a global function-pointer variable; the symbol returns its address after
//!   pointing it at libc `printf`, matching the header's `extern size_t (*zend_printf)(...)`.
//! - The bug-specific `zend_test` symbols live in `crate::php_src_fixtures` behind the
//!   `php-src-fixtures` feature, so production binaries never export them.

use std::os::raw::{c_char, c_ulong};

/// Static version string returned by `get_zend_version`.
static ZEND_VERSION: &[u8] = b"Zend Engine v4.5.10, Copyright (c) Zend Technologies\0";

/// Global function-pointer variable populated by `FfiContext::symbol`.
#[no_mangle]
pub static mut zend_printf: usize = 0;

/// Process stdout as a `FILE *` variable, matching libc's exported `stdout`.
#[no_mangle]
pub static mut elephc_stdout: usize = 0;
/// Process stdin as a `FILE *` variable.
#[no_mangle]
pub static mut elephc_stdin: usize = 0;
/// Process stderr as a `FILE *` variable.
#[no_mangle]
pub static mut elephc_stderr: usize = 0;

/// Returns the address of the process stdout `FILE *` variable.
pub fn stdout_address() -> usize {
    unsafe {
        if elephc_stdout == 0 {
            elephc_stdout = libc::fdopen(1, c"w".as_ptr()) as usize;
        }
        std::ptr::addr_of_mut!(elephc_stdout) as usize
    }
}

/// Returns the address of the process stdin `FILE *` variable.
pub fn stdin_address() -> usize {
    unsafe {
        if elephc_stdin == 0 {
            elephc_stdin = libc::fdopen(0, c"r".as_ptr()) as usize;
        }
        std::ptr::addr_of_mut!(elephc_stdin) as usize
    }
}

/// Returns the address of the process stderr `FILE *` variable.
pub fn stderr_address() -> usize {
    unsafe {
        if elephc_stderr == 0 {
            elephc_stderr = libc::fdopen(2, c"w".as_ptr()) as usize;
        }
        std::ptr::addr_of_mut!(elephc_stderr) as usize
    }
}

/// Returns the same first line PHP's `get_zend_version()` reports.
#[no_mangle]
pub extern "C" fn elephc_ffi_zend_version() -> *const c_char {
    ZEND_VERSION.as_ptr() as *const c_char
}

/// Computes a deterministic 64-bit hash for `zend_hash_func`.
#[no_mangle]
pub extern "C" fn elephc_ffi_zend_hash_func(string: *const c_char, length: usize) -> c_ulong {
    if string.is_null() {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(string as *const u8, length) };
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash as c_ulong
}

/// Lowercases an ASCII C string in place for `zend_str_tolower`.
#[no_mangle]
pub extern "C" fn elephc_ffi_zend_str_tolower(string: *mut c_char, length: usize) {
    if string.is_null() {
        return;
    }
    for index in 0..length {
        let byte = unsafe { *string.add(index) } as u8;
        unsafe {
            *string.add(index) = byte.to_ascii_lowercase() as c_char;
        }
    }
}

/// Default `zend_write` implementation: write the bytes to stdout and report the length.
pub extern "C" fn elephc_ffi_default_zend_write(pointer: *const c_char, len: usize) -> usize {
    if !pointer.is_null() && len > 0 {
        unsafe {
            libc::write(1, pointer as *const libc::c_void, len);
        }
    }
    len
}

/// Initializes the global `zend_printf` variable to libc `printf` and returns its address.
pub fn zend_printf_address() -> usize {
    unsafe {
        zend_printf = libc::printf as *const () as usize;
        std::ptr::addr_of_mut!(zend_printf) as usize
    }
}
