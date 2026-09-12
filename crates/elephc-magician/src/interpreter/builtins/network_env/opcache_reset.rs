//! Purpose:
//! Eval-interpreter implementation of `opcache_reset()`. Returns the compile-time
//! cache-enabled boolean, mirroring reference PHP where `opcache_reset()` yields
//! `false` when the cache is disabled and `true` when enabled.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call` (direct dispatch).
//! - `crate::interpreter::builtins::registry::dispatch::eval_builtin_with_values`
//!   (dynamic-callable / by-values dispatch).
//! - `crate::interpreter::builtins::symbols::function_exists` (existence probe).
//!
//! Key details:
//! - `opcache_reset` is prelude-provided on the native side (a real PHP function whose
//!   body is the compile-time enabled constant), NOT a checker catalog builtin, so it
//!   must NOT be a PHP-visible eval builtin either (that would break
//!   `builtin_parity_tests`, which require the two PHP-visible builtin sets to agree).
//!   It is therefore dispatched as a plain runtime handler and made visible to
//!   `function_exists` through a small allowlist, exactly like
//!   `opcache_get_configuration`.
//! - The eval interpreter has no runtime SAPI: it evaluates at compile time and most
//!   use is const-folding, so it reports the CLI default. That default is derived from
//!   the shared `opcache_cache_enabled` state function (with `is_web_sapi = false`)
//!   rather than hard-coded, so it tracks the directive table verbatim — for the CLI
//!   default that is `opcache.enable_cli` (false).
//! - The state derivation is shared verbatim from `src/opcache/state.rs` (and its
//!   `src/opcache/directives.rs` dependency) via `#[path]` includes, mirroring how
//!   `opcache_get_configuration.rs` shares the directive table, so there is a single
//!   source of truth across the two crates with no drift. The two files are included as
//!   sibling modules here so `state`'s `use super::directives` resolves to `directives`.

use super::*;

// `state` only reads `opcache_directives`/`DirectiveValue` from this shared file, so the
// version-string/product-name items are unused in this crate; they are exercised by the
// native `opcache_get_configuration` copy and by this file's own tests.
#[path = "../../../../../../src/opcache/directives.rs"]
#[allow(dead_code)]
mod directives;

#[path = "../../../../../../src/opcache/state.rs"]
mod state;

use state::opcache_cache_enabled;

/// Returns whether `name` (already lowercased and unqualified) is the OPcache reset
/// function, so `function_exists` reports it as existing even though it is not a
/// PHP-visible eval builtin.
pub(in crate::interpreter) fn eval_opcache_reset_function_exists(name: &str) -> bool {
    name == "opcache_reset"
}

/// Evaluates a direct `opcache_reset()` call from an eval fragment.
pub(in crate::interpreter) fn eval_opcache_reset_call(
    args: &[EvalCallArg],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_opcache_reset_result(values)
}

/// Builds the `opcache_reset()` return value.
///
/// Disabled → `false`, which is both reference PHP's answer and the compile-time
/// const-folder's state (no generated code has installed a configuration there).
/// Enabled → schedules the runtime script cache's restart: `true` on the FIRST call and
/// `false` on every call after it, exactly as php-src's `zend_accel_schedule_restart()`
/// clears the flag `opcache_reset()`'s own guard tests.
pub(in crate::interpreter) fn eval_opcache_reset_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !crate::script_cache::config().enabled {
        let enabled = opcache_cache_enabled(crate::eval_php_profile::eval_php_version_id(), false);
        debug_assert!(!enabled, "an uninstalled configuration means the CLI default");
        return values.bool_value(enabled);
    }
    values.bool_value(crate::script_cache::schedule_restart())
}
