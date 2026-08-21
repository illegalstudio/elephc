//! Purpose:
//! Eval home for `curl_setopt(CurlHandle $handle, int $option, mixed $value): bool`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//! - `curl_setopt_array`, which applies this same logic per array entry.
//!
//! Key details:
//! - The actual option-KIND dispatch lives in `super::handle::eval_curl_setopt_apply`,
//!   shared with `curl_setopt_array` — see that function's doc for the full kind table
//!   and what is deferred.

eval_builtin! {
    contract: "curl_setopt",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_setopt($handle, $option, $value)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_setopt(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle, option, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    let option = eval_expr(option, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_curl_setopt_result(handle, option, value, context, values)
}

/// Dispatches evaluated `curl_setopt()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_setopt_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle, option, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_setopt_result(*handle, *option, *value, context, values)
}

/// Resolves the handle and option number, then delegates to the shared KIND dispatcher.
pub(in crate::interpreter) fn eval_curl_setopt_result(
    handle: RuntimeCellHandle,
    option: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (table_id, raw) = eval_curl_easy_handle("curl_setopt", handle, context, values)?;
    let option = eval_int_value(option, values)?;
    eval_curl_setopt_apply(raw, table_id, option, value, context, values)
}
