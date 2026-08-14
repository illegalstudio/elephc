//! Purpose:
//! Eval home for `curl_strerror(int $error_code): ?string`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - Handle-free: a `CURLcode`'s text does not depend on any transfer. Always returns a
//!   real string, never `null` — measured on the AOT side too
//!   (`crate::curl_prelude::curl_strerror`'s own header: libcurl's `curl_easy_strerror`
//!   never answers empty, so the PHP-normative `?string` signature is never actually
//!   exercised).

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_strerror",
    area: Curl,
    params: [error_code],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_strerror($error_code)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_strerror(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [error_code] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let error_code = eval_expr(error_code, context, scope, values)?;
    eval_curl_strerror_result(error_code, values)
}

/// Dispatches evaluated `curl_strerror()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_strerror_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [error_code] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_strerror_result(*error_code, values)
}

fn eval_curl_strerror_result(
    error_code: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let code = eval_int_value(error_code, values)?;
    let Ok(code) = i32::try_from(code) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let message = ffi::strerror(code);
    values.string_bytes_value(&message)
}
