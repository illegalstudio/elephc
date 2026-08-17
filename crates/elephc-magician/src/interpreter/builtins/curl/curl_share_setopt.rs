//! Purpose:
//! Eval home for `curl_share_setopt(CurlShareHandle $share_handle, int $option, mixed
//! $value): bool`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - ONLY `CURLSHOPT_SHARE` (1) AND `CURLSHOPT_UNSHARE` (2) ARE REAL PHP SURFACE, so
//!   unlike `curl_setopt()`/`curl_multi_setopt()` there is no "real option this build
//!   cannot carry" bucket at all — everything else is php-src's own `ValueError`.
//! - A REFUSED VALUE IS A PLAIN `false`, NEVER A FABRICATED WARNING: `CURLSHE_BAD_OPTION`
//!   for a `CURL_LOCK_DATA_*` libcurl does not recognize (or `CURLSHE_NOT_BUILT_IN` for one
//!   this build lacks) is a genuine libcurl answer, retrievable through
//!   `curl_share_errno()`/`curl_share_strerror()`. Mirrors
//!   `crate::curl_prelude::curl_share_setopt` and `crates/elephc-curl/src/share.rs`'s
//!   module doc.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_share_setopt",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_share_setopt($share_handle, $option, $value)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_share_setopt(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [share_handle, option, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let share_handle = eval_expr(share_handle, context, scope, values)?;
    let option = eval_expr(option, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_curl_share_setopt_result(share_handle, option, value, context, values)
}

/// Dispatches evaluated `curl_share_setopt()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_share_setopt_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [share_handle, option, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_share_setopt_result(*share_handle, *option, *value, context, values)
}

/// Applies one `CURLSHOPT_*` option.
fn eval_curl_share_setopt_result(
    share_handle: RuntimeCellHandle,
    option: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_share_raw("curl_share_setopt", share_handle, context, values)?;
    let option = eval_int_value(option, values)?;
    // THE SCALAR GUARD RUNS BEFORE THE OPTION IS APPLIED, matching
    // `crate::curl_prelude::curl_share_setopt`'s own order (it type-checks `$value` first
    // and lets the bridge classify `$option` afterwards) — the opposite of
    // `curl_multi_setopt()`, whose php-src counterpart never looks at the value's type at
    // all. The two functions genuinely differ; this is not an inconsistency to tidy away.
    let tag = values.type_tag(value)?;
    if !matches!(
        tag,
        EVAL_TAG_INT | EVAL_TAG_STRING | EVAL_TAG_FLOAT | EVAL_TAG_BOOL
    ) {
        let given = eval_curl_given_type_name(value, context, values)?;
        return eval_throw_type_error(
            &format!(
                "curl_share_setopt(): Argument #3 ($value) must be of type \
                 string|int|float|bool, {given} given"
            ),
            context,
            values,
        );
    }
    let value = eval_int_value(value, values)?;
    let applied = ffi::share_setopt(raw, option, value);
    if applied == ffi::SHARE_SETOPT_APPLIED {
        return values.bool_value(true);
    }
    if applied == ffi::SHARE_SETOPT_INVALID {
        return eval_throw_builtin_value_error(
            "curl_share_setopt(): Argument #2 ($option) is not a valid cURL share option",
            context,
            values,
        );
    }
    // `SHARE_SETOPT_REFUSED`: libcurl itself declined the `CURL_LOCK_DATA_*` value. A plain
    // `false`, never a fabricated warning — the real `CURLSHcode` stays retrievable through
    // `curl_share_errno()`/`curl_share_strerror()`.
    debug_assert_eq!(applied, ffi::SHARE_SETOPT_REFUSED);
    values.bool_value(false)
}
