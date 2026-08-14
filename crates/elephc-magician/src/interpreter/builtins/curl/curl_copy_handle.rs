//! Purpose:
//! Eval home for `curl_copy_handle(CurlHandle $handle): CurlHandle`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - CALLBACKS ARE NOT RE-REGISTERED, unlike `crate::curl_prelude::curl_copy_handle`:
//!   this family does not implement curl callbacks in `eval()` at all (module doc), so
//!   there is nothing to re-point at the copy.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_copy_handle",
    area: Curl,
    params: [handle],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_copy_handle($handle)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_copy_handle(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    eval_curl_copy_handle_result(handle, context, values)
}

/// Dispatches evaluated `curl_copy_handle()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_copy_handle_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_copy_handle_result(*handle, context, values)
}

/// Duplicates both layers: `elephc_curl_easy_duphandle` copies libcurl's own options, and
/// this copies the PHP-layer mirror fields — mirrors
/// `crate::curl_prelude::curl_copy_handle`'s own two-layer argument.
///
/// Allocation failure is a hard fault, for the same reason `curl_init()`'s own doc gives:
/// no catchable-exception path exists from this interpreter's internals.
fn eval_curl_copy_handle_result(
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let table_id = eval_curl_easy_table_id(handle, values)?;
    let (raw, return_transfer, write_user, private_value) = {
        let resources = context.stream_resources();
        let raw = resources.curl_easy_raw(table_id).ok_or(EvalStatus::RuntimeFatal)?;
        let (return_transfer, write_user, private_value) = resources
            .curl_easy_mirror(table_id)
            .ok_or(EvalStatus::RuntimeFatal)?;
        (raw, return_transfer, write_user, private_value)
    };
    let Some(new_raw) = ffi::easy_duphandle(raw) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let new_table_id = context.stream_resources_mut().adopt_curl_easy_handle(
        new_raw,
        return_transfer,
        write_user,
        private_value,
    );
    values.curl_handle(new_table_id)
}
