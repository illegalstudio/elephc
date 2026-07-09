//! Purpose:
//! Measures this worker process's resident set size (RSS) so the accept loops
//! can enforce `--max-rss` worker recycling. Exposes one cheap helper,
//! `current_rss_bytes`, used by `worker::serve` and `worker_mode::enter_worker_loop`.
//!
//! Called from:
//! - `crate::worker::serve` (classic `--web` accept loop, after the
//!   `--max-requests` check).
//! - `crate::worker_mode::enter_worker_loop` (`--web-worker` and
//!   `--web-worker=script` accept loop, same position).
//!
//! Key details:
//! - Target-aware: Linux reads `/proc/self/status` (`VmRSS:` KiB), macOS uses
//!   `task_info` with `MACH_TASK_BASIC_INFO` (`resident_size` is bytes already).
//!   Every other target returns `None` (safe no-op — the caller skips
//!   recycling, so `--max-rss` is inert there by design).
//! - The caller gates measurement to at most once per 64 accepts (plus the
//!   first accept), so this never runs on every hot-path iteration; a small
//!   allocation on the Linux path is fine because the path is cold.
//! - A read failure returns `None`, which the caller treats as "do not
//!   recycle", so a transient parse/IO error is safe.

/// Returns this process's current resident set size in bytes, or `None` if the
/// platform is unsupported or the read fails (the caller treats `None` as "do
/// not recycle", so a read failure is safe). Used by the worker accept loops to
/// enforce `--max-rss`. Cheap (one syscall on Linux, one `task_info` mach call
/// on macOS); callers gate it so it never runs on every accept.
pub(crate) fn current_rss_bytes() -> Option<u64> {
    platform_rss()
}

#[cfg(target_os = "linux")]
fn platform_rss() -> Option<u64> {
    // Linux: /proc/self/status is a small text file with a `VmRSS:` line in
    // KiB. Read once per call (callers gate to <= 1/64 accepts), parse the
    // integer, convert KiB → bytes. A missing/unparseable line yields `None`.
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            // Format: `VmRSS:\t    1234 kB`
            let trimmed = rest.trim();
            // Take the leading integer (the numeric token before the unit).
            let num_end = trimmed
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(trimmed.len());
            if num_end == 0 {
                return None;
            }
            let kilobytes: u64 = trimmed[..num_end].parse().ok()?;
            return Some(kilobytes.saturating_mul(1024));
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn platform_rss() -> Option<u64> {
    // macOS: `task_info` with `MACH_TASK_BASIC_INFO` (flavor 20) fills a
    // `mach_task_basic_info` whose `resident_size` field is the RSS in bytes
    // already (no unit conversion needed). Stable Apple ABI; raw `extern "C"`
    // bindings to the `system`/`mach` API avoid adding a new crate dependency.
    // MACH_TASK_BASIC_INFO_COUNT = sizeof(mach_task_basic_info) / sizeof(natural_t)
    //   = 48 / 4 = 12.
    extern "C" {
        fn mach_task_self() -> u32; // mach_port_t
        fn task_info(
            target_task: u32,
            flavor: u32,
            task_info_out: *mut u8,
            task_info_count: *mut u32,
        ) -> i32; // kern_return_t; 0 = KERN_SUCCESS
    }

    // MACH_TASK_BASIC_INFO flavor constant (value 20).
    const MACH_TASK_BASIC_INFO: u32 = 20;
    // Expected count for `mach_task_basic_info` (in `natural_t` units).
    const MACH_TASK_BASIC_INFO_COUNT: u32 = 12;

    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        // `time_value_t` is two `integer_t` (int32) fields: secs + microsecs.
        user_time_secs: i32,
        user_time_microsecs: i32,
        system_time_secs: i32,
        system_time_microsecs: i32,
        policy: i32,
        suspend_count: i32,
    }

    let mut info: MachTaskBasicInfo = MachTaskBasicInfo {
        virtual_size: 0,
        resident_size: 0,
        resident_size_max: 0,
        user_time_secs: 0,
        user_time_microsecs: 0,
        system_time_secs: 0,
        system_time_microsecs: 0,
        policy: 0,
        suspend_count: 0,
    };
    let mut count: u32 = MACH_TASK_BASIC_INFO_COUNT;
    // SAFETY: `mach_task_self()` returns the current task port; passing it
    // with `MACH_TASK_BASIC_INFO` and a count of 12 writes at most
    // `sizeof(mach_task_basic_info)` bytes into `info`. The out-count pointer
    // is valid for one u32. Non-zero `kern_return` is treated as failure.
    let kr = unsafe {
        task_info(
            mach_task_self(),
            MACH_TASK_BASIC_INFO,
            &mut info as *mut _ as *mut u8,
            &mut count as *mut u32,
        )
    };
    if kr != 0 {
        return None;
    }
    Some(info.resident_size)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_rss() -> Option<u64> {
    // Unsupported target: the caller skips recycling, so `--max-rss` is a
    // safe no-op here. This is the intentional-unsupported path documented in
    // the module preamble; it never runs on the supported target matrix.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies `current_rss_bytes` returns `Some` on every supported target
    /// (the CI host is `macos-aarch64`; Linux runs in CI too via the Docker
    /// scripts). On unsupported targets the call returns `None` by design and
    /// the `is_some()` assertion is skipped (gated by the same cfg).
    #[test]
    fn current_rss_bytes_returns_some_on_supported_target() {
        let rss = current_rss_bytes();
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            assert!(rss.is_some(), "current_rss_bytes() returned None on a supported target");
            // The value must be positive (a running process has nonzero RSS).
            let bytes = rss.unwrap();
            assert!(bytes > 0, "current_rss_bytes() returned 0; RSS should be nonzero");
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            // Unsupported target: the call is a documented no-op.
            assert!(rss.is_none(), "current_rss_bytes() returned Some on an unsupported target");
        }
    }
}