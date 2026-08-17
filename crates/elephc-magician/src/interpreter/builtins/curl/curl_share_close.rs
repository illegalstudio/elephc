//! Purpose:
//! Eval home for `curl_share_close(CurlShareHandle $share_handle): void`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - A NO-OP past argument validation, exactly as in PHP 8 and exactly as
//!   `crate::curl_prelude::curl_share_close` (an empty body). The real
//!   `elephc_curl_share_free` runs at `EvalStreamResources::drop`, and even there the
//!   bridge DEFERS the underlying `curl_share_cleanup()` while any easy handle is still
//!   attached — see `crates/elephc-curl/src/share.rs`'s module doc and this table's own
//!   teardown-order comment in `crate::stream_resources::types`.

eval_builtin! {
    contract: "curl_share_close",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_share_close($share_handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_share_close(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [share_handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let share_handle = eval_expr(share_handle, context, scope, values)?;
    eval_curl_share_close_result(share_handle, context, values)
}

/// Dispatches evaluated `curl_share_close()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_share_close_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [share_handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_share_close_result(*share_handle, context, values)
}

/// Validates `$share_handle` and returns `null`, never touching the bridge.
fn eval_curl_share_close_result(
    share_handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_curl_share_raw("curl_share_close", share_handle, context, values)?;
    values.null()
}
