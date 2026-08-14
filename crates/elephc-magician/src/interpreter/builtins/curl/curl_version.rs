//! Purpose:
//! Eval home for `curl_version()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_version",
    area: Curl,
    params: [],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates PHP `curl_version()`. Decodes the bridge's JSON blob through the ordinary
/// `json_decode()` builtin, exactly like `crate::curl_prelude::curl_version` does — so the
/// array reflects whatever libcurl is actually linked at run time, never anything baked in
/// here.
pub(in crate::interpreter) fn eval_builtin_curl_version(
    _args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_curl_version_result(context, values)
}

/// Dispatches evaluated `curl_version()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_version_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_curl_version_result(context, values)
}

fn eval_curl_version_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let json = ffi::global_info_json();
    if json.is_empty() {
        return values.bool_value(false);
    }
    let json_cell = values.string_bytes_value(&json)?;
    let associative = values.bool_value(true)?;
    let decoded = crate::interpreter::builtins::json::eval_json_decode_values_result(
        &[json_cell, associative],
        context,
        values,
    )?;
    if !values.is_array_like(decoded)? {
        return values.bool_value(false);
    }
    Ok(decoded)
}
