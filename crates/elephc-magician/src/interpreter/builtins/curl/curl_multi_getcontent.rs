//! Purpose:
//! Eval home for `curl_multi_getcontent(CurlHandle $handle): ?string`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - `null` (NOT `""`) FOR A HANDLE WITHOUT `CURLOPT_RETURNTRANSFER`, matching php-src's
//!   own `RETURN_NULL()` for a handle whose write method is not `PHP_CURL_RETURN`, and
//!   matching `crate::curl_prelude::curl_multi_getcontent` verbatim. The two answers are
//!   genuinely different: `""` means "captured nothing", `null` means "was never
//!   capturing".
//! - READING THE BODY DOES NOT CONSUME IT — `elephc_curl_easy_take_body` hands back a copy
//!   and leaves the capture buffer in place (see its own ABI doc), so calling this twice
//!   answers the same bytes twice, exactly as php-src's `RETURN_STR_COPY` does.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_multi_getcontent",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_getcontent($handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_multi_getcontent(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    eval_curl_multi_getcontent_result(handle, context, values)
}

/// Dispatches evaluated `curl_multi_getcontent()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_multi_getcontent_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_multi_getcontent_result(*handle, context, values)
}

/// Reads a multi-driven transfer's captured body without consuming it.
fn eval_curl_multi_getcontent_result(
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (table_id, raw) = eval_curl_easy_handle("curl_multi_getcontent", handle, context, values)?;
    if !context.stream_resources().curl_easy_return_transfer(table_id) {
        return values.null();
    }
    let body = ffi::easy_take_body(raw).unwrap_or_default();
    values.string_bytes_value(&body)
}
