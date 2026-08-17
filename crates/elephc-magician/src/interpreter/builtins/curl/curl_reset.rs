//! Purpose:
//! Eval home for `curl_reset(CurlHandle $handle): void`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_reset",
    area: Curl,
    params: [handle],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_reset($handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_reset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    eval_curl_reset_result(handle, context, values)
}

/// Dispatches evaluated `curl_reset()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_reset_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_reset_result(*handle, context, values)
}

/// Resets libcurl's own options AND this interpreter's PHP-layer mirror fields
/// (`RETURNTRANSFER`/write-mode/`CURLOPT_PRIVATE`) — mirrors
/// `crate::curl_prelude::curl_reset`'s three-part reset (see that function's own comment
/// for why all three have to move together).
fn eval_curl_reset_result(
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (table_id, raw) = eval_curl_easy_handle("curl_reset", handle, context, values)?;
    ffi::easy_reset(raw);
    context
        .stream_resources_mut()
        .reset_curl_easy_mirror(table_id, values)?;
    // The bridge's own `elephc_curl_easy_reset` already ran `callbacks::clear_all`, which
    // dropped every libcurl-side registration; this drops the eval-side ROOTS to match, so a
    // reset handle holds no retained callable — php-src frees its handler callables in
    // `curl_reset()` for the same reason.
    eval_curl_clear_callbacks(table_id, context, values)?;
    values.null()
}
