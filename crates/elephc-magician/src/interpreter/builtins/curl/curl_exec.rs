//! Purpose:
//! Eval home for `curl_exec(CurlHandle $handle): string|bool`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - The DEFAULT (no `RETURNTRANSFER`, no `CURLOPT_FILE`) write path streams the response
//!   body straight to fd 1, matching PHP CLI — that happens entirely inside
//!   `elephc-curl`'s own default write callback (installed at `elephc_curl_easy_init()`
//!   time), so this function only ever has to decide `curl_exec()`'s RETURN shape.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_exec",
    area: Curl,
    params: [handle],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_exec($handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_exec(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    eval_curl_exec_result(handle, context, values)
}

/// Dispatches evaluated `curl_exec()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_exec_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_exec_result(*handle, context, values)
}

fn eval_curl_exec_result(
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (table_id, raw) = eval_curl_easy_handle("curl_exec", handle, context, values)?;
    // THE FRAME MUST COVER THE PERFORM, and the rethrow must follow it before anything else
    // is decided: a callback that threw aborted the transfer, so `easy_perform` answers
    // `false` for a reason that is a PHP exception rather than a `CURLcode`. Reporting
    // `false` first would swallow the throwable.
    let (performed, parked) = eval_curl_with_callback_frame(&[table_id], context, values, || {
        ffi::easy_perform(raw)
    })?;
    eval_curl_resume_callback_throw(parked)?;
    if !performed {
        return values.bool_value(false);
    }
    if context.stream_resources().curl_easy_return_transfer(table_id) {
        let body = ffi::easy_take_body(raw).unwrap_or_default();
        return values.string_bytes_value(&body);
    }
    values.bool_value(true)
}
