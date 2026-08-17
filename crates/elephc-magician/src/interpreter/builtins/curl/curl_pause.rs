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
    let (table_id, raw) = eval_curl_easy_handle("curl_pause", handle, context, values)?;
    let flags = eval_int_value(flags, values)?;
    let Ok(bitmask) = i32::try_from(flags) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    // A CALLBACK FRAME AROUND `curl_easy_pause` IS NOT BELT AND BRACES — UNPAUSING FLUSHES.
    // `CURLPAUSE_CONT` on a handle whose receive side was paused makes libcurl deliver the
    // buffered body immediately, from inside this very call, which fires the write (and
    // header) callback. Without an active frame the adapter would find no interpreter to
    // re-enter and answer "wrote nothing", which libcurl reads as `CURLE_WRITE_ERROR`. The
    // AOT side reaches the same conclusion from the other direction: its
    // `__rt_curl_easy_pause` is one of the three sites that calls
    // `__rt_curl_rethrow_pending` afterwards (`src/codegen_support/runtime/curl/callbacks.rs`),
    // which only makes sense because a callback can genuinely throw during a pause.
    let code = eval_curl_with_callback_frame(&[table_id], context, values, || {
        ffi::easy_pause(raw, bitmask)
    })?;
    eval_curl_rethrow_pending_callback_throw()?;
    values.int(i64::from(code))
}
