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

mod constants;
mod exec;
mod process;
mod signals;

pub use constants::{
    host_pcntl_int_constant, is_pcntl_int_constant, LINUX_PCNTL_INT_CONSTANTS,
    MACOS_PCNTL_INT_CONSTANTS,
};
pub use exec::*;
pub use process::*;
pub use signals::*;

use std::ffi::CStr;
use std::sync::atomic::{AtomicI32, Ordering};
#[cfg(test)]
use std::sync::Mutex;

static LAST_ERROR: AtomicI32 = AtomicI32::new(0);
#[cfg(test)]
static PROCESS_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Selects PHP's `pcntl_fork()` failure warning in the stable formatting ABI.
pub const PCNTL_WARNING_FORK: libc::c_int = 0;
/// Selects PHP's `pcntl_exec()` failure warning in the stable formatting ABI.
pub const PCNTL_WARNING_EXEC: libc::c_int = 1;
/// Selects PHP's `pcntl_signal()` failure warning in the stable formatting ABI.
pub const PCNTL_WARNING_SIGNAL: libc::c_int = 2;

/// Reads the current thread's OS errno without mutating PCNTL state.
fn current_errno() -> libc::c_int {
    unsafe { *errno_location() }
}

/// Records the current thread's OS errno as the last PCNTL error.
fn record_errno() {
    LAST_ERROR.store(current_errno(), Ordering::Relaxed);
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

/// Formats the PHP warning corresponding to the latest failing process operation.
///
/// The returned bytes include PHP's warning prefix and trailing newline so AOT and Magician can
/// forward the same diagnostic through their suppressible warning channels.
pub fn pcntl_last_error_warning(kind: libc::c_int) -> String {
    let error = LAST_ERROR.load(Ordering::Relaxed);
    match kind {
        PCNTL_WARNING_FORK => {
            let detail = match error {
                libc::EAGAIN => "Reached the maximum limit of number of processes",
                libc::ENOMEM => "Insufficient memory",
                libc::ENOSYS => "Unimplemented",
                libc::EBADF => "File descriptor concurrency issue",
                _ => return format!("Warning: pcntl_fork(): Error {error}\n"),
            };
            format!("Warning: pcntl_fork(): Error {error}: {detail}\n")
        }
        PCNTL_WARNING_EXEC => {
            let detail = unsafe { libc::strerror(error).as_ref() }
                .map(|pointer| unsafe { CStr::from_ptr(pointer) }.to_string_lossy())
                .unwrap_or_else(|| "Unknown error".into());
            format!("Warning: pcntl_exec(): Error has occurred: (errno {error}) {detail}\n")
        }
        PCNTL_WARNING_SIGNAL => "Warning: pcntl_signal(): Error assigning signal\n".to_string(),
        _ => "Warning: PCNTL operation failed\n".to_string(),
    }
}

/// Copies one formatted PHP warning into caller-owned storage and returns the copied byte count.
///
/// # Safety
/// `buffer` must be writable for `capacity` bytes when `capacity` is nonzero.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_format_last_error_warning(
    kind: libc::c_int,
    buffer: *mut u8,
    capacity: usize,
) -> usize {
    if buffer.is_null() || capacity == 0 {
        return 0;
    }
    let warning = pcntl_last_error_warning(kind);
    let copied = warning.len().min(capacity);
    std::ptr::copy_nonoverlapping(warning.as_ptr(), buffer, copied);
    copied
}

#[cfg(target_os = "macos")]
type QosClass = u32;

#[cfg(target_os = "macos")]
const QOS_CLASS_USER_INTERACTIVE: QosClass = 0x21;
#[cfg(target_os = "macos")]
const QOS_CLASS_USER_INITIATED: QosClass = 0x19;
#[cfg(target_os = "macos")]
const QOS_CLASS_DEFAULT: QosClass = 0x15;
#[cfg(target_os = "macos")]
const QOS_CLASS_UTILITY: QosClass = 0x11;
#[cfg(target_os = "macos")]
const QOS_CLASS_BACKGROUND: QosClass = 0x09;

#[cfg(target_os = "macos")]
extern "C" {
    /// Reads the requested QoS class of one Darwin pthread.
    fn pthread_get_qos_class_np(
        thread: libc::pthread_t,
        qos_class: *mut QosClass,
        relative_priority: *mut libc::c_int,
    ) -> libc::c_int;

    /// Changes the calling Darwin pthread's requested QoS class.
    fn pthread_set_qos_class_self_np(
        qos_class: QosClass,
        relative_priority: libc::c_int,
    ) -> libc::c_int;
}

/// Maps one Darwin QoS value into the stable ordinal consumed by generated code.
#[cfg(target_os = "macos")]
const fn qos_class_ordinal(qos_class: QosClass) -> libc::c_int {
    match qos_class {
        QOS_CLASS_USER_INTERACTIVE => 0,
        QOS_CLASS_USER_INITIATED => 1,
        QOS_CLASS_UTILITY => 3,
        QOS_CLASS_BACKGROUND => 4,
        _ => 2,
    }
}

/// Maps a PHP `Pcntl\QosClass` case name into its Darwin QoS value.
#[cfg(target_os = "macos")]
fn qos_class_from_name(name: &[u8]) -> Option<QosClass> {
    match name {
        b"UserInteractive" => Some(QOS_CLASS_USER_INTERACTIVE),
        b"UserInitiated" => Some(QOS_CLASS_USER_INITIATED),
        b"Default" => Some(QOS_CLASS_DEFAULT),
        b"Utility" => Some(QOS_CLASS_UTILITY),
        b"Background" => Some(QOS_CLASS_BACKGROUND),
        _ => None,
    }
}

/// Returns the current Darwin pthread QoS as a stable `Pcntl\QosClass` ordinal.
///
/// Returns `-1` and records the pthread error when inspection fails.
#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "C" fn elephc_pcntl_getqos_class() -> libc::c_int {
    let mut qos_class = 0;
    let result = unsafe {
        pthread_get_qos_class_np(libc::pthread_self(), &mut qos_class, std::ptr::null_mut())
    };
    if result != 0 {
        LAST_ERROR.store(result, Ordering::Relaxed);
        return -1;
    }
    qos_class_ordinal(qos_class)
}

/// Changes the current Darwin pthread QoS from a PHP enum case name.
///
/// Returns `1` on success, or `0` after recording `EINVAL`, `EFAULT`, or the pthread error.
///
/// # Safety
/// `name` must be readable for `name_len` bytes when `name_len` is nonzero.
#[cfg(target_os = "macos")]
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_setqos_class(
    name: *const u8,
    name_len: usize,
) -> libc::c_int {
    if name.is_null() && name_len != 0 {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return 0;
    }
    let name = if name_len == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(name, name_len)
    };
    let Some(qos_class) = qos_class_from_name(name) else {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return 0;
    };
    let result = pthread_set_qos_class_self_np(qos_class, 0);
    if result != 0 {
        LAST_ERROR.store(result, Ordering::Relaxed);
        return 0;
    }
    1
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
mod tests;
