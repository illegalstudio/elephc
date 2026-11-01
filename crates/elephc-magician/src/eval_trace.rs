//! Purpose:
//! Answers "is the eval trace on?" from a cached value instead of the environment, so a DISABLED
//! trace costs one relaxed atomic load rather than a `getenv`.
//!
//! Called from:
//! - every `[elephc-eval-trace]` emission site in the interpreter, the parser and the FFI layer.
//! - `crate::interpreter::builtins::network_env::putenv` to invalidate the cache.
//!
//! Key details:
//! - `ELEPHC_EVAL_TRACE` is read at 100+ sites, several of them per interpreted expression. `getenv`
//!   is not free: on macOS it takes `os_unfair_lock` and linearly scans `environ`, and a Symfony
//!   `--web` worker spent more samples there than anywhere else except the idle wait — for a
//!   facility that was switched OFF.
//! - The cache is INVALIDATED rather than permanent, because PHP's `putenv()` can set the variable
//!   mid-run and a `OnceLock` would then report the old answer forever. Invalidation keeps the
//!   observable behaviour exactly what reading the environment every time gave.
//! - Two questions are cached in one slot because they are answered by one lookup: whether the
//!   variable is set at all, and whether its value selects the FFI-entry trace.
//! - It also carries the one-slot hand-off that lets `phase=include_ok` name the file an include
//!   actually reached; see [`note_include_target`].

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

/// The cached verdict has not been computed since the last invalidation.
const TRACE_UNKNOWN: u8 = 0;
/// `ELEPHC_EVAL_TRACE` is unset: every trace site is off.
const TRACE_OFF: u8 = 1;
/// `ELEPHC_EVAL_TRACE` is set to a value that does not select the FFI-entry trace.
const TRACE_ON: u8 = 2;
/// `ELEPHC_EVAL_TRACE` is set to `ffi` or `all`, which also selects the FFI-entry trace.
const TRACE_ON_FFI: u8 = 3;

static TRACE_STATE: AtomicU8 = AtomicU8::new(TRACE_UNKNOWN);

/// Returns whether any eval trace output is enabled.
pub(crate) fn enabled() -> bool {
    verdict_enables(state())
}

/// Returns whether the per-FFI-entry trace is enabled, which needs `ffi` or `all` specifically.
pub(crate) fn ffi_enabled() -> bool {
    verdict_enables_ffi(state())
}

/// Whether a cached verdict switches the trace on.
fn verdict_enables(verdict: u8) -> bool {
    verdict >= TRACE_ON
}

/// Whether a cached verdict switches the per-FFI-entry trace on.
fn verdict_enables_ffi(verdict: u8) -> bool {
    verdict == TRACE_ON_FFI
}

/// The verdict an `ELEPHC_EVAL_TRACE` value (or its absence) gives.
fn verdict_for_level(level: Option<&std::ffi::OsStr>) -> u8 {
    match level {
        None => TRACE_OFF,
        Some(level) => {
            let level = level.to_string_lossy();
            if level == "ffi" || level == "all" {
                TRACE_ON_FFI
            } else {
                TRACE_ON
            }
        }
    }
}

thread_local! {
    /// The resolved target of the include that most recently finished on this thread.
    static INCLUDE_TARGET: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Records the file an include actually reached, for the trace line emitted after it returns.
///
/// The two halves of an include live in different crates' worth of frames. The bridge entry point
/// `__elephc_eval_include` is the only place that knows the include CROSSED from compiled code, so
/// it owns `phase=include_ok` — but all it has is the caller's position. Turning `$file` into a
/// path happens in the interpreter's include driver, several frames down, and that resolution is
/// the whole answer to "what did this request include?". Passing it back through the return type
/// would change a public signature for a facility that is off in every real run, so it travels in
/// this slot instead.
///
/// Written LAST, after the include's body has finished, so a nested include cannot leave its own
/// target behind for the enclosing one: the inner call writes and the bridge takes it, then the
/// outer call overwrites with its own. Nothing is allocated when the trace is off.
pub(crate) fn note_include_target(path: &Path) {
    if !recording_include_targets() {
        return;
    }
    INCLUDE_TARGET.with(|slot| {
        slot.replace(Some(path.to_path_buf()));
    });
}

/// Removes and returns the target recorded by the include that just finished.
///
/// Taking rather than peeking is deliberate: an emission site that finds the slot empty must print
/// that it does not know, never a target left over from an earlier include.
pub(crate) fn take_include_target() -> Option<PathBuf> {
    if !recording_include_targets() {
        return None;
    }
    INCLUDE_TARGET.with(|slot| slot.borrow_mut().take())
}

/// Answers whether the include target slot is live: exactly `enabled()` outside tests.
#[cfg(not(test))]
#[inline]
fn recording_include_targets() -> bool {
    enabled()
}

/// In tests the slot can also be armed for ONE THREAD, without switching the whole trace on.
///
/// Switching `TRACE_STATE` on is process-wide, and the test binary runs many tests at once, so it
/// turns every other trace site on underneath whatever else is running. That is not merely noisy:
/// `interpreter::statements::native_method_execution` reads three words from the result cell
/// through `slice::from_raw_parts` behind `enabled()`, with no null check, so a concurrent test
/// whose native method returns a null cell aborts the whole binary with an unsafe-precondition
/// violation. Arming one thread keeps this test's blast radius to this test.
#[cfg(test)]
fn recording_include_targets() -> bool {
    enabled() || INCLUDE_TARGET_ARMED.with(std::cell::Cell::get)
}

#[cfg(test)]
thread_local! {
    /// Set by [`arm_include_target_for_test`] for the duration of one test.
    static INCLUDE_TARGET_ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Arms the include target slot on THIS THREAD for a test, leaving the process-wide verdict alone.
#[cfg(test)]
pub(crate) fn arm_include_target_for_test(armed: bool) {
    INCLUDE_TARGET_ARMED.with(|cell| cell.set(armed));
}

/// Forgets the cached verdict, so the next question re-reads the environment.
///
/// `putenv()` is the one way a running PHP program can change the answer.
pub(crate) fn invalidate() {
    TRACE_STATE.store(TRACE_UNKNOWN, Ordering::Relaxed);
}

/// Returns the cached verdict, reading the environment once when it is not known.
///
/// A race between two threads computing it is harmless: both read the same environment and store
/// the same value, so the slot converges without needing a lock.
fn state() -> u8 {
    let cached = TRACE_STATE.load(Ordering::Relaxed);
    if cached != TRACE_UNKNOWN {
        return cached;
    }
    let computed = verdict_for_level(std::env::var_os("ELEPHC_EVAL_TRACE").as_deref());
    TRACE_STATE.store(computed, Ordering::Relaxed);
    computed
}

#[cfg(test)]
mod tests {
    use super::*;

    // These test the verdict as pure functions: storing into the process-wide `TRACE_STATE`
    // raced the other test in this module and any interpreter test whose `putenv()` invalidates
    // it, and switched every trace site on underneath tests running in parallel.

    /// Verifies the level spellings map to their verdicts and what each verdict enables.
    #[test]
    fn levels_map_to_verdicts() {
        assert_eq!(verdict_for_level(None), TRACE_OFF);
        assert_eq!(verdict_for_level(Some(std::ffi::OsStr::new("1"))), TRACE_ON);
        assert_eq!(verdict_for_level(Some(std::ffi::OsStr::new("ffi"))), TRACE_ON_FFI);
        assert_eq!(verdict_for_level(Some(std::ffi::OsStr::new("all"))), TRACE_ON_FFI);
        assert!(!verdict_enables(TRACE_OFF));
        assert!(verdict_enables(TRACE_ON_FFI));
        assert!(verdict_enables_ffi(TRACE_ON_FFI));
    }

    /// Verifies an enabled trace that does not name `ffi` leaves the FFI-entry trace off.
    #[test]
    fn a_plain_level_does_not_enable_the_ffi_entry_trace() {
        assert!(verdict_enables(TRACE_ON));
        assert!(!verdict_enables_ffi(TRACE_ON));
    }

    /// Verifies an invalidation forgets the cached verdict.
    #[test]
    fn invalidate_forgets_the_verdict() {
        invalidate();
        // Another thread may recompute it at once; either way it is a real verdict or unknown.
        assert!(TRACE_STATE.load(Ordering::Relaxed) <= TRACE_ON_FFI);
    }
}
