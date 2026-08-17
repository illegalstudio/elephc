//! Purpose:
//! Eval home for `curl_multi_errno(CurlMultiHandle $multi_handle): int`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_multi_errno",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_errno($multi_handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_multi_errno(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let multi_handle = eval_expr(multi_handle, context, scope, values)?;
    eval_curl_multi_errno_result(multi_handle, context, values)
}

/// Dispatches evaluated `curl_multi_errno()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_multi_errno_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_multi_errno_result(*multi_handle, context, values)
}

/// Reports the `CURLMcode` from the most recent operation on this multi handle.
fn eval_curl_multi_errno_result(
    multi_handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_multi_raw("curl_multi_errno", multi_handle, context, values)?;
    values.int(ffi::multi_errno(raw))
}
