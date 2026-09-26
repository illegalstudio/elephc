//! Purpose:
//! Eval-interpreter implementation of `opcache_get_status()`. Mirrors reference PHP:
//! it returns `false` when the OPcache *cache* is disabled and the status array when
//! enabled. The eval interpreter has no runtime SAPI and evaluates at compile time
//! (CLI default), where the cache is disabled — so eval always returns `false`, exactly
//! like `php script.php`.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call` (direct dispatch).
//! - `crate::interpreter::builtins::registry::dispatch::eval_builtin_with_values`
//!   (dynamic-callable / by-values dispatch).
//! - `crate::interpreter::builtins::symbols::function_exists` (existence probe).
//!
//! Key details:
//! - `opcache_get_status` is prelude-provided on the native side (a real PHP function
//!   whose body is baked from the compile-time enabled constant + directive table), NOT
//!   a checker catalog builtin, so it must NOT be a PHP-visible eval builtin either
//!   (that would break `builtin_parity_tests`, which require the two PHP-visible builtin
//!   sets to agree). It is therefore dispatched as a plain runtime handler and made
//!   visible to `function_exists` through a small allowlist, exactly like
//!   `opcache_reset` and `opcache_get_configuration`.
//! - It takes an optional `bool $include_scripts = true` argument. Because eval always
//!   reports the disabled cache (returns `false` before any array is built), the
//!   argument does not change the result; 0 or 1 arguments are accepted and anything
//!   more is a runtime fatal, matching PHP's arity.
//! - The cache-enabled state is derived from the shared `opcache_cache_enabled` state
//!   function (with `is_web_sapi = false`) rather than hard-coded, so it tracks the
//!   directive table verbatim — for the CLI default that is `opcache.enable_cli`
//!   (false). The state/directives files are shared verbatim from `src/opcache/` via
//!   `#[path]` includes (as sibling modules so `state`'s `use super::directives`
//!   resolves), giving a single source of truth across the two crates with no drift.

use super::*;

// `state` only reads `opcache_directives`/`DirectiveValue` from this shared file, so the
// version-string/product-name items are unused in this crate; they are exercised by the
// native `opcache_get_configuration` copy and by this file's own dispatch.
#[path = "../../../../../../src/opcache/directives.rs"]
#[allow(dead_code)]
mod directives;

#[path = "../../../../../../src/opcache/state.rs"]
mod state;

use state::opcache_cache_enabled;

/// Returns whether `name` (already lowercased and unqualified) is the OPcache status
/// function, so `function_exists` reports it as existing even though it is not a
/// PHP-visible eval builtin.
pub(in crate::interpreter) fn eval_opcache_get_status_function_exists(name: &str) -> bool {
    name == "opcache_get_status"
}

/// Evaluates a direct `opcache_get_status()` call from an eval fragment. The optional
/// `$include_scripts` argument is accepted (0 or 1 args) but does not change the result,
/// since eval reports the disabled cache.
pub(in crate::interpreter) fn eval_opcache_get_status_call(
    args: &[EvalCallArg],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_opcache_get_status_result(values)
}

/// Builds the `opcache_get_status()` return value. The eval interpreter has no runtime
/// SAPI, so it uses the CLI default (`is_web_sapi = false`), where the cache is disabled
/// and reference PHP returns `false` (`php script.php`). When the cache is disabled the
/// entire correct behavior is to return `false`, so no status array is materialized.
pub(in crate::interpreter) fn eval_opcache_get_status_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(refused) = super::opcache_file_functions::eval_opcache_api_refusal(values)? {
        return Ok(refused);
    }
    let enabled = opcache_cache_enabled(crate::eval_php_profile::eval_php_version_id(), false);
    // Disabled cache (the eval/CLI default) → `false`, the complete correct result.
    // An enabled status array is only reachable on the native `--web` prelude, never in
    // eval, so there is nothing else to build here.
    debug_assert!(!enabled, "eval reports the CLI default, which is disabled");
    values.bool_value(enabled)
}
