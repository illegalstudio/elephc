//! Purpose:
//! Eval-interpreter implementations of the five OPcache file/script functions
//! `opcache_is_script_cached()`, `opcache_invalidate()`, `opcache_compile_file()`,
//! `opcache_is_script_cached_in_file_cache()`, and `opcache_jit_blacklist()`.
//!
//! These answer about the RUNTIME SCRIPT CACHE (`crate::script_cache`), the tier reached
//! from inside `eval()` — a dynamically included file is not in the binary, so it is read,
//! segmented and cached here rather than compiled at link time. That makes this the one
//! OPcache surface with a real cache behind it.
//!
//! The gate is `script_cache::config().enabled`, installed by generated code through
//! `__elephc_eval_configure_opcache`. It defaults to DISABLED, which is what the
//! compile-time const-folder observes — no generated code has run there, there is no cache,
//! and the pre-existing disabled answers below are the correct ones:
//! - `opcache_is_script_cached` → `false` when disabled; when enabled, whether the path has
//!   a live (present, non-discarded) entry in the runtime script cache.
//! - `opcache_invalidate` → `false` when disabled (reference PHP returns `false` for any
//!   path when OPcache is off, even an existing file); when enabled, php-src's
//!   `zend_accel_invalidate()` answer, "cached OR resolvable" — both halves, because a cached
//!   file that has since been DELETED no longer resolves and must still answer `true`.
//! - `opcache_compile_file` → `false` when disabled (reference also emits an `E_NOTICE`,
//!   which the eval const-folder has no channel for — see below); when enabled it reads,
//!   segments and caches the file WITHOUT executing it, as php-src compiles and stores
//!   without running. Only a file that cannot be OPENED warns, and with the real
//!   `strerror` text; one that opens but does not parse answers `false` quietly (reference
//!   throws a `ParseError`, the pinned divergence).
//! - `opcache_is_script_cached_in_file_cache` → whether a usable entry for the path is on
//!   disk under `opcache.file_cache` (`script_cache::file_store::contains`). php-src gates the
//!   whole body on that directive, which has a C NULL default, so an unconfigured build
//!   answers `false` for every path — VERIFIED on PHP 8.5.6.
//! - `opcache_jit_blacklist` → `NULL` (a `void` function; php-src's body only mutates the
//!   JIT blacklist behind `#ifdef HAVE_JIT`, so a no-op returning null is the whole
//!   observable behavior — VERIFIED on PHP 8.5.6).
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call` (direct dispatch).
//! - `crate::interpreter::builtins::registry::dispatch::eval_builtin_with_values`
//!   (dynamic-callable / by-values dispatch).
//! - `crate::interpreter::builtins::symbols::function_exists` (existence probe).
//!
//! Key details:
//! - All five are prelude-provided on the native side (real PHP functions whose bodies
//!   are baked from the compile-time enabled constant), NOT checker catalog builtins, so
//!   they must NOT be PHP-visible eval builtins either (that would break
//!   `builtin_parity_tests`, which require the two PHP-visible builtin sets to agree).
//!   They are dispatched as plain runtime handlers and made visible to `function_exists`
//!   through a small allowlist, exactly like `opcache_reset` / `opcache_get_status`.
//! - The enabled state is READ FROM THE INSTALLED CONFIGURATION rather than re-derived from
//!   the directive table with `is_web_sapi = false`. Re-deriving it was correct while these
//!   functions were terminal falses, but it cannot see `--web` or
//!   `--ini opcache.enable_cli=1`, and answering `false` there while the cache is actually
//!   serving includes would make the binary contradict itself. The compiler resolves the
//!   same predicate (`opcache::runtime_cache::runtime_cache_config`) and installs the
//!   result, so the two surfaces still share one source of truth.
//! - The state/directives files are still shared verbatim from `src/opcache/` via `#[path]`
//!   includes (as sibling modules so `state`'s `use super::directives` resolves).
//! - Arity, matching PHP and the native prelude signatures: `is_script_cached` takes
//!   exactly 1 argument, `invalidate` takes 1 or 2, `compile_file` takes exactly 1,
//!   `is_script_cached_in_file_cache` takes exactly 1, and `jit_blacklist` takes exactly 1
//!   (VERIFIED on PHP 8.5.6 via `ReflectionFunction`: one required parameter named
//!   `closure`, typed `Closure`, return type `void`). Anything else is a runtime fatal.
//! - `opcache_compile_file`'s disabled `E_NOTICE`: the eval interpreter is a compile-time
//!   const-folder with no notice-level diagnostic channel (`RuntimeValueOps` exposes only
//!   a warning sink, at the wrong severity), and a const-fold should not synthesize a
//!   runtime notice anyway. Eval therefore returns `false` with no diagnostic; the notice
//!   is the native runtime's responsibility (rendered to STDERR by the prelude body). This
//!   mirrors how the sibling `opcache_reset` / `opcache_get_status` eval handlers are pure
//!   value producers.

use super::*;

// `state` only reads `opcache_directives`/`DirectiveValue` from this shared file, so the
// version-string/product-name items are unused in this crate; they are exercised by the
// native `opcache_get_configuration` copy and by this file's own tests.
#[path = "../../../../../../src/opcache/directives.rs"]
#[allow(dead_code)]
mod directives;

#[path = "../../../../../../src/opcache/state.rs"]
mod state;

#[allow(unused_imports)]
use state::opcache_cache_enabled;

/// `opcache.restrict_api`'s refusal, when it applies: the warning reference prints, then `false`.
///
/// php-src's restricted functions call `validate_api_restriction()` FIRST, before any cache or
/// enable check, and answer `false` after the warning. The injected native bodies bake that
/// verdict — but an OPcache call the compiler could not see, such as one in a `eval()` source
/// read at run time, gets no native body and landed here, where nothing checked. It scheduled a
/// real restart. MEASURED with `restrict_api=/nonexistent` and `eval(getenv(...))` running
/// `var_dump(opcache_reset());`: reference warns and prints `bool(false)`; elephc printed
/// `bool(true)` in silence. Reported by the PR review.
///
/// Every spelling reaches one of the six handlers that call this — the direct call, the
/// by-values dispatch behind `call_user_func`, `$f()` and `call_user_func_array` — so the
/// refusal does not depend on how the name was written.
pub(in crate::interpreter) fn eval_opcache_api_refusal(
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !crate::script_cache::config().api_restricted {
        return Ok(None);
    }
    values.warning(
        "Warning: Zend OPcache API is restricted by \"restrict_api\" configuration directive\n",
    )?;
    Ok(Some(values.bool_value(false)?))
}

/// Returns whether the runtime script cache is serving this process.
///
/// Mirrors `opcache_cache_enabled` for the binary that installed it, and defaults to
/// `false`, which is what the compile-time const-folder observes.
fn eval_opcache_cache_enabled() -> bool {
    crate::script_cache::config().enabled
}

/// Evaluates one call argument to an owned filesystem path.
///
/// A non-UTF-8 path is a runtime fatal rather than a silent miss, matching how every other
/// path surface in this interpreter reads its argument.
fn eval_opcache_path_arg(
    arg: &EvalCallArg,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<std::path::PathBuf, EvalStatus> {
    let value = eval_expr(arg.value(), context, scope, values)?;
    let bytes = values.string_bytes(value)?;
    let text = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(std::path::PathBuf::from(text))
}

/// Reads an ALREADY-EVALUATED call argument as a filesystem path.
///
/// The by-values dispatch path receives evaluated handles rather than expressions; it must
/// reach the same answer as a direct call, so it resolves its path the same way.
pub(in crate::interpreter) fn eval_opcache_path_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<std::path::PathBuf, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let text = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(std::path::PathBuf::from(text))
}

/// The `opcache_is_script_cached()` answer for a resolved path.
pub(in crate::interpreter) fn eval_opcache_is_script_cached_for_path(
    path: &std::path::Path,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !eval_opcache_cache_enabled() {
        return eval_opcache_file_disabled_result(values);
    }
    values.bool_value(crate::script_cache::is_cached(path))
}

/// The `opcache_invalidate()` answer for a resolved path.
///
/// WHETHER TO EVICT IS THE CACHE'S QUESTION, not this function's: it is php-src's
/// `force || !validate_timestamps || timestamps_failed`, and it needs the configuration and
/// the entry's recorded mtime to answer. This asked only `if forced`, which is one third of
/// it. `script_cache::invalidate` owns the predicate now, next to the state it reads.
///
/// WHAT TO RETURN stays here, because it is a PHP-visible convention rather than a cache
/// fact: reference answers whether the PATH RESOLVES, not whether anything was dropped, so
/// a live path whose entry survives the predicate still answers `true`.
pub(in crate::interpreter) fn eval_opcache_invalidate_for_path(
    path: &std::path::Path,
    forced: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !eval_opcache_cache_enabled() {
        return eval_opcache_invalidate_result(values);
    }
    // "CACHED OR RESOLVABLE", composed the way the native surface composes it. The answer
    // used to be the right-hand side alone, on the reasoning that a cached path was
    // canonicalized when it was stored and so always resolves — which is false for exactly
    // one case, a cached file that was DELETED since, and that is the case an invalidate most
    // often exists for. The eviction is what knows the entry was there. MEASURED, reached
    // only through a computed name so no native declaration is injected: `include` a file,
    // `unlink()` it, invalidate it — reference answers `true`, this answered `false`, while
    // the native prelude already answered `true` for the same file in the same process.
    let evicted = crate::script_cache::invalidate(path, forced);
    values.bool_value(evicted || eval_opcache_path_resolves(path))
}

/// The `opcache_compile_file()` answer for a resolved path.
///
/// A file that cannot be opened is not silent: reference PHP emits the SAME PAIR of warnings
/// an `include` of a missing file does, naming `opcache_compile_file` as the construct
/// (VERIFIED on PHP 8.5.6, which prints both before returning `false`). elephc reproduces
/// both, without the ` in <file> on line <n>` suffix it does not synthesize anywhere.
pub(in crate::interpreter) fn eval_opcache_compile_file_for_path(
    path: &std::path::Path,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !eval_opcache_cache_enabled() {
        return eval_opcache_compile_file_result(values);
    }
    if crate::script_cache::compile_file(path) {
        return values.bool_value(true);
    }
    // `compile_file` answers `false` for two different failures, and only one of them is a
    // missing file. A file that OPENS but does not parse is the pinned divergence —
    // reference throws a `ParseError` — and printing "No such file or directory" for it named
    // the wrong failure, on this surface only: the native wrapper warns from its
    // `realpath()`-failure arm and answers a parse failure in silence. Warn only when the file
    // really cannot be opened.
    //
    // And the warning names the ACTUAL open failure — `strerror` text, as php-src's stream
    // layer prints it — not always "No such file or directory". MEASURED as a non-root user on
    // a mode-`0000` file: reference says `Permission denied`; this said the file was missing.
    let Some(reason) = crate::script_cache::store::compile_open_failure(path) else {
        return values.bool_value(false);
    };
    let display = path.display().to_string();
    for warning in crate::script_cache::store::compile_open_failure_warnings(&display, &reason) {
        values.warning(&warning)?;
    }
    values.bool_value(false)
}

/// Returns whether a path RESOLVES, which is what `opcache_invalidate()` reports.
///
/// The empty string is php's `realpath('')` case: it resolves to the current working
/// directory, where `std::fs::canonicalize("")` reports an error instead.
fn eval_opcache_path_resolves(path: &std::path::Path) -> bool {
    if path.as_os_str().is_empty() {
        return std::env::current_dir().is_ok();
    }
    std::fs::canonicalize(path).is_ok()
}

/// Returns whether `name` (already lowercased and unqualified) is one of the five OPcache
/// file/script functions, so `function_exists` reports it as existing even though none is a
/// PHP-visible eval builtin.
pub(in crate::interpreter) fn eval_opcache_file_function_exists(name: &str) -> bool {
    matches!(
        name,
        "opcache_is_script_cached"
            | "opcache_invalidate"
            | "opcache_compile_file"
            | "opcache_is_script_cached_in_file_cache"
            | "opcache_jit_blacklist"
    )
}

/// Builds the shared disabled/CLI-default result: the OPcache cache is off under eval, so
/// all three file functions return `false`. Derived from the shared state function so it
/// tracks the directive table rather than a hard-coded literal.
fn eval_opcache_file_disabled_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let enabled = opcache_cache_enabled(crate::eval_php_profile::eval_php_version_id(), false);
    // Disabled cache (the eval/CLI default): every file function's correct result is
    // `false` — nothing is cached, invalidate reports not-found, compile cannot run.
    debug_assert!(!enabled, "eval reports the CLI default, which is disabled");
    values.bool_value(enabled)
}

/// Evaluates a direct `opcache_is_script_cached($filename)` call from an eval fragment.
/// Exactly one argument is required; the argument does not change the empty-cache result.
pub(in crate::interpreter) fn eval_opcache_is_script_cached_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if let Some(refused) = eval_opcache_api_refusal(values)? {
        return Ok(refused);
    }
    if !eval_opcache_cache_enabled() {
        return eval_opcache_file_disabled_result(values);
    }
    let path = eval_opcache_path_arg(filename, context, scope, values)?;
    eval_opcache_is_script_cached_for_path(&path, values)
}

/// Evaluates a direct `opcache_invalidate($filename, $force = false)` call from an eval
/// fragment. One or two arguments are accepted; under the disabled eval cache the result
/// is `false` regardless of the path or the `$force` flag.
pub(in crate::interpreter) fn eval_opcache_invalidate_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(refused) = eval_opcache_api_refusal(values)? {
        return Ok(refused);
    }
    if !eval_opcache_cache_enabled() {
        return eval_opcache_invalidate_result(values);
    }
    let path = eval_opcache_path_arg(&args[0], context, scope, values)?;
    let forced = match args.get(1) {
        Some(force) => {
            let value = eval_expr(force.value(), context, scope, values)?;
            values.truthy(value)?
        }
        None => false,
    };
    eval_opcache_invalidate_for_path(&path, forced, values)
}

/// Builds the `opcache_invalidate()` return value: `false` (disabled eval cache).
pub(in crate::interpreter) fn eval_opcache_invalidate_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_opcache_file_disabled_result(values)
}

/// Evaluates a direct `opcache_compile_file($filename)` call from an eval fragment.
/// Exactly one argument is required. Under the disabled eval cache the result is `false`;
/// the reference `E_NOTICE` is not synthesized here (see the module docblock).
pub(in crate::interpreter) fn eval_opcache_compile_file_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if !eval_opcache_cache_enabled() {
        return eval_opcache_compile_file_result(values);
    }
    let path = eval_opcache_path_arg(filename, context, scope, values)?;
    eval_opcache_compile_file_checked(&path, context, values)
}

/// `opcache_compile_file()` for an already-evaluated path, with php-src's argument check.
///
/// AN EMPTY PATH THROWS `ValueError: Path must not be empty` — but only once the cache is
/// enabled, because reference raises it from the open, after its "not properly started"
/// notice. MEASURED: enabled, reference throws; disabled, it prints the notice and answers
/// `false`. Both eval spellings — the direct call and the by-values dispatch — come through
/// here, so the check exists once.
pub(in crate::interpreter) fn eval_opcache_compile_file_checked(
    path: &std::path::Path,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_opcache_cache_enabled() && path.as_os_str().is_empty() {
        return crate::interpreter::throwables::eval_throw_builtin_value_error(
            "Path must not be empty",
            context,
            values,
        );
    }
    eval_opcache_compile_file_for_path(path, values)
}

/// Builds the `opcache_compile_file()` return value: `false` (disabled eval cache, no
/// runtime compiler). No notice is emitted (the eval const-folder has no notice channel).
pub(in crate::interpreter) fn eval_opcache_compile_file_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_opcache_file_disabled_result(values)
}

/// Evaluates a direct `opcache_is_script_cached_in_file_cache($filename)` call from an eval
/// fragment. Exactly one argument is required; the path does not change the result, because
/// reference PHP gates the whole body on `opcache.file_cache` being set and that directive
/// is registered with a C NULL default — so an unconfigured reference PHP answers `false`
/// for every path (VERIFIED on PHP 8.5.6), and elephc has no file cache at all.
pub(in crate::interpreter) fn eval_opcache_is_script_cached_in_file_cache_call(
    args: &[EvalCallArg],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() != 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(refused) = eval_opcache_api_refusal(values)? {
        return Ok(refused);
    }
    let path = eval_opcache_path_arg(&args[0], _context, _scope, values)?;
    eval_opcache_is_script_cached_in_file_cache_for_path(&path, values)
}

/// The `opcache_is_script_cached_in_file_cache()` answer for a resolved path.
///
/// Answers from the ON-DISK cache, not the in-memory one: php-src's function asks whether
/// the script is in the FILE cache specifically, and the two can legitimately disagree — a
/// script can sit in memory without a disk entry, or on disk without having been included
/// in this process yet.
///
/// It applies exactly the validation a read does, so it never reports an entry that a read
/// would then reject. With `opcache.file_cache` unset — php-src's default — there is no
/// directory to look in and the answer is `false`, which is what reference PHP returns too.
pub(in crate::interpreter) fn eval_opcache_is_script_cached_in_file_cache_for_path(
    path: &std::path::Path,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !eval_opcache_cache_enabled() {
        return eval_opcache_file_disabled_result(values);
    }
    values.bool_value(crate::script_cache::file_cache_contains(path))
}


/// Evaluates a direct `opcache_jit_blacklist($closure)` call from an eval fragment. Exactly
/// one argument is required (VERIFIED on PHP 8.5.6: one required parameter named `closure`,
/// typed `Closure`). The function is declared `void`, so the call evaluates to PHP `NULL`.
pub(in crate::interpreter) fn eval_opcache_jit_blacklist_call(
    args: &[EvalCallArg],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() != 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_opcache_jit_blacklist_result(values)
}

/// Builds the `opcache_jit_blacklist()` return value: PHP `NULL`. Reference PHP only mutates
/// the JIT blacklist behind `#ifdef HAVE_JIT` and returns `void`, which `var_export`s as
/// `NULL` (VERIFIED on PHP 8.5.6); elephc has no JIT, so the no-op is the whole behavior.
pub(in crate::interpreter) fn eval_opcache_jit_blacklist_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.null()
}
