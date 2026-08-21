//! Purpose:
//! Owns native string staging and process-image replacement for `pcntl_exec()`.
//!
//! Called from:
//! - AOT and Magician PCNTL adapters through the bridge's stable C ABI.
//!
//! Key details:
//! - Every PHP string is copied before calling libc.
//! - A successful exec never returns; failures consume the builder and preserve errno.

use crate::{record_errno, LAST_ERROR};
use std::ffi::CString;
use std::sync::atomic::Ordering;

/// Owned native strings staged for one `pcntl_exec()` attempt.
struct ExecBuilder {
    path: CString,
    args: Vec<CString>,
    env: Option<Vec<CString>>,
}

/// Copies one borrowed byte range into a null-terminated native string.
///
/// # Safety
/// `ptr` must be readable for `len` bytes when `len` is nonzero.
unsafe fn copy_c_string(ptr: *const u8, len: usize) -> Option<CString> {
    if ptr.is_null() && len != 0 {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return None;
    }
    let bytes = if len == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(ptr, len)
    };
    match CString::new(bytes) {
        Ok(value) => Some(value),
        Err(_) => {
            LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
            None
        }
    }
}

/// Releases an opaque execution builder previously returned by `elephc_pcntl_exec_new()`.
///
/// # Safety
/// `builder` must be null or a live builder pointer returned by this crate, and must not be
/// released or executed again afterward.
unsafe fn drop_exec_builder(builder: *mut libc::c_void) {
    if !builder.is_null() {
        drop(Box::from_raw(builder.cast::<ExecBuilder>()));
    }
}

/// Allocates an opaque builder for one `pcntl_exec()` attempt and copies the executable path.
///
/// `has_environment` distinguishes an omitted environment from an explicitly empty one: the
/// former inherits the current process environment through `execv()`, while the latter calls
/// `execve()` with an empty environment just like PHP.
///
/// # Safety
/// `path` must be readable for `path_len` bytes when `path_len` is nonzero.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_exec_new(
    path: *const u8,
    path_len: usize,
    has_environment: libc::c_int,
) -> *mut libc::c_void {
    let Some(path) = copy_c_string(path, path_len) else {
        return std::ptr::null_mut();
    };
    let builder = ExecBuilder {
        path,
        args: Vec::new(),
        env: (has_environment != 0).then(Vec::new),
    };
    Box::into_raw(Box::new(builder)).cast::<libc::c_void>()
}

/// Copies one already string-coerced PHP argument into an opaque execution builder.
///
/// Returns one on success or zero after recording `EINVAL`/`EFAULT`.
///
/// # Safety
/// `builder` must be a live pointer returned by `elephc_pcntl_exec_new()`, and `value` must be
/// readable for `value_len` bytes when the length is nonzero.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_exec_add_arg(
    builder: *mut libc::c_void,
    value: *const u8,
    value_len: usize,
) -> libc::c_int {
    let Some(builder) = builder.cast::<ExecBuilder>().as_mut() else {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return 0;
    };
    let Some(value) = copy_c_string(value, value_len) else {
        return 0;
    };
    builder.args.push(value);
    1
}

/// Copies one PHP environment entry into an opaque execution builder.
///
/// String keys use `key_low` as a pointer and nonnegative `key_high` as their byte length.
/// Integer keys use their signed bits in `key_low` with `key_high == -1`, matching PHP's decimal
/// string conversion before constructing `key=value`.
///
/// # Safety
/// `builder` must be a live pointer returned by `elephc_pcntl_exec_new()`. String-key and value
/// pointers must be readable for their corresponding nonzero lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_exec_add_env(
    builder: *mut libc::c_void,
    key_low: u64,
    key_high: i64,
    value: *const u8,
    value_len: usize,
) -> libc::c_int {
    let Some(builder) = builder.cast::<ExecBuilder>().as_mut() else {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return 0;
    };
    let Some(environment) = builder.env.as_mut() else {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return 0;
    };
    let key = if key_high == -1 {
        (key_low as i64).to_string().into_bytes()
    } else {
        let Some(key) = copy_c_string(key_low as *const u8, key_high as usize) else {
            return 0;
        };
        key.into_bytes()
    };
    let Some(value) = copy_c_string(value, value_len) else {
        return 0;
    };
    let mut pair = Vec::with_capacity(key.len() + value.as_bytes().len() + 1);
    pair.extend_from_slice(&key);
    pair.push(b'=');
    pair.extend_from_slice(value.as_bytes());
    let Some(pair) = CString::new(pair).ok() else {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return 0;
    };
    environment.push(pair);
    1
}

/// Executes and consumes one fully staged `pcntl_exec()` builder.
///
/// A successful call never returns. Failure records `errno`, releases all staged strings, and
/// returns zero, which is the only PHP-visible return path.
///
/// # Safety
/// `builder` must be a live pointer returned by `elephc_pcntl_exec_new()` and must not be used
/// again after this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_exec_run(builder: *mut libc::c_void) -> libc::c_int {
    if builder.is_null() {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return 0;
    }
    let builder = Box::from_raw(builder.cast::<ExecBuilder>());
    let mut argv = Vec::with_capacity(builder.args.len() + 2);
    argv.push(builder.path.as_ptr());
    argv.extend(builder.args.iter().map(|argument| argument.as_ptr()));
    argv.push(std::ptr::null());
    match &builder.env {
        Some(environment) => {
            let mut envp = Vec::with_capacity(environment.len() + 1);
            envp.extend(environment.iter().map(|entry| entry.as_ptr()));
            envp.push(std::ptr::null());
            libc::execve(builder.path.as_ptr(), argv.as_ptr(), envp.as_ptr());
        }
        None => {
            libc::execv(builder.path.as_ptr(), argv.as_ptr());
        }
    }
    record_errno();
    0
}

/// Cancels and consumes one staged `pcntl_exec()` builder after a conversion failure.
///
/// # Safety
/// `builder` must be null or a live pointer returned by `elephc_pcntl_exec_new()` and must not be
/// used again after this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_exec_free(builder: *mut libc::c_void) {
    drop_exec_builder(builder);
}
