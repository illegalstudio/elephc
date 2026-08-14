//! Purpose:
//! Eval home for `curl_escape(CurlHandle $handle, string $string): string|false`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_escape",
    area: Curl,
    params: [handle, string],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_escape($handle, $string)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_escape(
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
    eval_curl_escape_result(handle, string, context, values)
}

/// Dispatches evaluated `curl_escape()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_escape_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle, string] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_escape_result(*handle, *string, context, values)
}

fn eval_curl_escape_result(
    handle: RuntimeCellHandle,
    string: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_easy_raw(handle, context, values)?;
    let string = values.cast_string(string)?;
    let bytes = values.string_bytes(string)?;
    match ffi::easy_str_op(raw, ffi::STR_OP_ESCAPE, &bytes, 0) {
        Some(escaped) => values.string_bytes_value(&escaped),
        None => values.bool_value(false),
    }
}
