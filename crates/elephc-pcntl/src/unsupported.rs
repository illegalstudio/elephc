//! Purpose:
//! Provides the stable PCNTL bridge ABI on targets without Unix process and signal primitives.
//!
//! Called from:
//! - `crate` when the target is not Unix.
//! - Generated PCNTL call sites and Magician's PCNTL adapters through the bridge ABI.
//!
//! Key details:
//! - Every OS operation records `ENOSYS` and returns its documented failure sentinel.
//! - The target-neutral records and constant presence bits retain their Unix ABI layout.
//! - These stubs deliberately never emulate process creation, signal delivery, or profiling.

use crate::{LAST_ERROR, PCNTL_WARNING_CPU_AFFINITY};
use std::sync::atomic::Ordering;

/// Opaque storage retained for ABI-compatible signal-dispatch save slots.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct ElephcPcntlSignalMask {
    storage: [u8; 128],
}

impl Default for ElephcPcntlSignalMask {
    /// Returns zeroed ABI storage without relying on array-length-specific trait impls.
    fn default() -> Self {
        Self { storage: [0; 128] }
    }
}

/// Stable signal information shared by the AOT and eval PCNTL surfaces.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ElephcPcntlSigInfo {
    pub signo: i64,
    pub error: i64,
    pub code: i64,
    pub status: i64,
    pub pid: i64,
    pub uid: i64,
    pub utime: i64,
    pub stime: i64,
    pub address: i64,
    pub band: i64,
    pub fd: i64,
    pub present: u64,
}

/// Stable resource-usage record retained even when child waiting is unavailable.
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

/// Stable backend identifiers accepted by the Unix implementation.
pub const PCNTL_SIGNAL_OWNER_AOT: libc::c_int = 1;
/// Stable backend identifiers accepted by the Unix implementation.
pub const PCNTL_SIGNAL_OWNER_EVAL: libc::c_int = 2;
/// Presence bit for `ElephcPcntlSigInfo::signo`.
pub const SIGINFO_SIGNO: u64 = 1 << 0;
/// Presence bit for `ElephcPcntlSigInfo::error`.
pub const SIGINFO_ERRNO: u64 = 1 << 1;
/// Presence bit for `ElephcPcntlSigInfo::code`.
pub const SIGINFO_CODE: u64 = 1 << 2;
/// Presence bit for `ElephcPcntlSigInfo::status`.
pub const SIGINFO_STATUS: u64 = 1 << 3;
/// Presence bit for `ElephcPcntlSigInfo::pid`.
pub const SIGINFO_PID: u64 = 1 << 4;
/// Presence bit for `ElephcPcntlSigInfo::uid`.
pub const SIGINFO_UID: u64 = 1 << 5;
/// Presence bit for `ElephcPcntlSigInfo::utime`.
pub const SIGINFO_UTIME: u64 = 1 << 6;
/// Presence bit for `ElephcPcntlSigInfo::stime`.
pub const SIGINFO_STIME: u64 = 1 << 7;
/// Presence bit for `ElephcPcntlSigInfo::address`.
pub const SIGINFO_ADDRESS: u64 = 1 << 8;
/// Presence bit for `ElephcPcntlSigInfo::band`.
pub const SIGINFO_BAND: u64 = 1 << 9;
/// Presence bit for `ElephcPcntlSigInfo::fd`.
pub const SIGINFO_FD: u64 = 1 << 10;

/// Exec-input result retained for callers that inspect conversion failures.
pub const PCNTL_EXEC_INPUT_OK: libc::c_int = 0;
/// Exec-input result retained for callers that inspect conversion failures.
pub const PCNTL_EXEC_INPUT_PATH_NUL: libc::c_int = 1;
/// Exec-input result retained for callers that inspect conversion failures.
pub const PCNTL_EXEC_INPUT_ARG_NUL: libc::c_int = 2;
/// Exec-input result retained for callers that inspect conversion failures.
pub const PCNTL_EXEC_INPUT_ENV_NAME_NUL: libc::c_int = 3;
/// Exec-input result retained for callers that inspect conversion failures.
pub const PCNTL_EXEC_INPUT_ENV_VALUE_NUL: libc::c_int = 4;

/// Records a real, target-level lack of implementation rather than claiming success.
fn unavailable() {
    LAST_ERROR.store(libc::ENOSYS, Ordering::Relaxed);
}

macro_rules! unavailable_zero {
    ($(#[$meta:meta])* fn $name:ident($($arg:ident: $type:ty),* $(,)?) -> $return:ty) => {
        $(#[$meta])*
        #[no_mangle]
        pub extern "C" fn $name($($arg: $type),*) -> $return {
            let _ = ($($arg),*);
            unavailable();
            0
        }
    };
}

unavailable_zero!(fn elephc_pcntl_getpriority(process_id: i64, mode: libc::c_int, priority: *mut libc::c_int) -> libc::c_int);
unavailable_zero!(fn elephc_pcntl_setpriority(priority: libc::c_int, process_id: i64, mode: libc::c_int) -> libc::c_int);
unavailable_zero!(fn elephc_posix_setpgid(process_id: i64, process_group_id: i64) -> libc::c_int);
unavailable_zero!(fn elephc_pcntl_daemon(no_chdir: libc::c_int, no_close: libc::c_int) -> libc::c_int);
unavailable_zero!(fn elephc_pcntl_signal(signal: libc::c_int, disposition: libc::c_int, restart_syscalls: libc::c_int, owner: libc::c_int) -> libc::c_int);
unavailable_zero!(fn elephc_pcntl_dispatch_begin(previous_mask: *mut ElephcPcntlSignalMask) -> libc::c_int);
unavailable_zero!(fn elephc_pcntl_dispatch_end(previous_mask: *const ElephcPcntlSignalMask) -> libc::c_int);
unavailable_zero!(fn elephc_pcntl_getqos_class() -> libc::c_int);
unavailable_zero!(fn elephc_pcntl_setqos_class(name: *const u8, name_len: usize) -> libc::c_int);

/// Reports an unavailable `fork(2)` with its documented `-1` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pcntl_fork() -> i64 {
    unavailable();
    -1
}

/// Reports an unavailable `setsid(2)` with its documented `-1` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_posix_setsid() -> i64 {
    unavailable();
    -1
}

/// Returns no pending alarm because Windows does not implement Unix alarm signals.
#[no_mangle]
pub extern "C" fn elephc_pcntl_alarm(_seconds: i64) -> i64 {
    unavailable();
    -1
}

/// Makes signal APIs fail before a caller attempts to construct a Unix signal set.
#[no_mangle]
pub extern "C" fn elephc_pcntl_signal_limit() -> libc::c_int {
    unavailable();
    0
}

/// Reports no installed Unix signal owner on this target.
#[no_mangle]
pub extern "C" fn elephc_pcntl_signal_owner(_signal: libc::c_int) -> libc::c_int {
    unavailable();
    0
}

/// Rejects signal-set validation because the subsequent operation is unavailable.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_validate_signal_set(
    _signals: *const i64,
    _count: usize,
    _allow_empty: libc::c_int,
) -> libc::c_int {
    unavailable();
    0
}

/// Reports that no queued Unix signal can be delivered.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_signal_next(
    _info: *mut ElephcPcntlSigInfo,
    _owner: libc::c_int,
) -> libc::c_int {
    unavailable();
    -1
}

/// Reports that Unix signal masks are unavailable.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_sigprocmask(
    _how: libc::c_int,
    _signals: *const i64,
    _count: usize,
    _old_signals: *mut i64,
    _old_capacity: usize,
) -> i64 {
    unavailable();
    -1
}

/// Reports that synchronous Unix signal waits are unavailable.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_sigwaitinfo(
    _signals: *const i64,
    _count: usize,
    _info: *mut ElephcPcntlSigInfo,
) -> i64 {
    unavailable();
    -1
}

/// Reports that timed Unix signal waits are unavailable.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_sigtimedwait(
    _signals: *const i64,
    _count: usize,
    _info: *mut ElephcPcntlSigInfo,
    _seconds: i64,
    _nanoseconds: i64,
) -> i64 {
    unavailable();
    -1
}

/// Reports an unavailable child wait with its documented `-1` sentinel.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_waitpid(
    _process_id: i64,
    _status: *mut libc::c_int,
    _flags: libc::c_int,
) -> i64 {
    unavailable();
    -1
}

/// Reports an unavailable child wait with its documented `-1` sentinel.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_wait(
    _status: *mut libc::c_int,
    _flags: libc::c_int,
) -> i64 {
    unavailable();
    -1
}

/// Reports an unavailable child wait with resource usage.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_wait4(
    _process_id: i64,
    _status: *mut libc::c_int,
    _flags: libc::c_int,
    _usage: *mut ElephcPcntlRUsage,
) -> i64 {
    unavailable();
    -1
}

/// Reports an unavailable child-state wait.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_waitid(
    _id_type: libc::c_int,
    _id: i64,
    _info: *mut ElephcPcntlSigInfo,
    _flags: libc::c_int,
    _usage: *mut ElephcPcntlRUsage,
) -> libc::c_int {
    unavailable();
    0
}

macro_rules! unavailable_status {
    ($name:ident) => {
        #[no_mangle]
        pub extern "C" fn $name(_status: libc::c_int) -> libc::c_int {
            unavailable();
            0
        }
    };
}

unavailable_status!(elephc_pcntl_wifexited);
unavailable_status!(elephc_pcntl_wifstopped);
unavailable_status!(elephc_pcntl_wifsignaled);
unavailable_status!(elephc_pcntl_wifcontinued);
unavailable_status!(elephc_pcntl_wexitstatus);
unavailable_status!(elephc_pcntl_wtermsig);
unavailable_status!(elephc_pcntl_wstopsig);

/// There is no staged Unix executable on this target.
#[no_mangle]
pub extern "C" fn elephc_pcntl_exec_input_error() -> libc::c_int {
    unavailable();
    PCNTL_EXEC_INPUT_OK
}

/// Rejects executable staging before it can imply that `execve` is supported.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_exec_new(
    _path: *const u8,
    _path_len: usize,
    _has_environment: libc::c_int,
) -> *mut libc::c_void {
    unavailable();
    std::ptr::null_mut()
}

unavailable_zero!(fn elephc_pcntl_exec_add_arg(builder: *mut libc::c_void, value: *const u8, value_len: usize) -> libc::c_int);
unavailable_zero!(fn elephc_pcntl_exec_add_env(builder: *mut libc::c_void, key_low: u64, key_high: i64, value: *const u8, value_len: usize) -> libc::c_int);
unavailable_zero!(fn elephc_pcntl_exec_run(builder: *mut libc::c_void) -> libc::c_int);

/// Releases no state because staging always failed before an allocation was made.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_exec_free(_builder: *mut libc::c_void) {
    unavailable();
}

/// Explains that CPU-affinity operations are unavailable on this target.
pub fn pcntl_cpu_affinity_value_error(_kind: libc::c_int, _process_id: i64) -> String {
    "pcntl_setcpuaffinity(): Operation is unavailable on this target".to_string()
}

/// Writes the CPU-affinity unsupported diagnostic into caller-owned storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_format_cpu_affinity_value_error(
    _kind: libc::c_int,
    _process_id: i64,
    buffer: *mut u8,
    capacity: usize,
) -> usize {
    if buffer.is_null() || capacity == 0 {
        return 0;
    }
    let message = pcntl_cpu_affinity_value_error(PCNTL_WARNING_CPU_AFFINITY, 0);
    let copied = message.len().min(capacity);
    std::ptr::copy_nonoverlapping(message.as_ptr(), buffer, copied);
    copied
}

/// Reports that Linux-only CPU inspection is unavailable.
#[no_mangle]
pub extern "C" fn elephc_pcntl_getcpu() -> i64 {
    unavailable();
    -1
}

/// Reports that Linux-only CPU-affinity inspection is unavailable.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_getcpuaffinity(
    _process_id: i64,
    _cpus: *mut i64,
    _capacity: usize,
) -> i64 {
    unavailable();
    -1
}

/// Reports that Linux-only CPU-affinity changes are unavailable.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_setcpuaffinity(
    _process_id: i64,
    _cpus: *const i64,
    _count: usize,
) -> libc::c_int {
    unavailable();
    0
}

/// Reports that Linux namespaces are unavailable.
#[no_mangle]
pub extern "C" fn elephc_pcntl_setns(
    _process_id: i64,
    _namespace_type: libc::c_int,
) -> libc::c_int {
    unavailable();
    0
}

/// Reports that Linux namespaces are unavailable.
#[no_mangle]
pub extern "C" fn elephc_pcntl_unshare(_flags: libc::c_int) -> libc::c_int {
    unavailable();
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows stubs must record a real unsupported-operation failure, never success.
    #[test]
    fn unavailable_process_operations_record_enosys() {
        assert_eq!(elephc_pcntl_fork(), -1);
        assert_eq!(crate::elephc_pcntl_get_last_error(), libc::ENOSYS);
    }
}
