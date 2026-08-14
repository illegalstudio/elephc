//! Purpose:
//! Eval home for `curl_error(CurlHandle $handle): string`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_error",
    area: Curl,
    params: [handle],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_error($handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_error(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    eval_curl_error_result(handle, context, values)
}

/// Dispatches evaluated `curl_error()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_error_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_error_result(*handle, context, values)
}

fn eval_curl_error_result(
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_easy_raw("curl_error", handle, context, values)?;
    let message = ffi::easy_error(raw);
    values.string_bytes_value(&message)
}
