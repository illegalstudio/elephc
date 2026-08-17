//! Purpose:
//! Eval home for `curl_share_init(): CurlShareHandle`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_share_init",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_share_init()` over its (empty) eval expression list.
pub(in crate::interpreter) fn eval_builtin_curl_share_init(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_curl_share_init_result(context, values)
}

/// Dispatches evaluated `curl_share_init()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_share_init_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_curl_share_init_result(context, values)
}

/// Allocates a fresh share handle and boxes its eval table key.
///
/// Mirrors `crate::curl_prelude::curl_share_init`, including the same documented
/// divergence `curl_init()`/`curl_multi_init()` carry: PHP declares no `false` arm, so
/// libcurl's allocation failure becomes a catchable `\RuntimeException`.
fn eval_curl_share_init_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(raw) = ffi::share_init() else {
        return eval_throw_runtime_exception(
            "curl_share_init(): libcurl could not allocate a share handle",
            context,
            values,
        );
    };
    let table_id = context
        .stream_resources_mut()
        .open_curl_share_handle(raw, false);
    values.curl_handle(table_id)
}
