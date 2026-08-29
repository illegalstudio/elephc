//! Purpose:
//! Owns process creation, child waiting, stable resource usage, and wait-status decoding.
//!
//! Called from:
//! - AOT and Magician PCNTL adapters through the bridge's stable C ABI.
//!
//! Key details:
//! - The child resets inherited signal-queue descriptors before signals are unmasked.
//! - Failed waits preserve caller-owned output; resource usage is copied only for a reaped child.

use crate::{
    current_errno, record_errno, reset_signal_pipe_after_fork, signal_queue_initialized,
    ElephcPcntlSigInfo, LAST_ERROR, SIGINFO_CODE, SIGINFO_ERRNO, SIGINFO_PID, SIGINFO_SIGNO,
    SIGINFO_STATUS, SIGINFO_UID,
};
#[cfg(target_os = "linux")]
use crate::{SIGINFO_STIME, SIGINFO_UTIME};
use std::sync::atomic::Ordering;

/// Stable, target-neutral copy of the PHP `getrusage` fields returned by wait operations.
///
/// The layout is part of the C ABI shared with generated AOT code. Every field is widened to
/// `i64` so Darwin and Linux expose the same 17-word block even where libc uses narrower aliases.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ElephcPcntlRUsage {
    pub ru_oublock: i64,
    pub ru_inblock: i64,
    pub ru_msgsnd: i64,
    pub ru_msgrcv: i64,
    pub ru_maxrss: i64,
    pub ru_ixrss: i64,
    pub ru_idrss: i64,
    pub ru_minflt: i64,
    pub ru_majflt: i64,
    pub ru_nsignals: i64,
    pub ru_nvcsw: i64,
    pub ru_nivcsw: i64,
    pub ru_nswap: i64,
    pub ru_utime_tv_usec: i64,
    pub ru_utime_tv_sec: i64,
    pub ru_stime_tv_usec: i64,
    pub ru_stime_tv_sec: i64,
}

impl From<libc::rusage> for ElephcPcntlRUsage {
    /// Copies one target-native `rusage` value into the stable bridge layout.
    fn from(usage: libc::rusage) -> Self {
        Self {
            ru_oublock: usage.ru_oublock,
            ru_inblock: usage.ru_inblock,
            ru_msgsnd: usage.ru_msgsnd,
            ru_msgrcv: usage.ru_msgrcv,
            ru_maxrss: usage.ru_maxrss,
            ru_ixrss: usage.ru_ixrss,
            ru_idrss: usage.ru_idrss,
            ru_minflt: usage.ru_minflt,
            ru_majflt: usage.ru_majflt,
            ru_nsignals: usage.ru_nsignals,
            ru_nvcsw: usage.ru_nvcsw,
            ru_nivcsw: usage.ru_nivcsw,
            ru_nswap: usage.ru_nswap,
            ru_utime_tv_usec: usage.ru_utime.tv_usec,
            ru_utime_tv_sec: usage.ru_utime.tv_sec,
            ru_stime_tv_usec: usage.ru_stime.tv_usec,
            ru_stime_tv_sec: usage.ru_stime.tv_sec,
        }
    }
}

/// Copies child-state signal information into the stable PCNTL record.
unsafe fn copy_child_siginfo(info: &libc::siginfo_t) -> ElephcPcntlSigInfo {
    let stable = ElephcPcntlSigInfo {
        signo: i64::from(info.si_signo),
        error: i64::from(info.si_errno),
        code: i64::from(info.si_code),
        status: i64::from(info.si_status()),
        pid: i64::from(info.si_pid()),
        uid: i64::from(info.si_uid()),
        present: SIGINFO_SIGNO
            | SIGINFO_ERRNO
            | SIGINFO_CODE
            | SIGINFO_STATUS
            | SIGINFO_PID
            | SIGINFO_UID,
        ..ElephcPcntlSigInfo::default()
    };
    #[cfg(target_os = "linux")]
    {
        let mut stable = stable;
        stable.utime = info.si_utime();
        stable.stime = info.si_stime();
        stable.present |= SIGINFO_UTIME | SIGINFO_STIME;
        stable
    }
    #[cfg(target_os = "macos")]
    {
        stable
    }
}

/// Forks the current process, returning the child PID to the parent and zero to the child.
///
/// A failure returns `-1` and records errno for `pcntl_get_last_error()`.
#[no_mangle]
pub extern "C" fn elephc_pcntl_fork() -> i64 {
    let mut previous_mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let mut full_mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let mask_was_blocked = signal_queue_initialized()
        && unsafe {
            libc::sigfillset(&mut full_mask) == 0
                && libc::sigprocmask(libc::SIG_SETMASK, &full_mask, &mut previous_mask) == 0
        };
    let pid = unsafe { libc::fork() };
    let fork_error = current_errno();
    if pid == 0 {
        reset_signal_pipe_after_fork();
    }
    if mask_was_blocked {
        unsafe {
            libc::sigprocmask(libc::SIG_SETMASK, &previous_mask, std::ptr::null_mut());
        }
    }
    if pid == -1 {
        LAST_ERROR.store(fork_error, Ordering::Relaxed);
    }
    i64::from(pid)
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

/// Waits for any child and writes its opaque target-native status word.
///
/// A failure returns `-1` and records errno. The caller must provide writable status storage.
///
/// # Safety
/// `status` must be null or point to writable `libc::c_int` storage for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_wait(
    status: *mut libc::c_int,
    flags: libc::c_int,
) -> i64 {
    elephc_pcntl_waitpid(-1, status, flags)
}

/// Waits for a matching child while collecting its stable PHP resource-usage fields.
///
/// A failure returns `-1` and records errno. The usage structure is copied only when a child is
/// actually reaped, so a nohang result of zero leaves caller-owned storage untouched.
///
/// # Safety
/// `status` must be null or writable as a `libc::c_int`; `usage` must be null or writable as an
/// `ElephcPcntlRUsage`. Both pointers must remain valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_wait4(
    process_id: i64,
    status: *mut libc::c_int,
    flags: libc::c_int,
    usage: *mut ElephcPcntlRUsage,
) -> i64 {
    let mut native_usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let native_usage_ptr = if usage.is_null() {
        std::ptr::null_mut()
    } else {
        native_usage.as_mut_ptr()
    };
    let pid = libc::wait4(
        process_id as libc::pid_t,
        status,
        flags,
        native_usage_ptr,
    );
    if pid == -1 {
        record_errno();
    } else if pid > 0 && !usage.is_null() {
        *usage = ElephcPcntlRUsage::from(native_usage.assume_init());
    }
    i64::from(pid)
}

/// Waits for a child state change and writes PHP's stable signal-information record.
///
/// Returns one on success or zero after recording errno on failure.
///
/// # Safety
/// `info` must be null or point to writable `ElephcPcntlSigInfo` storage for the duration of
/// the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_waitid(
    id_type: libc::c_int,
    id: i64,
    info: *mut ElephcPcntlSigInfo,
    flags: libc::c_int,
) -> libc::c_int {
    let mut native_info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let result = libc::waitid(
        id_type as libc::idtype_t,
        id as libc::id_t,
        native_info.as_mut_ptr(),
        flags,
    );
    if result == -1 {
        record_errno();
        return 0;
    }
    if !info.is_null() {
        *info = copy_child_siginfo(&native_info.assume_init());
    }
    1
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
