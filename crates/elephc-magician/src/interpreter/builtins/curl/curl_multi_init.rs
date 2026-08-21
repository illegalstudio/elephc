//! Purpose:
//! Eval home for `curl_multi_init(): CurlMultiHandle`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_multi_init",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_init()` over its (empty) eval expression list.
pub(in crate::interpreter) fn eval_builtin_curl_multi_init(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_curl_multi_init_result(context, values)
}

/// Dispatches evaluated `curl_multi_init()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_multi_init_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_curl_multi_init_result(context, values)
}

/// Allocates a fresh multi handle and boxes its eval table key.
///
/// Mirrors `crate::curl_prelude::curl_multi_init` verbatim, including its ONE documented
/// divergence from php-src: PHP declares `curl_multi_init(): CurlMultiHandle` with no
/// `false` arm, so libcurl's allocation failure — which the bridge can still report —
/// becomes a catchable `\RuntimeException` rather than a return value the signature does
/// not have.
fn eval_curl_multi_init_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(raw) = ffi::multi_init() else {
        return eval_throw_runtime_exception(
            "curl_multi_init(): libcurl could not allocate a multi handle",
            context,
            values,
        );
    };
    let table_id = context.stream_resources_mut().open_curl_multi_handle(raw);
    values.curl_handle(table_id)
}
