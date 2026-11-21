//! Purpose:
//! Owns the PHP-only mode state for runtime eval: binaries compiled with
//! `elephc --php-only` hide elephc-extension builtins from eval dispatch and
//! introspection so eval'd code behaves like the PHP interpreter.
//!
//! Called from:
//! - `crate::ffi::context::__elephc_eval_set_php_only` (generated code sets
//!   the flag once while initializing the eval context).
//! - `crate::interpreter::builtins::registry::eval_declared_builtin_spec`
//!   (the single lookup choke point consults it on every resolution).
//!
//! Key details:
//! - Thread-local, mirroring the compiler's `php_only` state: extension visibility is
//!   a property of the whole compiled binary, and elephc programs execute the
//!   setter and every eval fragment on one thread (fibers switch stacks, not
//!   OS threads), while parallel `cargo test` threads stay isolated.

use std::cell::Cell;

thread_local! {
    /// Whether the compiled binary embedding this bridge was built with `--php-only`.
    static PHP_ONLY_MODE: Cell<bool> = const { Cell::new(false) };
}

/// Enables or disables PHP-only mode for eval on the current thread.
pub(crate) fn set_php_only_mode(enabled: bool) {
    PHP_ONLY_MODE.with(|cell| cell.set(enabled));
}

/// Returns whether PHP-only mode is active for eval on the current thread.
pub(crate) fn php_only_mode() -> bool {
    PHP_ONLY_MODE.with(|cell| cell.get())
}

/// RAII guard restoring the previous PHP-only state on drop.
///
/// Test fixtures hold one of these instead of calling `set_php_only_mode` in
/// pairs, so a panicking assertion cannot leak PHP-only state into later
/// fixtures on the same thread.
#[cfg(test)]
pub(crate) struct PhpOnlyGuard {
    previous: bool,
}

#[cfg(test)]
impl Drop for PhpOnlyGuard {
    /// Restores the PHP-only state captured when the guard was created.
    fn drop(&mut self) {
        set_php_only_mode(self.previous);
    }
}

/// Enables PHP-only mode and returns a guard that restores the previous state on drop.
#[cfg(test)]
pub(crate) fn scoped_enable() -> PhpOnlyGuard {
    let previous = php_only_mode();
    set_php_only_mode(true);
    PhpOnlyGuard { previous }
}
