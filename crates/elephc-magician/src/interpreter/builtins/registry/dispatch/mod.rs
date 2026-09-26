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

use super::eval_declared_builtin_values_call_from_scope;
use super::super::super::*;

/// Evaluates PHP-visible builtins when they are invoked through a dynamic callable name.
pub(in crate::interpreter) fn eval_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_builtin_with_values_from_scope(name, evaluated_args, None, context, values)
}

/// Evaluates a builtin values hook with the lexical scope owned by a direct source call.
pub(in crate::interpreter) fn eval_builtin_with_values_from_scope(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(result) = eval_declared_builtin_values_call_from_scope(
        name,
        evaluated_args,
        lexical_scope,
        context,
        values,
    )? {
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
    // here as plain runtime handlers (not PHP-visible builtins), each returning the
    // CLI-default disabled-cache result (`false`) after validating arity — except the
    // `void` `opcache_jit_blacklist`, which yields `NULL`.
    if name == "opcache_is_script_cached" {
        if evaluated_args.len() != 1 {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(Some(eval_opcache_is_script_cached_result(values)?));
    }
    if name == "opcache_invalidate" {
        if evaluated_args.is_empty() || evaluated_args.len() > 2 {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(Some(eval_opcache_invalidate_result(values)?));
    }
    if name == "opcache_compile_file" {
        if evaluated_args.len() != 1 {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(Some(eval_opcache_compile_file_result(values)?));
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
