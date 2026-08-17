//! Purpose:
//! Eval home for PHP 8.5's `curl_multi_get_handles(CurlMultiHandle $multi_handle): array`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - PHP 8.5 ONLY, and the gate is a RUNTIME check here where the AOT side gets it from
//!   the prelude's `-- elephc PHP >= 8.5 ... --` source fence: eval carries one registry
//!   for every compatibility profile, so `eval_curl_require_php_85` consults the profile
//!   generated code published through `__elephc_eval_set_php_version_id` and raises PHP's
//!   own catchable "Call to undefined function" `\Error` below 8.5 — the same observable
//!   answer a program compiled with `--php-version 8.4` gets.
//! - THE HANDLES COME BACK IN ADD ORDER, from the eval-side attachment list
//!   (`EvalCurlMultiHandle::attached`). Each is re-boxed from its table key rather than
//!   handed back from a stored cell: an eval curl handle is an inert resource-kind-5 cell
//!   owning nothing, so two cells carrying one key are interchangeable — the AOT class has
//!   to keep the original OBJECTS instead, because a second `CurlHandle` around one native
//!   id would double-free it.

eval_builtin! {
    name: "curl_multi_get_handles",
    area: Curl,
    params: [multi_handle],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_get_handles($multi_handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_multi_get_handles(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let multi_handle = eval_expr(multi_handle, context, scope, values)?;
    eval_curl_multi_get_handles_result(multi_handle, context, values)
}

/// Dispatches evaluated `curl_multi_get_handles()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_multi_get_handles_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_multi_get_handles_result(*multi_handle, context, values)
}

/// Lists the attached easy handles in add order.
fn eval_curl_multi_get_handles_result(
    multi_handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_curl_require_php_85("curl_multi_get_handles", context, values)?;
    let (multi_id, _) =
        eval_curl_multi_handle("curl_multi_get_handles", multi_handle, context, values)?;
    let attached = context
        .stream_resources()
        .curl_multi_attached(multi_id)
        .unwrap_or_default();
    let mut array = values.array_new(attached.len())?;
    for (position, easy_id) in attached.into_iter().enumerate() {
        let handle = values.curl_handle(easy_id)?;
        let key = values.int(position as i64)?;
        array = values.array_set(array, key, handle)?;
    }
    Ok(array)
}
