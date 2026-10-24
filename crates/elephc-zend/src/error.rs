//! Purpose:
//! Errors, warnings, output and the bailout: how a diagnostic an extension
//! raises reaches the user, and how a fatal one ends the call.
//!
//! Called from:
//! - `src/c/variadic.c`, which formats `zend_error(...)`/`php_error_docref(...)`
//!   and hands the message to `elephc_zend_report*`.
//! - Engine modules reporting their own diagnostics (`warning`, `fatal`).
//!
//! Key details:
//! - Diagnostics read like Elephc's own: `Warning: message` on stderr, silenced
//!   by `error_reporting` and by the program's `@` operator.
//! - A fatal error longjmps to the bailout frame of the running call (see
//!   `crate::host`), which reports it to the generated wrapper; outside any
//!   frame the process exits with status 255, as PHP does.
//! - Output an extension writes goes through Elephc's own stdout writer, so it
//!   interleaves with `echo` and is captured by `ob_*` and `--web`.

use std::ffi::{c_char, c_int};
use std::io::Write;

use crate::abi::*;
use crate::startup::eg;

/// The label PHP prints for an error level: "Fatal error", "Recoverable fatal error", "Warning",
/// "Notice", "Deprecated", "Parse error" or "Unknown error".
fn label(r#type: c_int) -> &'static str {
    match r#type {
        E_ERROR | E_CORE_ERROR | E_COMPILE_ERROR | E_USER_ERROR => "Fatal error",
        E_RECOVERABLE_ERROR => "Recoverable fatal error",
        E_WARNING | E_CORE_WARNING | E_COMPILE_WARNING | E_USER_WARNING => "Warning",
        E_NOTICE | E_USER_NOTICE => "Notice",
        E_DEPRECATED | E_USER_DEPRECATED => "Deprecated",
        E_PARSE => "Parse error",
        _ => "Unknown error",
    }
}

/// Whether an error level belongs to the fatal group (`E_FATAL_ERRORS`) that ends the program.
fn is_fatal(r#type: c_int) -> bool {
    r#type & E_FATAL_ERRORS != 0
}

/// Reports a diagnostic of level `type`; a fatal one does not return.
pub fn report(r#type: c_int, message: &[u8]) {
    let fatal = is_fatal(r#type);
    unsafe {
        if !fatal && ((eg().error_reporting & r#type) == 0 || elephc_zend_diag_suppressed() != 0) {
            return;
        }
        let mut line = Vec::with_capacity(message.len() + 16);
        line.extend_from_slice(label(r#type).as_bytes());
        line.extend_from_slice(b": ");
        line.extend_from_slice(message);
        line.push(b'\n');
        let _ = std::io::stderr().write_all(&line);
        if fatal {
            eg().exit_status = 255;
            drop(line);
            bailout();
        }
    }
}

/// Jumps back to the running call's bailout frame, or exits.
pub fn bailout() -> ! {
    unsafe {
        let target = eg().bailout;
        if target.is_null() {
            let _ = std::io::stderr().flush();
            std::process::exit(255);
        }
        elephc_zend_longjmp(target)
    }
}

/// Reports an `E_WARNING` diagnostic with the given message.
pub fn warning(message: &str) {
    report(E_WARNING, message.as_bytes());
}

/// Reports an `E_DEPRECATED` diagnostic with the given message.
pub fn deprecated(message: &str) {
    report(E_DEPRECATED, message.as_bytes());
}

/// Reports an `E_ERROR` fatal diagnostic and bails out; never returns.
pub fn fatal(message: &str) -> ! {
    report(E_ERROR, message.as_bytes());
    bailout()
}

/// The running internal function's name, as diagnostics prefix it.
pub unsafe fn active_function_prefix() -> Option<String> {
    let name = crate::api::get_active_function_or_method_name();
    if name.is_null() {
        return None;
    }
    let text = crate::string::text(name);
    crate::string::zend_string_release(name);
    if text == "main" {
        None
    } else {
        Some(text)
    }
}

/// `zend_error()` after formatting.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_report(r#type: c_int, message: *const c_char, len: usize) {
    report(r#type, std::slice::from_raw_parts(message.cast(), len));
}

/// `php_verror()` after formatting: the active function and the docref parameters prefix it, as
/// PHP prints `func(params): message` (`params` may be NULL, read as empty).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_report_docref(r#type: c_int, params: *const c_char, message: *const c_char, len: usize) {
    let message = std::slice::from_raw_parts(message.cast::<u8>(), len);
    let params: &[u8] = if params.is_null() { b"" } else { std::ffi::CStr::from_ptr(params).to_bytes() };
    match active_function_prefix() {
        Some(function) => {
            let mut text = function.into_bytes();
            text.push(b'(');
            text.extend_from_slice(params);
            text.extend_from_slice(b"): ");
            text.extend_from_slice(message);
            report(r#type, &text);
        }
        None => report(r#type, message),
    }
}

/// `zend_error_zstr`: reports a diagnostic of level `type` whose message is a `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn zend_error_zstr(r#type: c_int, message: *mut zend_string) {
    report(r#type, crate::string::bytes(message));
}

/// `zend_error_zstr_at`: like `zend_error_zstr`; the file name and line number are ignored.
#[no_mangle]
pub unsafe extern "C" fn zend_error_zstr_at(r#type: c_int, _filename: *mut zend_string, _lineno: u32, message: *mut zend_string) {
    report(r#type, crate::string::bytes(message));
}

/// `_zend_bailout`: aborts the current request after a fatal error, as `zend_bailout()` does; never
/// returns.
#[no_mangle]
pub extern "C" fn _zend_bailout(_filename: *const c_char, _lineno: u32) -> ! {
    bailout()
}

/// Stops the program: the extension reached something a compiled program does
/// not provide (calling back into PHP code, PHP streams, output buffers).
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_unsupported(what: *const c_char) -> ! {
    let what = std::ffi::CStr::from_ptr(what).to_string_lossy();
    let _ = writeln!(
        std::io::stderr(),
        "Fatal error: hosted PHP extension reached '{what}', which an ahead-of-time Elephc binary does not provide"
    );
    std::process::abort();
}

/// Rust-side entry to `elephc_zend_unsupported`: stops the program, naming the unsupported feature;
/// never returns.
pub fn unsupported(what: &str) -> ! {
    let what = std::ffi::CString::new(what).unwrap_or_default();
    unsafe { elephc_zend_unsupported(what.as_ptr()) }
}

/// `zend_replace_error_handling`: installs an error-handling mode and exception class, saving the
/// previous pair in `current` (when non-NULL) for `zend_restore_error_handling`.
#[no_mangle]
pub unsafe extern "C" fn zend_replace_error_handling(
    error_handling: zend_error_handling_t,
    exception_class: *mut zend_class_entry,
    current: *mut zend_error_handling,
) {
    if !current.is_null() {
        (*current).handling = eg().error_handling;
        (*current).exception = eg().exception_class;
    }
    eg().error_handling = error_handling;
    eg().exception_class = exception_class;
}

/// `zend_restore_error_handling`: reinstalls the error-handling mode and exception class saved by
/// `zend_replace_error_handling`.
#[no_mangle]
pub unsafe extern "C" fn zend_restore_error_handling(saved: *mut zend_error_handling) {
    eg().error_handling = (*saved).handling;
    eg().exception_class = (*saved).exception;
}

/// `zend_begin_record_errors`: no-op, because errors are never deferred in this engine.
#[no_mangle]
pub extern "C" fn zend_begin_record_errors() {}

/// `zend_emit_recorded_errors`: no-op, because no errors are recorded.
#[no_mangle]
pub extern "C" fn zend_emit_recorded_errors() {}

/// `zend_free_recorded_errors`: no-op, because no errors are recorded.
#[no_mangle]
pub extern "C" fn zend_free_recorded_errors() {}

// ------------------------------------------------------------ output

/// `zend_write`: writes `len` bytes of `str` to the program's standard output and returns `len`.
#[no_mangle]
pub unsafe extern "C" fn zend_write(str: *const c_char, len: usize) -> usize {
    if len > 0 {
        elephc_zend_stdout_write(str, len);
    }
    len
}

/// `php_write`: writes a buffer to standard output; same as `zend_write`.
#[no_mangle]
pub unsafe extern "C" fn php_write(buf: *mut std::ffi::c_void, size: usize) -> usize {
    zend_write(buf.cast(), size)
}

/// `php_output_write`: writes bytes to standard output; there are no output buffers, so this is
/// `zend_write`.
#[no_mangle]
pub unsafe extern "C" fn php_output_write(str: *const c_char, len: usize) -> usize {
    zend_write(str, len)
}

/// `php_output_write_unbuffered`: writes bytes to standard output; same as `zend_write`.
#[no_mangle]
pub unsafe extern "C" fn php_output_write_unbuffered(str: *const c_char, len: usize) -> usize {
    zend_write(str, len)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each level prints PHP's own label; E_RECOVERABLE_ERROR has a label of its own.
    #[test]
    fn levels_print_php_labels() {
        assert_eq!(label(E_RECOVERABLE_ERROR), "Recoverable fatal error");
        assert_eq!(label(E_USER_ERROR), "Fatal error");
        assert_eq!(label(E_CORE_WARNING), "Warning");
    }
}
