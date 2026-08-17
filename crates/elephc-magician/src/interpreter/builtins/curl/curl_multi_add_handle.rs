//! Purpose:
//! Eval home for `curl_multi_add_handle(CurlMultiHandle $multi_handle, CurlHandle $handle): int`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - THE ADD-ORDER BOOKKEEPING HAPPENS ONLY ON `CURLM_OK`, exactly as
//!   `crate::curl_prelude::curl_multi_add_handle` does it: a refused attach
//!   (`CURLM_ADDED_ALREADY` for a handle already on this or another multi handle) must
//!   leave the list untouched, or `curl_multi_get_handles()` would report a handle libcurl
//!   never took.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_multi_add_handle",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_add_handle($multi_handle, $handle)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_multi_add_handle(
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
    eval_curl_multi_add_handle_result(multi_handle, handle, context, values)
}

/// Dispatches evaluated `curl_multi_add_handle()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_multi_add_handle_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle, handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_multi_add_handle_result(*multi_handle, *handle, context, values)
}

/// Attaches an easy handle and returns libcurl's raw `CURLMcode`.
fn eval_curl_multi_add_handle_result(
    multi_handle: RuntimeCellHandle,
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (multi_id, multi_raw) =
        eval_curl_multi_handle("curl_multi_add_handle", multi_handle, context, values)?;
    let (easy_id, easy_raw) = eval_curl_easy_handle_at(
        "curl_multi_add_handle",
        2,
        "handle",
        handle,
        context,
        values,
    )?;
    let code = ffi::multi_add(multi_raw, easy_raw);
    if code == 0 {
        context
            .stream_resources_mut()
            .attach_curl_multi_easy(multi_id, easy_id);
    }
    values.int(code)
}
