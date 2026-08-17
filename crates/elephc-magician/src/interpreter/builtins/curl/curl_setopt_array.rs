//! Purpose:
//! Eval home for `curl_setopt_array(CurlHandle $handle, array $options): bool`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

eval_builtin! {
    contract: "curl_setopt_array",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_setopt_array($handle, $options)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_setopt_array(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle, options] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    let options = eval_expr(options, context, scope, values)?;
    eval_curl_setopt_array_result(handle, options, context, values)
}

/// Dispatches evaluated `curl_setopt_array()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_setopt_array_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle, options] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_setopt_array_result(*handle, *options, context, values)
}

/// Applies `curl_setopt()` once per `$options` entry, short-circuiting `false` on the
/// first rejected option — matching `crate::curl_prelude::curl_setopt_array`'s own
/// `foreach` loop exactly.
///
/// `$handle` (argument #1) is resolved BEFORE the `$options` (argument #2) array check,
/// matching AOT's own declared parameter order (`CurlHandle $handle, array $options`):
/// with a bad value in both positions, PHP validates left to right, so argument #1's
/// error must be the one reported. This used to check `$options` first, which would have
/// reported the wrong argument's error for `curl_setopt_array("not a handle", "not an
/// array")`.
fn eval_curl_setopt_array_result(
    handle: RuntimeCellHandle,
    options: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (table_id, raw) = eval_curl_easy_handle("curl_setopt_array", handle, context, values)?;
    if !values.is_array_like(options)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(options)?;
    for position in 0..len {
        let key = values.array_iter_key(options, position)?;
        let value = values.array_get(options, key)?;
        let option = eval_int_value(key, values)?;
        let applied = eval_curl_setopt_apply(raw, table_id, option, value, context, values)?;
        if !values.truthy(applied)? {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}
