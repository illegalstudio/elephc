//! Purpose:
//! Eval home for `curl_multi_select(CurlMultiHandle $multi_handle, float $timeout = 1.0):
//! int`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - The seconds-to-milliseconds conversion is php-src's own plain `(int)($timeout *
//!   1000.0)` cast, reproduced verbatim from `crate::curl_prelude::curl_multi_select`,
//!   INCLUDING for a zero or negative timeout — libcurl reads those as "return
//!   immediately" and php-src passes them straight through.

use crate::curl_ffi as ffi;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "curl_multi_select",
    area: Curl,
    params: [multi_handle, timeout = EvalBuiltinDefaultValue::Float(1.0)],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_select($multi_handle, $timeout)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_multi_select(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (multi_handle, timeout) = match args {
        [multi_handle] => (eval_expr(multi_handle, context, scope, values)?, None),
        [multi_handle, timeout] => {
            let multi_handle = eval_expr(multi_handle, context, scope, values)?;
            let timeout = eval_expr(timeout, context, scope, values)?;
            (multi_handle, Some(timeout))
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_curl_multi_select_result(multi_handle, timeout, context, values)
}

/// Dispatches evaluated `curl_multi_select()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_multi_select_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (multi_handle, timeout) = match evaluated_args {
        [multi_handle] => (*multi_handle, None),
        [multi_handle, timeout] => (*multi_handle, Some(*timeout)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_curl_multi_select_result(multi_handle, timeout, context, values)
}

/// Waits for an attached transfer to become ready, answering the ready-descriptor count or
/// `-1` on a libcurl error.
fn eval_curl_multi_select_result(
    multi_handle: RuntimeCellHandle,
    timeout: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_multi_raw("curl_multi_select", multi_handle, context, values)?;
    let seconds = match timeout {
        Some(timeout) => eval_float_value(timeout, values)?,
        None => 1.0,
    };
    let milliseconds = (seconds * 1000.0) as i64;
    values.int(ffi::multi_select(raw, milliseconds))
}
