//! Purpose:
//! Eval home for `curl_pause(CurlHandle $handle, int $flags): int`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_pause",
    area: Curl,
    params: [handle, flags],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_pause($handle, $flags)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_pause(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle, flags] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    let flags = eval_expr(flags, context, scope, values)?;
    eval_curl_pause_result(handle, flags, context, values)
}

/// Dispatches evaluated `curl_pause()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_pause_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle, flags] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_pause_result(*handle, *flags, context, values)
}

fn eval_curl_pause_result(
    handle: RuntimeCellHandle,
    flags: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_easy_raw(handle, context, values)?;
    let flags = eval_int_value(flags, values)?;
    let Ok(bitmask) = i32::try_from(flags) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    values.int(i64::from(ffi::easy_pause(raw, bitmask)))
}
