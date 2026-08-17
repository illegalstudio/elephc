//! Purpose:
//! Eval home for `curl_init(?string $url = null)`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_init",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates PHP `curl_init($url)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_init(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let url = match args {
        [] => None,
        [url] => Some(eval_expr(url, context, scope, values)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_curl_init_result(url, context, values)
}

/// Dispatches evaluated `curl_init()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_init_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let url = match evaluated_args {
        [] => None,
        [url] => Some(*url),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_curl_init_result(url, context, values)
}

/// Allocates a fresh easy handle, optionally seeds `CURLOPT_URL`, and boxes it.
///
/// Mirrors `crate::curl_prelude::curl_init`'s allocation-failure handling verbatim: a
/// catchable `\RuntimeException` with AOT's exact message
/// (`"curl_init(): libcurl could not allocate an easy handle"`), through the same
/// `eval_throw_runtime_exception` mechanism `curl_escape()`/`curl_unescape()` use for their
/// own libcurl-failure path (WP-B item 8, curl punch list) — this file used to hard-fault
/// here instead, on the mistaken belief that this interpreter has no catchable-exception
/// path from internals at all.
fn eval_curl_init_result(
    url: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(raw) = ffi::easy_init() else {
        return eval_throw_runtime_exception(
            "curl_init(): libcurl could not allocate an easy handle",
            context,
            values,
        );
    };
    if let Some(url) = url {
        if values.type_tag(url)? != EVAL_TAG_NULL {
            let url = values.cast_string(url)?;
            let bytes = values.string_bytes(url)?;
            // Ignored, matching `curl_init()`'s own AOT wrapper: a bad URL surfaces
            // later, at `curl_exec()`, not here.
            let _ = ffi::easy_set_url(raw, &bytes);
        }
    }
    let table_id = context.stream_resources_mut().open_curl_easy_handle(raw);
    values.curl_handle(table_id)
}
