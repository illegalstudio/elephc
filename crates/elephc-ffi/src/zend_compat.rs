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
use std::sync::atomic::{AtomicUsize, Ordering};

/// Static version string returned by `get_zend_version`.
static ZEND_VERSION: &[u8] = b"Zend Engine v4.5.10, Copyright (c) Zend Technologies\0";

/// Global function-pointer variable populated by `FfiContext::symbol`.
#[no_mangle]
pub static mut zend_printf: usize = 0;

/// Terminal writer registered by generated output code.
///
/// The runtime publishes its built-in terminal path (web response sink or stdout syscall)
/// here before invoking `zend_write`. The default writer calls it so a replacement hook that
/// saved the original writer still reaches the web response sink.
#[no_mangle]
pub static elephc_stdout_terminal: AtomicUsize = AtomicUsize::new(0);

/// Process stdout as a `FILE *` variable, matching libc's exported `stdout`.
#[no_mangle]
pub static mut elephc_stdout: usize = 0;
/// Process stdin as a `FILE *` variable.
#[no_mangle]
pub static mut elephc_stdin: usize = 0;
/// Process stderr as a `FILE *` variable.
#[no_mangle]
pub static mut elephc_stderr: usize = 0;

/// Resolves one C library standard stream, falling back to a fresh `fdopen` stream.
///
/// `dlsym` returns the address of the library's `FILE *` variable on Linux and of its
/// `__stdoutp`-style variable on Apple targets; the stored slot must hold the `FILE *` itself
/// so the prelude's `extern FILE *stdout` reads the same stream php-src uses. `fdopen` would
/// create a second buffer over the same descriptor and reorder C output against PHP's writes.
fn resolve_standard_stream(names: &[&std::ffi::CStr], fd: i32, mode: &std::ffi::CStr, slot: *mut usize) {
    if slot.is_null() {
        return;
    }
    unsafe {
        if *slot != 0 {
            return;
        }
        for name in names {
            let pointer = libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr());
            if pointer.is_null() {
                continue;
            }
            let stream = *(pointer as *const usize);
            if stream != 0 {
                *slot = stream;
                return;
            }
        }
        *slot = libc::fdopen(fd, mode.as_ptr()) as usize;
    }
}

/// Returns the address of the process stdout `FILE *` variable.
pub fn stdout_address() -> usize {
    resolve_standard_stream(&[c"stdout", c"__stdoutp"], 1, c"w", std::ptr::addr_of_mut!(elephc_stdout));
    std::ptr::addr_of_mut!(elephc_stdout) as usize
}

/// Returns the address of the process stdin `FILE *` variable.
pub fn stdin_address() -> usize {
    resolve_standard_stream(&[c"stdin", c"__stdinp"], 0, c"r", std::ptr::addr_of_mut!(elephc_stdin));
    std::ptr::addr_of_mut!(elephc_stdin) as usize
}

/// Returns the address of the process stderr `FILE *` variable.
pub fn stderr_address() -> usize {
    resolve_standard_stream(&[c"stderr", c"__stderrp"], 2, c"w", std::ptr::addr_of_mut!(elephc_stderr));
    std::ptr::addr_of_mut!(elephc_stderr) as usize
}

/// Returns the same first line PHP's `get_zend_version()` reports.
#[no_mangle]
pub extern "C" fn elephc_ffi_zend_version() -> *const c_char {
    ZEND_VERSION.as_ptr() as *const c_char
}

/// Computes Zend's DJBX33A hash for `zend_hash_func`.
#[no_mangle]
pub extern "C" fn elephc_ffi_zend_hash_func(string: *const c_char, length: usize) -> c_ulong {
    if string.is_null() {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(string as *const u8, length) };
    // php-src's `zend_inline_hash_func`: 5381, then `hash * 33 + byte`. The corpus fixtures
    // compare this through the test-only Zend shim, so a foreign hash would diverge there.
    let mut hash = 5381u64;
    for byte in bytes {
        hash = hash.wrapping_mul(33).wrapping_add(u64::from(*byte));
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

/// Default `zend_write` implementation: use the registered terminal sink, or stdout.
pub extern "C" fn elephc_ffi_default_zend_write(pointer: *const c_char, len: usize) -> usize {
    let terminal = elephc_stdout_terminal.load(Ordering::Relaxed);
    if terminal != 0 {
        let writer: unsafe extern "C" fn(*const c_char, usize) =
            unsafe { std::mem::transmute(terminal) };
        unsafe { writer(pointer, len) };
        return len;
    }
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

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the Zend compatibility shims compiled into `php-src-fixtures` builds.
    //!
    //! Called from:
    //! - `cargo test -p elephc-ffi`.
    //!
    //! Key details:
    //! - The hash pins php-src's DJBX33A so the test-only Zend fixture surface cannot drift.

    use super::*;

    use std::sync::Mutex;

    /// Serializes tests that mutate the process-wide terminal sink.
    static TERMINAL_LOCK: Mutex<()> = Mutex::new(());
    /// Bytes recorded by the test terminal sink.
    static CAPTURED_LEN: AtomicUsize = AtomicUsize::new(0);
    /// Pointer recorded by the test terminal sink.
    static CAPTURED_PTR: AtomicUsize = AtomicUsize::new(0);

    /// Test sink that records the bytes handed to the default writer.
    unsafe extern "C" fn capture_terminal(pointer: *const c_char, len: usize) {
        CAPTURED_LEN.store(len, Ordering::SeqCst);
        CAPTURED_PTR.store(pointer as usize, Ordering::SeqCst);
    }

    /// Verifies the default `zend_write` forwards to the registered terminal sink.
    #[test]
    fn default_zend_write_uses_the_registered_terminal_sink() {
        let _guard = TERMINAL_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let message = b"web-response";
        CAPTURED_LEN.store(0, Ordering::SeqCst);
        CAPTURED_PTR.store(0, Ordering::SeqCst);
        elephc_stdout_terminal.store(capture_terminal as usize, Ordering::SeqCst);
        let written =
            elephc_ffi_default_zend_write(message.as_ptr() as *const c_char, message.len());
        elephc_stdout_terminal.store(0, Ordering::SeqCst);
        assert_eq!(written, message.len());
        assert_eq!(CAPTURED_LEN.load(Ordering::SeqCst), message.len());
        assert_eq!(CAPTURED_PTR.load(Ordering::SeqCst), message.as_ptr() as usize);
    }

    /// `zend_hash_func` returns Zend's DJBX33A value for a byte string.
    #[test]
    fn zend_hash_func_uses_djbx33a() {
        let mut expected = 5381u64;
        for byte in b"file" {
            expected = expected.wrapping_mul(33).wrapping_add(u64::from(*byte));
        }
        assert_eq!(
            elephc_ffi_zend_hash_func(c"file".as_ptr(), 4),
            expected as c_ulong
        );
    }
}
