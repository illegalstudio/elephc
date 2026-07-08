//! Purpose:
//! Worker CPU affinity for the prefork `--web` server. With `--worker-affinity`,
//! each forked worker pins itself to CPU `getpid() % ncpus` before entering the
//! serve loop — a best-effort lever that reduces scheduler migration and
//! improves per-worker L1/L2 cache warmth under N=1.
//!
//! Called from:
//! - `crate::server::spawn_worker` (in the forked child, after
//!   `reset_signal_handlers_to_default`, when `cfg.worker_affinity` is set).
//!
//! Key details:
//! - Linux: hard pin via `sched_setaffinity` to a single CPU. macOS: advisory
//!   `thread_policy_set` (`THREAD_AFFINITY_POLICY`) tag hint — macOS does NOT
//!   support hard CPU pinning. Other Unix: no-op. Failure is best-effort
//!   (eprintln + continue; never kills the worker).
//! - OFF path (`--worker-affinity` not set): `spawn_worker` never calls
//!   `pin_worker_cpu`, so this module is dead code that compiles out — the
//!   child arm is byte-for-byte the original.
//! - The `libc` crate (0.2.x) does NOT expose `cpu_set_t`/`CPU_ZERO`/`CPU_SET`
//!   on glibc/musl Linux (they are glibc inline macros the crate does not
//!   translate; only `sched_setaffinity` is declared but it references the
//!   undefined `cpu_set_t` type). To stay within `libc` (no new dep) and keep
//!   the 3-target matrix building, the Linux path declares
//!   `sched_setaffinity`/`sched_getaffinity` via a raw `extern "C"` block gated
//!   by `#[cfg(target_os = "linux")]` and uses a local `#[repr(C)]` `CpuSet`
//!   type that matches glibc's `cpu_set_t` layout (1024 bits = 128 bytes on
//!   64-bit). This mirrors the macOS raw-extern fallback the spec already
//!   sanctions for missing `libc` symbols.

/// Computes the CPU index a worker should pin to: `pid % ncpus`. Pure (no
/// syscalls), so it is unit-testable. `ncpus` is clamped to >= 1 by the caller
/// (`available_parallelism().unwrap_or(1)`), so this never divides by zero.
pub(crate) fn cpu_for_pid(pid: u64, ncpus: u64) -> u64 {
    pid % ncpus
}

/// Pins the calling worker process/thread to CPU `getpid() % ncpus`. Called
/// once in the forked child before the serve loop. Best-effort: on failure,
/// logs a warning and returns (does not kill the worker). Reads `ncpus` from
/// `std::thread::available_parallelism()` (logical CPU count).
pub(crate) fn pin_worker_cpu() {
    let ncpus = std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(1);
    // available_parallelism always returns >= 1, so ncpus >= 1 here.
    let pid = unsafe { libc::getpid() } as u64;
    let cpu = cpu_for_pid(pid, ncpus);
    pin_to_cpu(cpu);
}

// -- Linux (glibc/musl shared by linux-aarch64 + linux-x86_64) --
//
// `libc::cpu_set_t`/`CPU_ZERO`/`CPU_SET` are not exposed on glibc/musl in the
// `libc` crate (0.2.x): they are glibc inline macros the crate does not
// translate. `libc::sched_setaffinity` is declared but references the
// undefined `cpu_set_t` type, so it cannot be used as-is. We declare the
// affinity syscalls ourselves (the same way the macOS raw-extern fallback
// does for Mach symbols) and back them with a local `CpuSet` whose layout
// matches glibc's `cpu_set_t` (`__cpu_mask __bits[CPU_SETSIZE / __NCPUBITS]`,
// `CPU_SETSIZE = 1024`, `__cpu_mask = unsigned long`). On a 64-bit target this
// is `[usize; 16]` = 1024 bits = 128 bytes, identical to glibc's default
// `cpu_set_t`. The supported Linux targets (aarch64, x86_64) are all 64-bit.

#[cfg(target_os = "linux")]
const CPU_SETSIZE: usize = 1024;

/// Bits per `CpuSet` word (`8 * sizeof(unsigned long)` on 64-bit Linux = 64).
#[cfg(target_os = "linux")]
const CPU_WORD_BITS: usize = 8 * std::mem::size_of::<usize>();

/// glibc `cpu_set_t` equivalent: a 1024-bit bitset stored as an array of
/// `unsigned long` (`usize` on 64-bit). `#[repr(C)]` so its layout matches
/// glibc's `cpu_set_t` and the kernel reads the bits where it expects them.
#[cfg(target_os = "linux")]
#[repr(C)]
struct CpuSet {
    bits: [usize; CPU_SETSIZE / CPU_WORD_BITS],
}

#[cfg(target_os = "linux")]
extern "C" {
    /// Sets the CPU affinity mask of the process `pid` (`0` = caller). The
    /// mask is `cpusetsize` bytes at `cpuset`.
    fn sched_setaffinity(
        pid: libc::pid_t,
        cpusetsize: libc::size_t,
        cpuset: *const CpuSet,
    ) -> libc::c_int;
    /// Reads the CPU affinity mask of the process `pid` (`0` = caller) into
    /// `cpuset`. Used only by the round-trip unit test, so gated to `test` to
    /// avoid a `dead_code` warning in the non-test lib build (the CI builds the
    /// lib without `--cfg test` when linking it into integration-test binaries).
    #[cfg(test)]
    fn sched_getaffinity(
        pid: libc::pid_t,
        cpusetsize: libc::size_t,
        cpuset: *mut CpuSet,
    ) -> libc::c_int;
}

/// Returns whether CPU `cpu` is set in `set` (zero-indexed). Manual equivalent
/// of glibc's `CPU_IS(cpu, &set)` macro. Used only by the round-trip unit test,
/// so gated to `test` to avoid a `dead_code` warning in the non-test lib build.
#[cfg(all(target_os = "linux", test))]
fn cpu_is_set(cpu: usize, set: &CpuSet) -> bool {
    let word = cpu / CPU_WORD_BITS;
    let bit = cpu % CPU_WORD_BITS;
    word < set.bits.len() && (set.bits[word] & (1usize << bit)) != 0
}

/// Hard-pins the calling process to the single CPU `cpu` via
/// `sched_setaffinity(0, ..., {cpu})`. Best-effort: a non-zero return logs a
/// warning and returns (never kills the worker).
#[cfg(target_os = "linux")]
fn pin_to_cpu(cpu: u64) {
    let mut set = CpuSet {
        bits: [0usize; CPU_SETSIZE / CPU_WORD_BITS],
    };
    let word = (cpu as usize) / CPU_WORD_BITS;
    let bit = (cpu as usize) % CPU_WORD_BITS;
    // CPUs beyond CPU_SETSIZE (>= 1024) cannot be expressed in the static
    // `cpu_set_t`; skip setting the bit and let the kernel reject the empty
    // mask (best-effort, logs + continues).
    if word < set.bits.len() {
        set.bits[word] |= 1usize << bit;
    }
    // SAFETY: `sched_setaffinity(0, size, &set)` sets the calling process's
    // CPU affinity mask to the bits in `set`. `CpuSet` is `#[repr(C)]` with
    // the same layout as glibc's `cpu_set_t`. The call is best-effort: a
    // non-zero return (e.g. CPU index outside the allowed set) only logs.
    let rc = unsafe { sched_setaffinity(0, std::mem::size_of::<CpuSet>(), &set) };
    if rc != 0 {
        eprintln!(
            "elephc-web: --worker-affinity: sched_setaffinity to CPU {} failed: {}",
            cpu,
            std::io::Error::last_os_error()
        );
    }
}

// -- macOS (aarch64) --
//
// macOS does NOT support hard CPU pinning. The `thread_policy_set` Mach call
// with `THREAD_AFFINITY_POLICY` sets an advisory affinity tag (best-effort
// hint): the scheduler groups threads/processes with the same tag onto the
// same cluster/core when convenient, but never hard-pins. The tag is the CPU
// index as an `integer_t` (i32). All symbols are exposed by the `libc` crate
// on macOS, so we use `libc::` directly (no raw extern needed).

/// Sets an advisory affinity tag of `cpu` on the calling thread via the Mach
/// `thread_policy_set` call. Best-effort: a non-zero `kern_return` logs a
/// warning and returns (never kills the worker). This is an advisory hint —
/// macOS does NOT support hard CPU pinning.
#[cfg(target_os = "macos")]
fn pin_to_cpu(cpu: u64) {
    let mut tag: libc::integer_t = cpu as libc::integer_t;
    // SAFETY: `mach_thread_self()` returns the calling thread's Mach port
    // (always valid for the current thread); `thread_policy_set` with
    // `THREAD_AFFINITY_POLICY` sets an advisory affinity tag (best-effort
    // hint, not a hard pin — macOS does not support hard CPU affinity). The
    // tag pointer is a valid `*mut integer_t` to a stack local live across
    // the call.
    //
    // `libc::mach_thread_self` is marked `#[deprecated]` in libc 0.2.x in
    // favor of the `mach2` crate, but this crate's policy is to stay within
    // `libc` (no new deps). The symbol works correctly; suppress the
    // deprecation lint locally for that single call.
    #[allow(deprecated)]
    let kr = unsafe {
        libc::thread_policy_set(
            libc::mach_thread_self(),
            libc::THREAD_AFFINITY_POLICY as libc::thread_policy_flavor_t,
            &mut tag as *mut libc::integer_t as libc::thread_policy_t,
            1,
        )
    };
    if kr != 0 {
        eprintln!(
            "elephc-web: --worker-affinity: thread_policy_set(tag={}) failed (kern_return={}): {}",
            tag, kr, std::io::Error::last_os_error()
        );
    }
}

// -- Other Unix (unreachable in the supported matrix; keeps the crate portable) --

/// No-op CPU pin on targets without a pinning primitive. Unreachable in the
/// supported target matrix (macos-aarch64, linux-aarch64, linux-x86_64) but
/// keeps the crate portable to any Unix the prefork `fork()` path compiles on.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn pin_to_cpu(_cpu: u64) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies `cpu_for_pid` distributes workers round-robin across CPUs and
    /// wraps at `ncpus`.
    #[test]
    fn cpu_for_pid_round_robin() {
        assert_eq!(cpu_for_pid(0, 4), 0);
        assert_eq!(cpu_for_pid(5, 4), 1);
        assert_eq!(cpu_for_pid(7, 8), 7);
        assert_eq!(cpu_for_pid(4, 4), 0);
    }

    /// Verifies that with a single CPU every PID maps to CPU 0 (no division
    /// by zero, no out-of-range index).
    #[test]
    fn cpu_for_pid_single_cpu() {
        assert_eq!(cpu_for_pid(123, 1), 0);
    }

    // Linux-only round-trip: pin the test thread to CPU 0, read the affinity
    // mask back via `sched_getaffinity`, assert CPU 0 is set and (if the box
    // has > 1 CPU) CPU 1 is NOT set, then restore the original mask so the
    // test thread does not stay pinned for the rest of the suite.
    //
    // macOS has NO read-back API (`thread_policy_set` is advisory, write-only;
    // there is no `sched_getaffinity` equivalent), so no macOS round-trip test
    // exists here by design.
    #[cfg(target_os = "linux")]
    #[test]
    fn pin_roundtrip_sets_cpu_zero() {
        let size = std::mem::size_of::<CpuSet>();
        // Save the calling thread's current affinity mask so we can restore it.
        let mut cur = CpuSet {
            bits: [0usize; CPU_SETSIZE / CPU_WORD_BITS],
        };
        // SAFETY: `sched_getaffinity(0, size, &mut cur)` reads the calling
        // process's current CPU affinity mask. The pointer is a valid `*mut
        // CpuSet` of the right size; failure is non-fatal for the test.
        let grc = unsafe { sched_getaffinity(0, size, &mut cur) };
        if grc != 0 {
            // If the initial read fails (e.g. sandboxed CI without
            // permission to read affinity), the round-trip is not observable
            // here — skip rather than fail spuriously.
            eprintln!(
                "pin_roundtrip_sets_cpu_zero: sched_getaffinity failed: {}; skipping",
                std::io::Error::last_os_error()
            );
            return;
        }
        // Pin the calling thread to CPU 0.
        pin_to_cpu(0);
        // Read back the effective mask.
        let mut readback = CpuSet {
            bits: [0usize; CPU_SETSIZE / CPU_WORD_BITS],
        };
        // SAFETY: same as above; reads the calling process's affinity mask
        // into `readback`.
        let rrc = unsafe { sched_getaffinity(0, size, &mut readback) };
        assert_eq!(rrc, 0, "sched_getaffinity readback failed");
        assert!(cpu_is_set(0, &readback), "CPU 0 must be set after pin_to_cpu(0)");
        let ncpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        if ncpus > 1 {
            assert!(
                !cpu_is_set(1, &readback),
                "CPU 1 must NOT be set after pinning to CPU 0 (ncpus={})",
                ncpus
            );
        }
        // Restore the original mask so the test thread is not pinned to CPU 0
        // for the rest of the suite. Best-effort: ignore errors.
        // SAFETY: `sched_setaffinity(0, size, &cur)` restores the previously
        // saved affinity mask; `cur` was populated by `sched_getaffinity`.
        let _ = unsafe { sched_setaffinity(0, size, &cur) };
    }
}