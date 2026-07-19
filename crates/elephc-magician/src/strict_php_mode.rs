//! Purpose:
//! Owns the strict-PHP mode state for runtime eval: binaries compiled with
//! `elephc --strict-php` hide elephc-extension builtins from eval dispatch and
//! introspection so eval'd code behaves like the PHP interpreter.
//!
//! Called from:
//! - `crate::ffi::context::__elephc_eval_set_strict_php` (generated code sets
//!   the flag once while initializing the eval context).
//! - `crate::interpreter::builtins::registry::eval_declared_builtin_spec`
//!   (the single lookup choke point consults it on every resolution).
//!
//! Key details:
//! - Thread-local, mirroring the compiler's `strict_php` state: strictness is
//!   a property of the whole compiled binary, and elephc programs execute the
//!   setter and every eval fragment on one thread (fibers switch stacks, not
//!   OS threads), while parallel `cargo test` threads stay isolated.

use std::cell::Cell;

thread_local! {
    /// Whether the compiled binary embedding this bridge was built with `--strict-php`.
    static STRICT_PHP_MODE: Cell<bool> = const { Cell::new(false) };
}

/// Enables or disables strict-PHP mode for eval on the current thread.
pub(crate) fn set_strict_php_mode(enabled: bool) {
    STRICT_PHP_MODE.with(|cell| cell.set(enabled));
}

/// Returns whether strict-PHP mode is active for eval on the current thread.
pub(crate) fn strict_php_mode() -> bool {
    STRICT_PHP_MODE.with(|cell| cell.get())
}

/// RAII guard restoring the previous strict-mode state on drop.
///
/// Test fixtures hold one of these instead of calling `set_strict_php_mode` in
/// pairs, so a panicking assertion cannot leak strict state into later
/// fixtures on the same thread.
#[cfg(test)]
pub(crate) struct StrictModeGuard {
    previous: bool,
}

#[cfg(test)]
impl Drop for StrictModeGuard {
    /// Restores the strict-mode state captured when the guard was created.
    fn drop(&mut self) {
        set_strict_php_mode(self.previous);
    }
}

/// Enables strict mode and returns a guard that restores the previous state on drop.
#[cfg(test)]
pub(crate) fn scoped_enable() -> StrictModeGuard {
    let previous = strict_php_mode();
    set_strict_php_mode(true);
    StrictModeGuard { previous }
}
