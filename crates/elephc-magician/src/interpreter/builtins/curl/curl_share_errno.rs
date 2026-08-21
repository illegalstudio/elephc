//! Purpose:
//! Eval home for `curl_share_errno(CurlShareHandle $share_handle): int`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_share_errno",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_share_errno($share_handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_share_errno(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [share_handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let share_handle = eval_expr(share_handle, context, scope, values)?;
    eval_curl_share_errno_result(share_handle, context, values)
}

/// Dispatches evaluated `curl_share_errno()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_share_errno_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [share_handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_share_errno_result(*share_handle, context, values)
}

/// Reports the `CURLSHcode` from the most recent operation on this share handle.
fn eval_curl_share_errno_result(
    share_handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_share_raw("curl_share_errno", share_handle, context, values)?;
    values.int(ffi::share_errno(raw))
}
