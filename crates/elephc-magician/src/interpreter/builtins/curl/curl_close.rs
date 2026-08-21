//! Purpose:
//! Eval home for `curl_close(CurlHandle $handle): void`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - A NO-OP past argument validation, exactly as in PHP 8: `crate::curl_prelude::
//!   curl_close` compiles to an empty function body too. The handle stays usable (and, in
//!   eval, stays allocated until `EvalStreamResources::drop` — see `EvalCurlEasyHandle`'s
//!   doc) until it is garbage collected.
//! - `$handle` IS validated, even though the body never dereferences it: real PHP raises a
//!   catchable `\TypeError` for a non-`CurlHandle` argument (verified against PHP 8.4.20),
//!   and this used to accept literally anything (issue tracked as WP-B item 9) — see
//!   `eval_curl_easy_handle`'s own doc for why that check has to live in eval and cannot be
//!   copied from an AOT runtime throw (there isn't one; the AOT parameter type is enforced
//!   at compile time instead).

eval_builtin! {
    contract: "curl_close",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_close($handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_close(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    eval_curl_close_result(handle, context, values)
}

/// Dispatches evaluated `curl_close()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_close_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_close_result(*handle, context, values)
}

/// Validates `$handle` (throwing PHP's own catchable `\TypeError` for anything else) and
/// returns `null`, never touching the bridge.
fn eval_curl_close_result(
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_curl_easy_raw("curl_close", handle, context, values)?;
    values.null()
}
