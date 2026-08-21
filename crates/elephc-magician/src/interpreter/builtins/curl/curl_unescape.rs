//! Purpose:
//! Eval home for `curl_unescape(CurlHandle $handle, string $string): string|false`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_unescape",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_unescape($handle, $string)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_unescape(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle, string] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    let string = eval_expr(string, context, scope, values)?;
    eval_curl_unescape_result(handle, string, context, values)
}

/// Dispatches evaluated `curl_unescape()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_unescape_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle, string] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_unescape_result(*handle, *string, context, values)
}

fn eval_curl_unescape_result(
    handle: RuntimeCellHandle,
    string: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_easy_raw("curl_unescape", handle, context, values)?;
    let string = values.cast_string(string)?;
    let bytes = values.string_bytes(string)?;
    match ffi::easy_str_op(raw, ffi::STR_OP_UNESCAPE, &bytes, 0) {
        Some(unescaped) => values.string_bytes_value(&unescaped),
        // See `curl_escape`'s matching branch: mirrors
        // `crate::curl_prelude::curl_unescape`'s catchable `\RuntimeException` on a
        // libcurl decode failure instead of answering `false`.
        None => eval_throw_runtime_exception(
            "curl_unescape(): libcurl could not URL-decode the string",
            context,
            values,
        ),
    }
}
