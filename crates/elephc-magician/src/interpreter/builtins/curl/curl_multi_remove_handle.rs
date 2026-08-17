//! Purpose:
//! Eval home for `curl_multi_remove_handle(CurlMultiHandle $multi_handle, CurlHandle
//! $handle): int`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - The add-order bookkeeping is updated only on `CURLM_OK`, the mirror image of
//!   `curl_multi_add_handle()`'s own rule.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_multi_remove_handle",
    area: Curl,
    params: [multi_handle, handle],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_remove_handle($multi_handle, $handle)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_multi_remove_handle(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle, handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let multi_handle = eval_expr(multi_handle, context, scope, values)?;
    let handle = eval_expr(handle, context, scope, values)?;
    eval_curl_multi_remove_handle_result(multi_handle, handle, context, values)
}

/// Dispatches evaluated `curl_multi_remove_handle()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_multi_remove_handle_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle, handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_multi_remove_handle_result(*multi_handle, *handle, context, values)
}

/// Detaches an easy handle and returns libcurl's raw `CURLMcode`.
fn eval_curl_multi_remove_handle_result(
    multi_handle: RuntimeCellHandle,
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (multi_id, multi_raw) =
        eval_curl_multi_handle("curl_multi_remove_handle", multi_handle, context, values)?;
    let (easy_id, easy_raw) = eval_curl_easy_handle_at(
        "curl_multi_remove_handle",
        2,
        "handle",
        handle,
        context,
        values,
    )?;
    let code = ffi::multi_remove(multi_raw, easy_raw);
    if code == 0 {
        context
            .stream_resources_mut()
            .detach_curl_multi_easy(multi_id, easy_id);
    }
    values.int(code)
}
