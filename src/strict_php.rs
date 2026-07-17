//! Purpose:
//! Owns the `--strict-php` compilation mode: the thread-local enablement state
//! consulted by the builtin catalog and checker, and (in later sections) the
//! AST audit pass that rejects elephc-only syntax extensions.
//!
//! Called from:
//! - `crate::pipeline::compile()` (sets the mode from the CLI flag).
//! - `crate::types::checker::builtins::catalog` (filters extension builtins).
//! - Test helpers that drive compiler phases in-process.
//!
//! Key details:
//! - The state is a thread-local `Cell`, mirroring `codegen_support`'s
//!   `AUTOLOAD_RULE_COUNT` precedent, so parallel test runs cannot interfere
//!   (the compile pipeline runs on a single thread per invocation).
//! - Strict mode must never hide `internal: true` builtins: injected compiler
//!   preludes call them, and they are already invisible to user programs.

mod audit;

pub use audit::check;

use std::cell::Cell;

thread_local! {
    /// Whether `--strict-php` is active for the compilation running on this thread.
    static STRICT_PHP: Cell<bool> = const { Cell::new(false) };
}

/// Enables or disables strict-PHP mode for the current thread's compilation.
pub fn set_enabled(enabled: bool) {
    STRICT_PHP.with(|cell| cell.set(enabled));
}

/// Returns whether strict-PHP mode is active for the current thread's compilation.
pub fn is_enabled() -> bool {
    STRICT_PHP.with(|cell| cell.get())
}

/// RAII guard restoring the previous strict-mode state on drop.
///
/// Test fixtures that enable strict mode must hold one of these instead of
/// calling `set_enabled` in pairs: if an assertion panics mid-fixture, the
/// guard's `Drop` still runs during unwinding, so later fixtures on the same
/// thread (e.g. the examples corpus loop) can never inherit stale strict state.
///
/// Constructed only by lib unit tests and integration tests; the compiler
/// binary sets the flag once from the CLI and never scopes it, hence the
/// dead-code allowance for the bin target.
#[allow(dead_code)]
pub struct StrictModeGuard {
    previous: bool,
}

impl Drop for StrictModeGuard {
    /// Restores the strict-mode state captured when the guard was created.
    fn drop(&mut self) {
        set_enabled(self.previous);
    }
}

/// Enables strict mode and returns a guard that restores the previous state on drop.
///
/// Used by lib unit tests and integration tests only (see `StrictModeGuard`).
#[allow(dead_code)]
pub fn scoped_enable() -> StrictModeGuard {
    let previous = is_enabled();
    set_enabled(true);
    StrictModeGuard { previous }
}

/// Audits one user source file's parsed statements when strict mode is active.
///
/// No-op (always `Ok`) when strict mode is off. Violations are bundled into a
/// single `CompileError` (primary + related) attributed to `file`, so callers
/// that parse user files mid-pipeline — the include resolver and the autoloader
/// — can propagate every violation through their existing single-error paths.
/// Every call site runs it on the parsed AST right after magic-constant
/// substitution (a PHP-compatible rewrite) and before any pass synthesizes
/// compiler-internal names or nodes into it.
pub fn check_file(
    program: &crate::parser::ast::Program,
    file: &str,
) -> Result<(), crate::errors::CompileError> {
    if !is_enabled() {
        return Ok(());
    }
    let violations = check(program);
    if violations.is_empty() {
        return Ok(());
    }
    Err(crate::errors::CompileError::from_many(violations).with_file(file.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies strict mode defaults to off so ordinary compiles are unaffected.
    #[test]
    fn strict_mode_defaults_off() {
        assert!(!is_enabled());
    }

    /// Verifies enabling and disabling strict mode round-trips on the same thread.
    #[test]
    fn strict_mode_set_and_clear_roundtrip() {
        set_enabled(true);
        assert!(is_enabled());
        set_enabled(false);
        assert!(!is_enabled());
    }

    /// Verifies the RAII guard restores the previous state on drop, including
    /// when dropped during a panic unwind, so panicking fixtures cannot leak
    /// strict state to later fixtures on the same thread.
    #[test]
    fn scoped_enable_restores_on_drop_and_panic() {
        {
            let _guard = scoped_enable();
            assert!(is_enabled());
        }
        assert!(!is_enabled());

        let unwind = std::panic::catch_unwind(|| {
            let _guard = scoped_enable();
            panic!("fixture failure");
        });
        assert!(unwind.is_err());
        assert!(!is_enabled(), "guard must restore state during unwinding");
    }

    /// Verifies strict mode is thread-local: enabling it on another thread must
    /// not leak into this thread's compilation state.
    #[test]
    fn strict_mode_is_thread_local() {
        let handle = std::thread::spawn(|| {
            set_enabled(true);
            is_enabled()
        });
        assert!(handle.join().expect("thread must not panic"));
        assert!(!is_enabled());
    }
}
