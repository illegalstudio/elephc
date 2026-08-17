//! Purpose:
//! Eval home for `curl_multi_close(CurlMultiHandle $multi_handle): void`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - A NO-OP past argument validation, exactly as in PHP 8 and exactly as
//!   `crate::curl_prelude::curl_multi_close` (an empty body) — the same shape
//!   `curl_close()` already has here. The multi handle stays usable and stays allocated
//!   until `EvalStreamResources::drop`, which is where every eval-owned curl handle is
//!   freed.

eval_builtin! {
    name: "curl_multi_close",
    area: Curl,
    params: [multi_handle],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_close($multi_handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_multi_close(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let multi_handle = eval_expr(multi_handle, context, scope, values)?;
    eval_curl_multi_close_result(multi_handle, context, values)
}

/// Dispatches evaluated `curl_multi_close()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_multi_close_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_multi_close_result(*multi_handle, context, values)
}

/// Validates `$multi_handle` and returns `null`, never touching the bridge.
fn eval_curl_multi_close_result(
    multi_handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_curl_multi_raw("curl_multi_close", multi_handle, context, values)?;
    values.null()
}
