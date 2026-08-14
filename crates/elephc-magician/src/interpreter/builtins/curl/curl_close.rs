//! Purpose:
//! Eval home for `curl_close(CurlHandle $handle): void`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - A NO-OP, exactly as in PHP 8: `crate::curl_prelude::curl_close` compiles to an empty
//!   function body too. The handle stays usable (and, in eval, stays allocated until
//!   `EvalStreamResources::drop` — see `EvalCurlEasyHandle`'s doc) until it is garbage
//!   collected.

eval_builtin! {
    name: "curl_close",
    area: Curl,
    params: [handle],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_close($handle)`: evaluates the argument for any side effects in the
/// expression and returns `null`, never touching the bridge.
pub(in crate::interpreter) fn eval_builtin_curl_close(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_expr(handle, context, scope, values)?;
    values.null()
}

/// Dispatches evaluated `curl_close()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_close_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [_handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    values.null()
}
