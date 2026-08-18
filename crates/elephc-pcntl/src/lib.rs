//! Purpose:
//! Exposes the OS-facing PHP PCNTL operations through a stable panic-free C ABI.
//!
//! Called from:
//! - Elephc AOT runtime helpers linked through the `elephc_pcntl` bridge.
//! - Magician adapters and bridge unit tests through the crate's rlib form.
//!
//! Key details:
//! - Failures record one extension-local errno value without clearing it on success.
//! - Process status decoding is delegated to libc so Darwin and Linux layouts stay correct.
//! - PHP values, callable descriptors, signal dispatch, and output arrays remain runtime-owned.

use std::ffi::CStr;
use std::sync::atomic::{AtomicI32, Ordering};

static LAST_ERROR: AtomicI32 = AtomicI32::new(0);

/// Records the current thread's OS errno as the last PCNTL error.
fn record_errno() {
    let error = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    LAST_ERROR.store(error, Ordering::Relaxed);
}

/// Returns a writable pointer to the target C library's thread-local errno.
#[cfg(target_os = "macos")]
unsafe fn errno_location() -> *mut libc::c_int {
    libc::__error()
}

/// Returns a writable pointer to the target C library's thread-local errno.
#[cfg(target_os = "linux")]
unsafe fn errno_location() -> *mut libc::c_int {
    libc::__errno_location()
}

/// Calls Darwin's `getpriority`, whose selector uses `c_int` in libc.
#[cfg(target_os = "macos")]
unsafe fn get_priority(mode: libc::c_int, process_id: libc::id_t) -> libc::c_int {
    libc::getpriority(mode, process_id)
}

/// Calls glibc's `getpriority`, whose selector uses its exported unsigned alias.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
unsafe fn get_priority(mode: libc::c_int, process_id: libc::id_t) -> libc::c_int {
    libc::getpriority(mode as libc::__priority_which_t, process_id)
}

/// Calls musl's `getpriority`, whose selector uses `c_int` in libc.
#[cfg(all(target_os = "linux", not(target_env = "gnu")))]
unsafe fn get_priority(mode: libc::c_int, process_id: libc::id_t) -> libc::c_int {
    libc::getpriority(mode, process_id)
}

/// Calls Darwin's `setpriority`, whose selector uses `c_int` in libc.
#[cfg(target_os = "macos")]
unsafe fn set_priority(
    mode: libc::c_int,
    process_id: libc::id_t,
    priority: libc::c_int,
) -> libc::c_int {
    libc::setpriority(mode, process_id, priority)
}

/// Calls glibc's `setpriority`, whose selector uses its exported unsigned alias.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
unsafe fn set_priority(
    mode: libc::c_int,
    process_id: libc::id_t,
    priority: libc::c_int,
) -> libc::c_int {
    libc::setpriority(mode as libc::__priority_which_t, process_id, priority)
}

/// Calls musl's `setpriority`, whose selector uses `c_int` in libc.
#[cfg(all(target_os = "linux", not(target_env = "gnu")))]
unsafe fn set_priority(
    mode: libc::c_int,
    process_id: libc::id_t,
    priority: libc::c_int,
) -> libc::c_int {
    libc::setpriority(mode, process_id, priority)
}

/// Forks the current process, returning the child PID to the parent and zero to the child.
///
/// A failure returns `-1` and records errno for `pcntl_get_last_error()`.
#[no_mangle]
pub extern "C" fn elephc_pcntl_fork() -> i64 {
    let pid = unsafe { libc::fork() };
    if pid == -1 {
        record_errno();
    }
    i64::from(pid)
}

/// Schedules `SIGALRM` after `seconds` and returns the prior alarm's remaining seconds.
#[no_mangle]
pub extern "C" fn elephc_pcntl_alarm(seconds: i64) -> i64 {
    i64::from(unsafe { libc::alarm(seconds as libc::c_uint) })
}

/// Waits for a matching child and writes its opaque target-native status word.
///
/// A failure returns `-1` and records errno. The caller must provide writable status storage.
///
/// # Safety
/// `status` must be null or point to writable `libc::c_int` storage for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_waitpid(
    process_id: i64,
    status: *mut libc::c_int,
    flags: libc::c_int,
) -> i64 {
    let pid = libc::waitpid(process_id as libc::pid_t, status, flags);
    if pid == -1 {
        record_errno();
    }
    i64::from(pid)
}

/// Reports whether a wait status represents normal child termination.
#[no_mangle]
pub extern "C" fn elephc_pcntl_wifexited(status: libc::c_int) -> libc::c_int {
    libc::WIFEXITED(status) as libc::c_int
}

/// Reports whether a wait status represents a stopped child.
#[no_mangle]
pub extern "C" fn elephc_pcntl_wifstopped(status: libc::c_int) -> libc::c_int {
    libc::WIFSTOPPED(status) as libc::c_int
}

/// Reports whether a wait status represents termination by a signal.
#[no_mangle]
pub extern "C" fn elephc_pcntl_wifsignaled(status: libc::c_int) -> libc::c_int {
    libc::WIFSIGNALED(status) as libc::c_int
}

/// Reports whether a wait status represents a child resumed by `SIGCONT`.
#[no_mangle]
pub extern "C" fn elephc_pcntl_wifcontinued(status: libc::c_int) -> libc::c_int {
    libc::WIFCONTINUED(status) as libc::c_int
}

/// Extracts the exit code from a target-native child wait status.
#[no_mangle]
pub extern "C" fn elephc_pcntl_wexitstatus(status: libc::c_int) -> libc::c_int {
    libc::WEXITSTATUS(status)
}

/// Extracts the terminating signal from a target-native child wait status.
#[no_mangle]
pub extern "C" fn elephc_pcntl_wtermsig(status: libc::c_int) -> libc::c_int {
    libc::WTERMSIG(status)
}

/// Extracts the stopping signal from a target-native child wait status.
#[no_mangle]
pub extern "C" fn elephc_pcntl_wstopsig(status: libc::c_int) -> libc::c_int {
    libc::WSTOPSIG(status)
}

/// Reads the scheduling priority selected by `mode` and `process_id`.
///
/// Returns one on success and writes the priority to `priority`; returns zero and records errno
/// on failure. The split status/output ABI preserves `-1` as a valid priority.
///
/// # Safety
/// `priority` must point to writable `libc::c_int` storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_getpriority(
    process_id: i64,
    mode: libc::c_int,
    priority: *mut libc::c_int,
) -> libc::c_int {
    if priority.is_null() {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return 0;
    }
    *errno_location() = 0;
    let value = get_priority(mode, process_id as libc::id_t);
    if *errno_location() != 0 {
        record_errno();
        return 0;
    }
    *priority = value;
    1
}

/// Changes the scheduling priority selected by `mode` and `process_id`.
///
/// Returns one on success, or zero after recording errno on failure.
#[no_mangle]
pub extern "C" fn elephc_pcntl_setpriority(
    priority: libc::c_int,
    process_id: i64,
    mode: libc::c_int,
) -> libc::c_int {
    let result = unsafe { set_priority(mode, process_id as libc::id_t, priority) };
    if result == -1 {
        record_errno();
        return 0;
    }
    1
}

/// Returns the errno recorded by the most recent failing PCNTL bridge operation.
#[no_mangle]
pub extern "C" fn elephc_pcntl_get_last_error() -> libc::c_int {
    LAST_ERROR.load(Ordering::Relaxed)
}

/// Returns the C library's borrowed error string and writes its byte length.
///
/// The returned pointer remains C-library-owned and must be copied before a later `strerror` call.
///
/// # Safety
/// `length` must be null or point to writable `usize` storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_strerror(
    error: libc::c_int,
    length: *mut usize,
) -> *const u8 {
    let message = libc::strerror(error);
    if message.is_null() {
        if !length.is_null() {
            *length = 0;
        }
        return std::ptr::null();
    }
    let message = CStr::from_ptr(message);
    if !length.is_null() {
        *length = message.to_bytes().len();
    }
    message.as_ptr().cast()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Forks a real child, reaps it, and verifies target-native wait status decoding.
    #[test]
    fn fork_waitpid_and_status_decoding_round_trip() {
        let pid = elephc_pcntl_fork();
        assert!(pid >= 0, "fork failed with errno {}", elephc_pcntl_get_last_error());
        if pid == 0 {
            unsafe { libc::_exit(23) };
        }

        let mut status = 0;
        let waited = unsafe { elephc_pcntl_waitpid(pid, &mut status, 0) };
        assert_eq!(waited, pid);
        assert_eq!(elephc_pcntl_wifexited(status), 1);
        assert_eq!(elephc_pcntl_wifsignaled(status), 0);
        assert_eq!(elephc_pcntl_wifstopped(status), 0);
        assert_eq!(elephc_pcntl_wexitstatus(status), 23);
    }

    /// Reads the current process priority without confusing a valid `-1` with failure.
    #[test]
    fn getpriority_uses_a_separate_success_status() {
        let mut priority = 0;
        let success = unsafe { elephc_pcntl_getpriority(0, libc::PRIO_PROCESS as _, &mut priority) };
        assert_eq!(success, 1, "getpriority failed with errno {}", elephc_pcntl_get_last_error());
        assert!((-20..=20).contains(&priority));
    }

    /// Returns a non-empty C-library message for a known errno value.
    #[test]
    fn strerror_returns_borrowed_bytes_and_length() {
        let mut length = 0;
        let pointer = unsafe { elephc_pcntl_strerror(libc::EINVAL, &mut length) };
        assert!(!pointer.is_null());
        assert!(length > 0);
        let bytes = unsafe { std::slice::from_raw_parts(pointer, length) };
        assert!(!bytes.contains(&0));
    }

    /// Rejects a missing getpriority output pointer with `EFAULT` instead of dereferencing it.
    #[test]
    fn getpriority_rejects_a_null_output_pointer() {
        let success = unsafe { elephc_pcntl_getpriority(0, libc::PRIO_PROCESS as _, std::ptr::null_mut()) };
        assert_eq!(success, 0);
        assert_eq!(elephc_pcntl_get_last_error(), libc::EFAULT);
    }
}
