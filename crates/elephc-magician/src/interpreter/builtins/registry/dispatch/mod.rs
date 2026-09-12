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
    // disagree with `opcache_is_script_cached($f)`. The remaining two are terminal: the file
    // cache does not exist, and the `void` `opcache_jit_blacklist` yields `NULL`.
    if name == "opcache_is_script_cached" {
        let [filename] = evaluated_args else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let path = eval_opcache_path_value(*filename, values)?;
        return Ok(Some(eval_opcache_is_script_cached_for_path(&path, values)?));
    }
    if name == "opcache_invalidate" {
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
        return Ok(Some(eval_opcache_compile_file_for_path(&path, values)?));
    }
    if name == "opcache_is_script_cached_in_file_cache" {
        if evaluated_args.len() != 1 {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(Some(eval_opcache_is_script_cached_in_file_cache_result(
            values,
        )?));
    }
    if name == "opcache_jit_blacklist" {
        if evaluated_args.len() != 1 {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(Some(eval_opcache_jit_blacklist_result(values)?));
    }

    if let Some(result) =
        eval_date_procedural_alias_with_values(name, evaluated_args, context, values)?
    {
        return Ok(Some(result));
    }
    Ok(None)
}
