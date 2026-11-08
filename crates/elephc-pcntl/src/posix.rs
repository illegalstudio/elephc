//! Purpose:
//! Owns the ext/posix process, terminal, and identity operations the PCNTL bridge exposes,
//! together with the extension-local errno that `posix_get_last_error()` reports.
//!
//! Called from:
//! - AOT `posix_*` lowering and Magician's PCNTL-area adapters through the stable C ABI.
//!
//! Key details:
//! - ext/posix keeps its own last error, distinct from PCNTL's: a failing `posix_*` call
//!   never changes `pcntl_get_last_error()`, and neither is cleared by a later success.
//! - Status-returning entry points use `i64` so generated code can compare the full register.

use crate::current_errno;
use std::sync::atomic::{AtomicI32, Ordering};

/// The errno recorded by the most recent failing ext/posix operation.
static POSIX_LAST_ERROR: AtomicI32 = AtomicI32::new(0);

/// Status returned by `elephc_posix_kill()` when the process id does not fit a C `pid_t`.
pub const POSIX_KILL_PID_OUT_OF_RANGE: i64 = -1;
/// Status returned by `elephc_posix_isatty()` when the descriptor is outside `0..=INT_MAX`.
pub const POSIX_ISATTY_FD_OUT_OF_RANGE: i64 = -1;

/// PHP 8.5's `ValueError` text for an out-of-range `posix_kill()` process id.
///
/// The lower bound is printed without its sign by PHP itself; the text is reproduced
/// byte-for-byte so AOT, eval and the interpreter agree.
pub const POSIX_KILL_PID_RANGE_MESSAGE: &str =
    "posix_kill(): Argument #1 ($process_id) must be between 2147483648 and 2147483647";
/// PHP 8.5's warning for an out-of-range `posix_isatty()` descriptor.
pub const POSIX_ISATTY_FD_RANGE_WARNING: &str =
    "Warning: posix_isatty(): Argument #1 ($file_descriptor) must be between 0 and 2147483647\n";

/// Records one errno value as the last ext/posix error.
pub(crate) fn record_posix_error(error: libc::c_int) {
    POSIX_LAST_ERROR.store(error, Ordering::Relaxed);
}

/// Records the current thread's OS errno as the last ext/posix error.
pub(crate) fn record_posix_errno() {
    record_posix_error(current_errno());
}

/// Returns the errno recorded by the most recent failing ext/posix operation.
///
/// Backs both `posix_get_last_error()` and its `posix_errno()` alias.
#[no_mangle]
pub extern "C" fn elephc_posix_get_last_error() -> i64 {
    i64::from(POSIX_LAST_ERROR.load(Ordering::Relaxed))
}

/// Sends `signal` to `process_id` through `kill(2)`.
///
/// Returns one on success, zero after recording errno, or
/// `POSIX_KILL_PID_OUT_OF_RANGE` when PHP would raise its process-id `ValueError`. The signal
/// is truncated to a C `int` exactly as PHP does, so an out-of-range signal reaches the kernel.
#[no_mangle]
pub extern "C" fn elephc_posix_kill(process_id: i64, signal: i64) -> i64 {
    let Ok(process_id) = libc::pid_t::try_from(process_id) else {
        return POSIX_KILL_PID_OUT_OF_RANGE;
    };
    if unsafe { libc::kill(process_id, signal as libc::c_int) } == -1 {
        record_posix_errno();
        return 0;
    }
    1
}

/// Reports whether `fd` refers to a terminal through `isatty(3)`.
///
/// Returns one for a terminal and zero after recording errno otherwise. A descriptor outside
/// `0..=INT_MAX` records `EBADF` and returns `POSIX_ISATTY_FD_OUT_OF_RANGE`, for which PHP
/// emits `POSIX_ISATTY_FD_RANGE_WARNING` before returning false.
#[no_mangle]
pub extern "C" fn elephc_posix_isatty(fd: i64) -> i64 {
    let Ok(fd) = libc::c_int::try_from(fd) else {
        record_posix_error(libc::EBADF);
        return POSIX_ISATTY_FD_OUT_OF_RANGE;
    };
    if fd < 0 {
        record_posix_error(libc::EBADF);
        return POSIX_ISATTY_FD_OUT_OF_RANGE;
    }
    if unsafe { libc::isatty(fd) } == 1 {
        return 1;
    }
    record_posix_errno();
    0
}

/// Returns the effective user id of the calling process.
#[no_mangle]
pub extern "C" fn elephc_posix_geteuid() -> i64 {
    i64::from(unsafe { libc::geteuid() })
}

/// Returns the process group id of the calling process.
#[no_mangle]
pub extern "C" fn elephc_posix_getpgrp() -> i64 {
    i64::from(unsafe { libc::getpgrp() })
}

/// Moves one process into a process group and records errno on failure.
#[no_mangle]
pub extern "C" fn elephc_posix_setpgid(process_id: i64, process_group_id: i64) -> libc::c_int {
    let result = unsafe {
        libc::setpgid(
            process_id as libc::pid_t,
            process_group_id as libc::pid_t,
        )
    };
    if result == -1 {
        record_posix_errno();
        return 0;
    }
    1
}

/// Creates a new session for the calling process and returns its session identifier.
#[no_mangle]
pub extern "C" fn elephc_posix_setsid() -> i64 {
    let session_id = unsafe { libc::setsid() };
    if session_id == -1 {
        record_posix_errno();
    }
    i64::from(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{elephc_pcntl_get_last_error, PROCESS_TEST_LOCK};

    /// Records ESRCH for a missing process in the posix state while leaving PCNTL's untouched.
    #[test]
    fn kill_failure_records_posix_errno_only() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
        let pcntl_before = elephc_pcntl_get_last_error();
        assert_eq!(elephc_posix_kill(i64::from(libc::pid_t::MAX), 0), 0);
        assert_eq!(elephc_posix_get_last_error(), i64::from(libc::ESRCH));
        assert_eq!(elephc_pcntl_get_last_error(), pcntl_before);
    }

    /// Accepts signal zero against the current process and keeps the recorded error.
    #[test]
    fn kill_self_with_signal_zero_succeeds_without_clearing_errno() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
        record_posix_error(libc::EPERM);
        let pid = i64::from(unsafe { libc::getpid() });
        assert_eq!(elephc_posix_kill(pid, 0), 1);
        assert_eq!(elephc_posix_get_last_error(), i64::from(libc::EPERM));
    }

    /// Classifies a process id outside `pid_t` as PHP's ValueError case.
    #[test]
    fn kill_rejects_process_ids_outside_pid_t() {
        assert_eq!(elephc_posix_kill(i64::MAX, 0), POSIX_KILL_PID_OUT_OF_RANGE);
        assert_eq!(
            elephc_posix_kill(i64::from(libc::pid_t::MIN) - 1, 0),
            POSIX_KILL_PID_OUT_OF_RANGE
        );
    }

    /// Reports a regular file as no terminal with ENOTTY, and bad descriptors with EBADF.
    #[test]
    fn isatty_records_terminal_probe_errors() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
        let file = std::fs::File::open("/dev/null").expect("open /dev/null");
        let fd = std::os::fd::AsRawFd::as_raw_fd(&file);
        assert_eq!(elephc_posix_isatty(i64::from(fd)), 0);
        assert_eq!(elephc_posix_get_last_error(), i64::from(libc::ENOTTY));
        assert_eq!(elephc_posix_isatty(i64::from(libc::c_int::MAX)), 0);
        assert_eq!(elephc_posix_get_last_error(), i64::from(libc::EBADF));
        record_posix_error(0);
        assert_eq!(elephc_posix_isatty(-1), POSIX_ISATTY_FD_OUT_OF_RANGE);
        assert_eq!(elephc_posix_get_last_error(), i64::from(libc::EBADF));
        assert_eq!(elephc_posix_isatty(i64::MAX), POSIX_ISATTY_FD_OUT_OF_RANGE);
    }

    /// Mirrors the calling process's effective user and process group identifiers.
    #[test]
    fn identity_queries_match_libc() {
        assert_eq!(elephc_posix_geteuid(), i64::from(unsafe { libc::geteuid() }));
        assert_eq!(elephc_posix_getpgrp(), i64::from(unsafe { libc::getpgrp() }));
    }
}
