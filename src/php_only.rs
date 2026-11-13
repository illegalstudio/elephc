//! Purpose:
//! Owns the `--php-only` compilation mode: the thread-local enablement state
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
//! - PHP-only mode must never hide `internal: true` builtins: injected compiler
//!   preludes call them, and they are already invisible to user programs.

mod audit;

pub use audit::check;

use std::cell::Cell;

thread_local! {
    /// Whether `--php-only` was requested for the compilation running on this thread.
    static PHP_ONLY: Cell<bool> = const { Cell::new(false) };

    /// Language mode of the physical source currently being resolved or checked.
    static SOURCE_MODE: Cell<crate::source::SourceMode> =
        const { Cell::new(crate::source::SourceMode::Php) };
}

/// Enables or disables PHP-only mode for the current thread's compilation.
pub fn set_enabled(enabled: bool) {
    PHP_ONLY.with(|cell| cell.set(enabled));
}

/// Returns whether PHP-only was requested for the current thread's compilation.
pub fn is_requested() -> bool {
    PHP_ONLY.with(|cell| cell.get())
}

/// Returns whether PHP-only is effective for the source currently being processed.
pub fn is_enabled() -> bool {
    SOURCE_MODE.with(|mode| mode.get().php_only_is_effective(is_requested()))
}

/// RAII guard restoring the previous physical-source mode on drop.
struct SourceModeGuard {
    previous: crate::source::SourceMode,
}

impl Drop for SourceModeGuard {
    /// Restores the source mode that was active before the nested operation.
    fn drop(&mut self) {
        SOURCE_MODE.with(|cell| cell.set(self.previous));
    }
}

/// Runs one source-sensitive compiler operation under the physical file's mode.
///
/// Nested calls restore their caller's mode even when the operation unwinds.
pub fn with_source_mode<T>(
    mode: crate::source::SourceMode,
    operation: impl FnOnce() -> T,
) -> T {
    let previous = SOURCE_MODE.with(|cell| cell.replace(mode));
    let _guard = SourceModeGuard { previous };
    operation()
}

/// RAII guard restoring the previous PHP-only state on drop.
///
/// Test fixtures that enable PHP-only mode must hold one of these instead of
/// calling `set_enabled` in pairs: if an assertion panics mid-fixture, the
/// guard's `Drop` still runs during unwinding, so later fixtures on the same
/// thread (e.g. the examples corpus loop) can never inherit stale PHP-only state.
///
/// Constructed only by lib unit tests and integration tests; the compiler
/// binary sets the flag once from the CLI and never scopes it, hence the
/// dead-code allowance for the bin target.
#[allow(dead_code)]
pub struct PhpOnlyGuard {
    previous: bool,
}

impl Drop for PhpOnlyGuard {
    /// Restores the PHP-only state captured when the guard was created.
    fn drop(&mut self) {
        set_enabled(self.previous);
    }
}

/// Enables PHP-only mode and returns a guard that restores the previous state on drop.
///
/// Used by lib unit tests and integration tests only (see `PhpOnlyGuard`).
#[allow(dead_code)]
pub fn scoped_enable() -> PhpOnlyGuard {
    let previous = is_requested();
    set_enabled(true);
    PhpOnlyGuard { previous }
}

/// Audits one user source file's parsed statements when PHP-only mode is active.
///
/// No-op (always `Ok`) when PHP-only mode is off. Violations are bundled into a
/// single `CompileError` (primary + related) attributed to `file`, so callers
/// that parse user files mid-pipeline — the include resolver and the autoloader
/// — can propagate every violation through their existing single-error paths.
/// Every call site runs it on the parsed AST right after magic-constant
/// substitution (a PHP-compatible rewrite) and before any pass synthesizes
/// compiler-internal names or nodes into it.
#[allow(dead_code)]
pub fn check_file(
    program: &crate::parser::ast::Program,
    file: &str,
) -> Result<(), crate::errors::CompileError> {
    check_file_with_mode(program, file, crate::source::SourceMode::Php)
}

/// Audits one physical user source file under its path-selected language mode.
///
/// An invocation-level strict request applies only to PHP-mode source. LFC and
/// compiler-internal source always retain elephc extensions.
pub fn check_file_with_mode(
    program: &crate::parser::ast::Program,
    file: &str,
    mode: crate::source::SourceMode,
) -> Result<(), crate::errors::CompileError> {
    if !mode.php_only_is_effective(is_requested()) {
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

    /// Verifies PHP-only mode defaults to off so ordinary compiles are unaffected.
    #[test]
    fn strict_mode_defaults_off() {
        assert!(!is_enabled());
    }

    /// Verifies enabling and disabling PHP-only mode round-trips on the same thread.
    #[test]
    fn strict_mode_set_and_clear_roundtrip() {
        set_enabled(true);
        assert!(is_enabled());
        assert!(!with_source_mode(crate::source::SourceMode::Lfc, is_enabled));
        assert!(is_enabled());
        set_enabled(false);
        assert!(!is_enabled());
    }

    /// Verifies the RAII guard restores the previous state on drop, including
    /// when dropped during a panic unwind, so panicking fixtures cannot leak
    /// PHP-only state to later fixtures on the same thread.
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

    /// Verifies PHP-only mode is thread-local: enabling it on another thread must
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
