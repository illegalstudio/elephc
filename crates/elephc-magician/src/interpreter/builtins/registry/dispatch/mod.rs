//! Purpose:
//! Routes by-value dynamic builtin dispatch through declarative registry lookup
//! and eval-only runtime alias fallbacks.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Migrated builtins dispatch through `eval_declared_builtin_values_call`.
//! - Procedural date/time aliases remain a runtime fallback because eval cannot
//!   run the static name-resolver rewrite before dispatch.

use super::eval_declared_builtin_values_call;
use super::super::super::*;

/// Evaluates PHP-visible builtins when they are invoked through a dynamic callable name.
pub(in crate::interpreter) fn eval_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(result) = eval_declared_builtin_values_call(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }

    // THE SAME GUARD THE DIRECT CALL PATH CARRIES. When the binary declares these names
    // itself — which it does whenever the OPcache prelude is injected — that declaration is
    // the only thing that knows the resolved directives, the live cache, and
    // `opcache.restrict_api`. Intercepting ahead of it does not merely answer with stale
    // data: it answers INSTEAD of the refusal.
    //
    // `expressions/calls.rs` was fixed for the direct spelling and this path was not, so the
    // refusal survived `opcache_reset()` and `eval('opcache_reset()')` and did not survive
    // `eval('call_user_func("opcache_reset")')` — which returned `true`, emitted no warning,
    // and scheduled a real flush under `opcache.restrict_api=/nonexistent`. A guard with a
    // second way in is not a guard.
    //
    // Three spellings reach here: `call_user_func`, and — as a last resort in
    // `eval_callable_with_call_array_args` — a variable call `$f()` and
    // `call_user_func_array`, which used to fatal on an unsupported construct before
    // arriving. Guarding the whole block rather than the one name keeps the next spelling
    // from needing its own discovery.
    if context.native_function(name).is_none() {
    // `opcache_get_configuration` is prelude-provided on native and dispatched here as
    // a plain runtime handler (not a PHP-visible builtin); it takes no arguments.
    if name == "opcache_get_configuration" {
        if !evaluated_args.is_empty() {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(Some(eval_opcache_get_configuration_result(values)?));
    }

    // `opcache_reset` is prelude-provided on native and dispatched here as a plain
    // runtime handler (not a PHP-visible builtin); it takes no arguments and returns
    // the CLI-default cache-enabled boolean.
    if name == "opcache_reset" {
        if !evaluated_args.is_empty() {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(Some(eval_opcache_reset_result(values)?));
    }

    // `opcache_get_status` is prelude-provided on native and dispatched here as a plain
    // runtime handler (not a PHP-visible builtin); it takes an optional `$include_scripts`
    // argument and returns the CLI-default result (cache disabled → `false`).
    if name == "opcache_get_status" {
        if evaluated_args.len() > 1 {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(Some(eval_opcache_get_status_result(values)?));
    }

    // The five OPcache file/script functions are prelude-provided on native and dispatched
    // here as plain runtime handlers (not PHP-visible builtins). Three of them answer about
    // the runtime script cache, so they resolve their path and go through the SAME core the
    // direct call handlers use — `call_user_func('opcache_is_script_cached', $f)` must not
    // disagree with `opcache_is_script_cached($f)`. `opcache_is_script_cached_in_file_cache`
    // joins them: this comment used to call it terminal because "the file cache does not
    // exist", which this branch made untrue by shipping one, and the arm went on answering a
    // constant while its direct twin read the disk. MEASURED with an entry actually on disk
    // and the name computed at runtime: the direct call answered `1`, `call_user_func`
    // answered `0`, reference answers `1` to both. The one remaining terminal arm is the
    // `void` `opcache_jit_blacklist`, which yields `NULL`.
    if name == "opcache_is_script_cached" {
        if let Some(refused) = eval_opcache_api_refusal(values)? {
            return Ok(Some(refused));
        }
        let [filename] = evaluated_args else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let path = eval_opcache_path_value(*filename, values)?;
        return Ok(Some(eval_opcache_is_script_cached_for_path(&path, values)?));
    }
    if name == "opcache_invalidate" {
        if let Some(refused) = eval_opcache_api_refusal(values)? {
            return Ok(Some(refused));
        }
        if evaluated_args.is_empty() || evaluated_args.len() > 2 {
            return Err(EvalStatus::RuntimeFatal);
        }
        let path = eval_opcache_path_value(evaluated_args[0], values)?;
        let forced = match evaluated_args.get(1) {
            Some(force) => values.truthy(*force)?,
            None => false,
        };
        return Ok(Some(eval_opcache_invalidate_for_path(&path, forced, values)?));
    }
    if name == "opcache_compile_file" {
        let [filename] = evaluated_args else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let path = eval_opcache_path_value(*filename, values)?;
        return Ok(Some(eval_opcache_compile_file_checked(&path, context, values)?));
    }
    if name == "opcache_is_script_cached_in_file_cache" {
        if let Some(refused) = eval_opcache_api_refusal(values)? {
            return Ok(Some(refused));
        }
        let [filename] = evaluated_args else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let path = eval_opcache_path_value(*filename, values)?;
        return Ok(Some(eval_opcache_is_script_cached_in_file_cache_for_path(
            &path, values,
        )?));
    }
    if name == "opcache_jit_blacklist" {
        if evaluated_args.len() != 1 {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(Some(eval_opcache_jit_blacklist_result(values)?));
    }
    }

    if let Some(result) =
        eval_date_procedural_alias_with_values(name, evaluated_args, context, values)?
    {
        return Ok(Some(result));
    }
    Ok(None)
}
