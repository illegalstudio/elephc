//! Purpose:
//! Eval home for `curl_multi_setopt(CurlMultiHandle $multi_handle, int $option, mixed
//! $value): bool`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - THE OPTION IS CLASSIFIED BEFORE THE VALUE IS TYPE-CHECKED, which is php-src's own
//!   order and the order `crate::curl_prelude::curl_multi_setopt` reproduces (measured
//!   there against PHP 8.4.20: `curl_multi_setopt($mh, 999999, function () {})` is a
//!   `ValueError`, not a `TypeError`, and `CURLMOPT_PUSHFUNCTION` with any value at all —
//!   a closure included — is the unsupported-option warning plus `false`).
//! - The `CURLMOPT_*` NUMBERS ARE CLASSIFIED INSIDE THE BRIDGE, never here: the libcurl
//!   entry point is variadic and picks its third argument's C type purely from the
//!   option's numeric range, so this is a memory-safety boundary, not a lookup
//!   convenience. The literal list below only decides WHICH of the three answers is
//!   reached first, exactly as the AOT prelude's own copy does; the bridge re-classifies
//!   every call that gets past it and stays the authority.

use crate::curl_ffi as ffi;

eval_builtin! {
    name: "curl_multi_setopt",
    area: Curl,
    params: [multi_handle, option, value],
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_setopt($multi_handle, $option, $value)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_multi_setopt(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle, option, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let multi_handle = eval_expr(multi_handle, context, scope, values)?;
    let option = eval_expr(option, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_curl_multi_setopt_result(multi_handle, option, value, context, values)
}

/// Dispatches evaluated `curl_multi_setopt()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_multi_setopt_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [multi_handle, option, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_multi_setopt_result(*multi_handle, *option, *value, context, values)
}

/// Applies one `CURLMOPT_*` option, mirroring the AOT prelude's three-way answer.
fn eval_curl_multi_setopt_result(
    multi_handle: RuntimeCellHandle,
    option: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_multi_raw("curl_multi_setopt", multi_handle, context, values)?;
    let option = eval_int_value(option, values)?;
    // 3 PIPELINING, 6 MAXCONNECTS, 7 MAX_HOST_CONNECTIONS, 8 MAX_PIPELINE_LENGTH,
    // 13 MAX_TOTAL_CONNECTIONS, 16 MAX_CONCURRENT_STREAMS -> long;
    // 30009 CONTENT_LENGTH_PENALTY_SIZE, 30010 CHUNK_LENGTH_PENALTY_SIZE -> off_t.
    let carryable = matches!(option, 3 | 6 | 7 | 8 | 13 | 16 | 30_009 | 30_010);
    if !carryable {
        // 20014 CURLMOPT_PUSHFUNCTION: a real php-src option this build cannot carry (an
        // HTTP/2 server-push hook, and HTTP/2 is not built in).
        if option == 20_014 {
            eval_curl_warn_unsupported_multi_option(option, values)?;
            return values.bool_value(false);
        }
        return eval_throw_builtin_value_error(
            "curl_multi_setopt(): Argument #2 ($option) is not a valid cURL multi option",
            context,
            values,
        );
    }
    let tag = values.type_tag(value)?;
    if !matches!(
        tag,
        EVAL_TAG_INT | EVAL_TAG_STRING | EVAL_TAG_FLOAT | EVAL_TAG_BOOL
    ) {
        let given = eval_curl_given_type_name(value, context, values)?;
        return eval_throw_type_error(
            &format!(
                "curl_multi_setopt(): Argument #3 ($value) must be of type \
                 string|int|float|bool, {given} given"
            ),
            context,
            values,
        );
    }
    let value = eval_int_value(value, values)?;
    // Both non-applied answers stay honored rather than assumed unreachable: the bridge is
    // the authority, and `MULTI_SETOPT_UNSUPPORTED` is also how it reports an option
    // libcurl itself refused.
    let applied = ffi::multi_setopt(raw, option, value);
    if applied == ffi::MULTI_SETOPT_APPLIED {
        return values.bool_value(true);
    }
    if applied == ffi::MULTI_SETOPT_INVALID {
        return eval_throw_builtin_value_error(
            "curl_multi_setopt(): Argument #2 ($option) is not a valid cURL multi option",
            context,
            values,
        );
    }
    // `MULTI_SETOPT_UNSUPPORTED`, and — defensively rather than by assumption — any code a
    // future bridge might add: both mean "not applied", which is PHP's `false` plus the
    // unsupported-option warning.
    debug_assert_eq!(applied, ffi::MULTI_SETOPT_UNSUPPORTED);
    eval_curl_warn_unsupported_multi_option(option, values)?;
    values.bool_value(false)
}
