//! Purpose:
//! Eval home for `curl_init(?string $url = null)`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.

use crate::curl_ffi as ffi;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "curl_init",
    area: Curl,
    params: [url = EvalBuiltinDefaultValue::Null],
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
/// Mirrors `crate::curl_prelude::curl_init`'s allocation-failure handling: real PHP
/// throws `\RuntimeException` there; this interpreter has no catchable-exception path
/// from internals (see `crate::interpreter::builtins::curl::handle`'s own note on the same
/// tradeoff for `curl_setopt()`), so a libcurl allocation failure — which no real program
/// meaningfully recovers from either way — is a hard fault here instead.
fn eval_curl_init_result(
    url: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(raw) = ffi::easy_init() else {
        return Err(EvalStatus::RuntimeFatal);
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
