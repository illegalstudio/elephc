//! Purpose:
//! Eval home for `curl_share_strerror(int $error_code): ?string`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - A THIRD numbering space: `curl_share_strerror(2)` ("share is in use") is unrelated to
//!   `curl_strerror(2)` and `curl_multi_strerror(2)`.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_share_strerror",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_share_strerror($error_code)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_share_strerror(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [error_code] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let error_code = eval_expr(error_code, context, scope, values)?;
    eval_curl_share_strerror_result(error_code, values)
}

/// Dispatches evaluated `curl_share_strerror()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_share_strerror_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [error_code] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_share_strerror_result(*error_code, values)
}

/// Returns libcurl's own message for a `CURLSHcode`.
fn eval_curl_share_strerror_result(
    error_code: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let code = eval_int_value(error_code, values)?;
    let Ok(code) = i32::try_from(code) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let message = ffi::share_strerror(code);
    values.string_bytes_value(&message)
}
