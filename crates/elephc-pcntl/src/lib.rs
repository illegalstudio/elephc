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
#[cfg(test)]
use std::sync::Mutex;

static LAST_ERROR: AtomicI32 = AtomicI32::new(0);
#[cfg(test)]
static PROCESS_TEST_LOCK: Mutex<()> = Mutex::new(());

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
            ru_oublock: usage.ru_oublock as i64,
            ru_inblock: usage.ru_inblock as i64,
            ru_msgsnd: usage.ru_msgsnd as i64,
            ru_msgrcv: usage.ru_msgrcv as i64,
            ru_maxrss: usage.ru_maxrss as i64,
            ru_ixrss: usage.ru_ixrss as i64,
            ru_idrss: usage.ru_idrss as i64,
            ru_minflt: usage.ru_minflt as i64,
            ru_majflt: usage.ru_majflt as i64,
            ru_nsignals: usage.ru_nsignals as i64,
            ru_nvcsw: usage.ru_nvcsw as i64,
            ru_nivcsw: usage.ru_nivcsw as i64,
            ru_nswap: usage.ru_nswap as i64,
            ru_utime_tv_usec: usage.ru_utime.tv_usec as i64,
            ru_utime_tv_sec: usage.ru_utime.tv_sec as i64,
            ru_stime_tv_usec: usage.ru_stime.tv_usec as i64,
            ru_stime_tv_sec: usage.ru_stime.tv_sec as i64,
        }
    }
}

/// Stable, target-neutral signal-information record shared with generated AOT code.
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

const SIGINFO_SIGNO: u64 = 1 << 0;
const SIGINFO_ERRNO: u64 = 1 << 1;
const SIGINFO_CODE: u64 = 1 << 2;
const SIGINFO_STATUS: u64 = 1 << 3;
const SIGINFO_PID: u64 = 1 << 4;
const SIGINFO_UID: u64 = 1 << 5;
#[cfg(target_os = "linux")]
const SIGINFO_UTIME: u64 = 1 << 6;
#[cfg(target_os = "linux")]
const SIGINFO_STIME: u64 = 1 << 7;
#[cfg(target_os = "linux")]
const SIGINFO_ADDRESS: u64 = 1 << 8;
#[cfg(target_os = "linux")]
const SIGINFO_BAND: u64 = 1 << 9;
#[cfg(target_os = "linux")]
const SIGINFO_FD: u64 = 1 << 10;

/// Returns one past the largest signal number accepted by the current target.
#[cfg(target_os = "linux")]
fn signal_limit() -> libc::c_int {
    libc::SIGRTMAX() + 1
}

/// Returns one past the largest signal number accepted by Darwin.
#[cfg(target_os = "macos")]
const fn signal_limit() -> libc::c_int {
    32
}

/// Builds a native signal set from the stable widened integer-array ABI.
///
/// # Safety
/// `signals` must be readable for `count` consecutive `i64` values when `count` is nonzero.
unsafe fn build_signal_set(
    signals: *const i64,
    count: usize,
    allow_empty: bool,
) -> Option<libc::sigset_t> {
    if count == 0 && !allow_empty {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return None;
    }
    if count != 0 && signals.is_null() {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return None;
    }
    let mut set = std::mem::zeroed::<libc::sigset_t>();
    if libc::sigemptyset(&mut set) != 0 {
        record_errno();
        return None;
    }
    for index in 0..count {
        let signal = *signals.add(index);
        if signal < 1 || signal >= i64::from(signal_limit()) {
            LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
            return None;
        }
        if libc::sigaddset(&mut set, signal as libc::c_int) != 0 {
            record_errno();
            return None;
        }
    }
    Some(set)
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
        stable.utime = info.si_utime() as i64;
        stable.stime = info.si_stime() as i64;
        stable.present |= SIGINFO_UTIME | SIGINFO_STIME;
        stable
    }
    #[cfg(target_os = "macos")]
    {
        stable
    }
}

/// Copies Linux signal-specific information into the stable PCNTL record.
#[cfg(target_os = "linux")]
unsafe fn copy_signal_siginfo(
    signal: libc::c_int,
    info: &libc::siginfo_t,
) -> ElephcPcntlSigInfo {
    let mut stable = ElephcPcntlSigInfo {
        signo: i64::from(info.si_signo),
        error: i64::from(info.si_errno),
        code: i64::from(info.si_code),
        present: SIGINFO_SIGNO | SIGINFO_ERRNO | SIGINFO_CODE,
        ..ElephcPcntlSigInfo::default()
    };
    if signal == libc::SIGCHLD {
        stable.status = i64::from(info.si_status());
        stable.utime = info.si_utime() as i64;
        stable.stime = info.si_stime() as i64;
        stable.pid = i64::from(info.si_pid());
        stable.uid = i64::from(info.si_uid());
        stable.present |= SIGINFO_STATUS
            | SIGINFO_UTIME
            | SIGINFO_STIME
            | SIGINFO_PID
            | SIGINFO_UID;
    } else if signal == libc::SIGUSR1
        || signal == libc::SIGUSR2
        || (signal >= libc::SIGRTMIN() && signal <= libc::SIGRTMAX())
    {
        stable.pid = i64::from(info.si_pid());
        stable.uid = i64::from(info.si_uid());
        stable.present |= SIGINFO_PID | SIGINFO_UID;
    } else if matches!(signal, libc::SIGILL | libc::SIGFPE | libc::SIGSEGV | libc::SIGBUS) {
        stable.address = info.si_addr() as usize as i64;
        stable.present |= SIGINFO_ADDRESS;
    } else if signal == libc::SIGPOLL {
        #[repr(C)]
        struct PollSigInfo {
            signo: libc::c_int,
            error: libc::c_int,
            code: libc::c_int,
            alignment: libc::c_int,
            band: libc::c_long,
            fd: libc::c_int,
        }
        let poll = &*(info as *const libc::siginfo_t).cast::<PollSigInfo>();
        stable.band = poll.band as i64;
        stable.fd = i64::from(poll.fd);
        stable.present |= SIGINFO_BAND | SIGINFO_FD;
    }
    stable
}

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
/// A failure returns `-1` and records errno. On success the target-native `rusage` structure is
/// copied into the fixed 17-word bridge ABI consumed by generated code.
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
    } else if !usage.is_null() {
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

/// Changes the calling thread's signal mask and optionally returns its prior members.
///
/// A nonnegative return is the number of signals written to `old_signals`; `-1` records errno
/// or `EINVAL` and leaves the caller's PHP output untouched.
///
/// # Safety
/// `signals` must be readable for `count` values. When non-null, `old_signals` must be writable
/// for `old_capacity` values.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_sigprocmask(
    how: libc::c_int,
    signals: *const i64,
    count: usize,
    old_signals: *mut i64,
    old_capacity: usize,
) -> i64 {
    if !matches!(how, libc::SIG_BLOCK | libc::SIG_UNBLOCK | libc::SIG_SETMASK) {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return -1;
    }
    let Some(set) = build_signal_set(signals, count, how == libc::SIG_SETMASK) else {
        return -1;
    };
    let mut old_set = std::mem::zeroed::<libc::sigset_t>();
    if libc::sigprocmask(how, &set, &mut old_set) != 0 {
        record_errno();
        return -1;
    }
    if old_signals.is_null() {
        return 0;
    }
    let mut old_count = 0usize;
    for signal in 1..signal_limit() {
        if libc::sigismember(&old_set, signal) != 1 {
            continue;
        }
        if old_count == old_capacity {
            LAST_ERROR.store(libc::EOVERFLOW, Ordering::Relaxed);
            return -1;
        }
        *old_signals.add(old_count) = i64::from(signal);
        old_count += 1;
    }
    old_count as i64
}

/// Waits synchronously for one Linux signal and writes its stable signal-information record.
///
/// Returns the delivered signal number or `-1` after recording errno.
///
/// # Safety
/// `signals` must be readable for `count` values and `info` must be null or writable for one
/// `ElephcPcntlSigInfo` value.
#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_sigwaitinfo(
    signals: *const i64,
    count: usize,
    info: *mut ElephcPcntlSigInfo,
) -> i64 {
    let Some(set) = build_signal_set(signals, count, false) else {
        return -1;
    };
    let mut native_info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    *errno_location() = 0;
    let mut signal = libc::sigwaitinfo(&set, native_info.as_mut_ptr());
    if signal == -1 {
        record_errno();
        return -1;
    }
    let native_info = native_info.assume_init();
    if signal == 0 && native_info.si_signo != 0 {
        signal = native_info.si_signo;
    }
    if !info.is_null() {
        *info = copy_signal_siginfo(signal, &native_info);
    }
    i64::from(signal)
}

/// Waits up to the supplied Linux timeout for one selected signal.
///
/// Returns the delivered signal number or `-1`. A timeout intentionally leaves the previous
/// PCNTL last-error value unchanged, matching PHP's `EAGAIN` behavior.
///
/// # Safety
/// `signals` must be readable for `count` values and `info` must be null or writable for one
/// `ElephcPcntlSigInfo` value.
#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_sigtimedwait(
    signals: *const i64,
    count: usize,
    info: *mut ElephcPcntlSigInfo,
    seconds: i64,
    nanoseconds: i64,
) -> i64 {
    let Some(set) = build_signal_set(signals, count, false) else {
        return -1;
    };
    if seconds < 0 || !(0..1_000_000_000).contains(&nanoseconds) || (seconds == 0 && nanoseconds == 0)
    {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return -1;
    }
    let timeout = libc::timespec {
        tv_sec: seconds as _,
        tv_nsec: nanoseconds as libc::c_long,
    };
    *errno_location() = 0;
    let mut native_info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let mut signal = libc::sigtimedwait(&set, native_info.as_mut_ptr(), &timeout);
    if signal == -1 {
        if *errno_location() != libc::EAGAIN {
            record_errno();
        }
        return -1;
    }
    let native_info = native_info.assume_init();
    if signal == 0 && native_info.si_signo != 0 {
        signal = native_info.si_signo;
    }
    if !info.is_null() {
        *info = copy_signal_siginfo(signal, &native_info);
    }
    i64::from(signal)
}

/// Returns the logical CPU on which the calling thread is currently executing.
///
/// A failure returns `-1` and records errno for `pcntl_get_last_error()`.
#[cfg(target_os = "linux")]
#[no_mangle]
pub extern "C" fn elephc_pcntl_getcpu() -> i64 {
    let cpu = unsafe { libc::sched_getcpu() };
    if cpu == -1 {
        record_errno();
    }
    i64::from(cpu)
}

/// Copies the selected process CPU affinity into a stable array of widened CPU identifiers.
///
/// Returns the number of copied identifiers, or `-1` after recording errno. A process id of zero
/// follows `sched_getaffinity()` and selects the calling process.
///
/// # Safety
/// `cpus` must point to writable storage for at least `capacity` `i64` values.
#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_getcpuaffinity(
    process_id: i64,
    cpus: *mut i64,
    capacity: usize,
) -> i64 {
    if cpus.is_null() || capacity == 0 {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return -1;
    }
    let mut mask = std::mem::zeroed::<libc::cpu_set_t>();
    libc::CPU_ZERO(&mut mask);
    if libc::sched_getaffinity(
        process_id as libc::pid_t,
        std::mem::size_of::<libc::cpu_set_t>(),
        &mut mask,
    ) != 0
    {
        record_errno();
        return -1;
    }
    let configured = libc::sysconf(libc::_SC_NPROCESSORS_CONF);
    let limit = if configured > 0 {
        configured as usize
    } else {
        libc::CPU_SETSIZE as usize
    }
    .min(libc::CPU_SETSIZE as usize);
    let mut count = 0usize;
    for cpu in 0..limit {
        if libc::CPU_ISSET(cpu, &mask) {
            if count == capacity {
                LAST_ERROR.store(libc::EOVERFLOW, Ordering::Relaxed);
                return -1;
            }
            *cpus.add(count) = cpu as i64;
            count += 1;
        }
    }
    count as i64
}

/// Replaces the selected process CPU affinity with the supplied CPU identifier list.
///
/// Returns one on success or zero after recording errno. Empty masks and identifiers outside the
/// configured CPU range are rejected with `EINVAL`, matching PHP's value validation boundary.
///
/// # Safety
/// `cpus` must point to readable storage for at least `count` `i64` values.
#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_setcpuaffinity(
    process_id: i64,
    cpus: *const i64,
    count: usize,
) -> libc::c_int {
    if cpus.is_null() || count == 0 {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return 0;
    }
    let configured = libc::sysconf(libc::_SC_NPROCESSORS_CONF);
    let limit = if configured > 0 {
        configured as usize
    } else {
        libc::CPU_SETSIZE as usize
    }
    .min(libc::CPU_SETSIZE as usize);
    let mut mask = std::mem::zeroed::<libc::cpu_set_t>();
    libc::CPU_ZERO(&mut mask);
    for index in 0..count {
        let cpu = *cpus.add(index);
        if cpu < 0 || cpu as usize >= limit {
            LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
            return 0;
        }
        libc::CPU_SET(cpu as usize, &mut mask);
    }
    if libc::sched_setaffinity(
        process_id as libc::pid_t,
        std::mem::size_of::<libc::cpu_set_t>(),
        &mask,
    ) != 0
    {
        record_errno();
        return 0;
    }
    1
}

/// Joins the selected process namespace identified by `namespace_type`.
///
/// A process id of zero selects the calling process. Returns one on success or zero after
/// recording the `pidfd_open()` or `setns()` errno.
#[cfg(target_os = "linux")]
#[no_mangle]
pub extern "C" fn elephc_pcntl_setns(
    process_id: i64,
    namespace_type: libc::c_int,
) -> libc::c_int {
    let pid = if process_id == 0 {
        unsafe { libc::getpid() }
    } else {
        process_id as libc::pid_t
    };
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) as libc::c_int };
    if fd == -1 {
        record_errno();
        return 0;
    }
    let result = unsafe { libc::setns(fd, namespace_type) };
    let setns_error = if result == -1 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    } else {
        0
    };
    unsafe { libc::close(fd) };
    if result == -1 {
        LAST_ERROR.store(setns_error, Ordering::Relaxed);
        return 0;
    }
    1
}

/// Disassociates the requested Linux process execution contexts.
///
/// Returns one on success or zero after recording errno, matching PHP's boolean surface.
#[cfg(target_os = "linux")]
#[no_mangle]
pub extern "C" fn elephc_pcntl_unshare(flags: libc::c_int) -> libc::c_int {
    if unsafe { libc::unshare(flags) } == -1 {
        record_errno();
        return 0;
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the current Linux CPU and affinity mask through the stable bridge ABI.
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cpu_queries_return_consistent_identifiers() {
        let cpu = elephc_pcntl_getcpu();
        assert!(cpu >= 0, "sched_getcpu failed with errno {}", elephc_pcntl_get_last_error());
        let mut cpus = [0i64; libc::CPU_SETSIZE as usize];
        let count = unsafe { elephc_pcntl_getcpuaffinity(0, cpus.as_mut_ptr(), cpus.len()) };
        assert!(count > 0, "get affinity failed with errno {}", elephc_pcntl_get_last_error());
        assert!(cpus[..count as usize].contains(&cpu));
    }

    /// Reapplies the current Linux affinity mask without changing process placement policy.
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cpu_affinity_round_trips_current_mask() {
        let mut cpus = [0i64; libc::CPU_SETSIZE as usize];
        let count = unsafe { elephc_pcntl_getcpuaffinity(0, cpus.as_mut_ptr(), cpus.len()) };
        assert!(count > 0, "get affinity failed with errno {}", elephc_pcntl_get_last_error());
        let success = unsafe { elephc_pcntl_setcpuaffinity(0, cpus.as_ptr(), count as usize) };
        assert_eq!(success, 1, "set affinity failed with errno {}", elephc_pcntl_get_last_error());
    }

    /// Rejects an empty Linux affinity mask and exposes the expected `EINVAL` status.
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_empty_cpu_affinity_is_rejected() {
        let success = unsafe { elephc_pcntl_setcpuaffinity(0, std::ptr::null(), 0) };
        assert_eq!(success, 0);
        assert_eq!(elephc_pcntl_get_last_error(), libc::EINVAL);
    }

    /// Forks a real child, reaps it, and verifies target-native wait status decoding.
    #[test]
    fn fork_waitpid_and_status_decoding_round_trip() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
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

    /// Reaps a real child through the any-child wait entry point.
    #[test]
    fn fork_wait_and_status_decoding_round_trip() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
        let pid = elephc_pcntl_fork();
        assert!(pid >= 0, "fork failed with errno {}", elephc_pcntl_get_last_error());
        if pid == 0 {
            unsafe { libc::_exit(31) };
        }

        let mut status = 0;
        let waited = unsafe { elephc_pcntl_wait(&mut status, 0) };
        assert_eq!(waited, pid);
        assert_eq!(elephc_pcntl_wifexited(status), 1);
        assert_eq!(elephc_pcntl_wexitstatus(status), 31);
    }

    /// Reaps a real child through `wait4` and exposes its usage in the stable bridge layout.
    #[test]
    fn fork_wait4_populates_stable_resource_usage() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
        let pid = elephc_pcntl_fork();
        assert!(pid >= 0, "fork failed with errno {}", elephc_pcntl_get_last_error());
        if pid == 0 {
            unsafe { libc::_exit(19) };
        }

        let mut status = 0;
        let mut usage = ElephcPcntlRUsage::default();
        let waited = unsafe { elephc_pcntl_wait4(pid, &mut status, 0, &mut usage) };
        assert_eq!(waited, pid);
        assert_eq!(elephc_pcntl_wexitstatus(status), 19);
        assert!(usage.ru_utime_tv_sec >= 0);
        assert!(usage.ru_stime_tv_sec >= 0);
    }

    /// Reaps a real child through `waitid` and copies its portable PHP information fields.
    #[test]
    fn fork_waitid_populates_stable_siginfo() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
        let pid = elephc_pcntl_fork();
        assert!(pid >= 0, "fork failed with errno {}", elephc_pcntl_get_last_error());
        if pid == 0 {
            unsafe { libc::_exit(29) };
        }

        let mut info = ElephcPcntlSigInfo::default();
        let success = unsafe {
            elephc_pcntl_waitid(libc::P_PID as libc::c_int, pid, &mut info, libc::WEXITED)
        };
        assert_eq!(success, 1);
        assert_eq!(info.pid, pid);
        assert_eq!(info.status, 29);
        assert_ne!(info.present & SIGINFO_STATUS, 0);
    }

    /// Blocks one signal through the stable array ABI, returns the prior mask, and restores it.
    #[test]
    fn signal_mask_round_trips_old_members() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
        let mut original = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        let mut selected = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        unsafe {
            libc::sigemptyset(&mut selected);
            libc::sigaddset(&mut selected, libc::SIGUSR1);
            assert_eq!(libc::sigprocmask(libc::SIG_UNBLOCK, &selected, &mut original), 0);
        }
        let signals = [i64::from(libc::SIGUSR1)];
        let mut old = [0i64; 128];
        let count = unsafe {
            elephc_pcntl_sigprocmask(
                libc::SIG_BLOCK,
                signals.as_ptr(),
                signals.len(),
                old.as_mut_ptr(),
                old.len(),
            )
        };
        assert!(count >= 0);
        assert!(!old[..count as usize].contains(&i64::from(libc::SIGUSR1)));
        unsafe {
            assert_eq!(libc::sigprocmask(libc::SIG_SETMASK, &original, std::ptr::null_mut()), 0);
        }
    }

    /// Receives a queued Linux signal synchronously and exposes sender identity fields.
    #[cfg(target_os = "linux")]
    #[test]
    fn signal_wait_info_receives_a_blocked_signal() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
        let mut original = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        let mut selected = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        unsafe {
            libc::sigemptyset(&mut selected);
            libc::sigaddset(&mut selected, libc::SIGUSR1);
            assert_eq!(libc::sigprocmask(libc::SIG_BLOCK, &selected, &mut original), 0);
            assert_eq!(libc::raise(libc::SIGUSR1), 0);
        }
        let signals = [i64::from(libc::SIGUSR1)];
        let mut info = ElephcPcntlSigInfo::default();
        let signal = unsafe { elephc_pcntl_sigwaitinfo(signals.as_ptr(), 1, &mut info) };
        unsafe {
            assert_eq!(libc::sigprocmask(libc::SIG_SETMASK, &original, std::ptr::null_mut()), 0);
        }
        assert_eq!(signal, i64::from(libc::SIGUSR1));
        assert_eq!(info.signo, i64::from(libc::SIGUSR1));
        assert_eq!(info.pid, i64::from(unsafe { libc::getpid() }));
        assert_ne!(info.present & SIGINFO_PID, 0);
        assert_ne!(info.present & SIGINFO_UID, 0);
    }

    /// Times out without replacing PCNTL's last error when no selected signal is pending.
    #[cfg(target_os = "linux")]
    #[test]
    fn timed_signal_wait_preserves_last_error_on_timeout() {
        let _guard = PROCESS_TEST_LOCK.lock().expect("process test lock poisoned");
        LAST_ERROR.store(731, Ordering::Relaxed);
        let signals = [i64::from(libc::SIGUSR2)];
        let signal = unsafe {
            elephc_pcntl_sigtimedwait(
                signals.as_ptr(),
                signals.len(),
                std::ptr::null_mut(),
                0,
                1,
            )
        };
        assert_eq!(signal, -1);
        assert_eq!(elephc_pcntl_get_last_error(), 731);
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
