//! Purpose:
//! Eval home for `curl_upkeep(CurlHandle $handle): bool`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_upkeep",
    area: Curl,
    params: [handle],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_upkeep($handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_upkeep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    eval_curl_upkeep_result(handle, context, values)
}

/// Dispatches evaluated `curl_upkeep()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_upkeep_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_upkeep_result(*handle, context, values)
}

fn eval_curl_upkeep_result(
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_easy_raw("curl_upkeep", handle, context, values)?;
    values.bool_value(ffi::easy_upkeep(raw))
}
