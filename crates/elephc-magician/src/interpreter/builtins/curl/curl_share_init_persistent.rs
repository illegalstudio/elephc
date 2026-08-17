//! Purpose:
//! Eval home for PHP 8.5's `curl_share_init_persistent(array $share_options):
//! CurlSharePersistentHandle`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - PHP 8.5 ONLY, gated the same runtime way `curl_multi_get_handles()` is (see its own
//!   header): the AOT prelude fences this declaration out below 8.5 and eval checks the
//!   published compatibility profile instead.
//! - ONLY THE FIVE `CURL_LOCK_DATA_*` VALUES PHP ACTUALLY EXPOSES ARE ACCEPTED — 2 COOKIE,
//!   3 DNS, 4 SSL_SESSION, 5 CONNECT, 6 PSL — everything else is php-src's own
//!   `ValueError`, mirroring `crate::curl_prelude::curl_share_init_persistent` literal for
//!   literal.
//! - THE ARRAY CROSSES THE ABI AS A COMMA-SEPARATED DECIMAL STRING, the same encoding the
//!   AOT prelude builds, because this C ABI has no native array shape. The BRIDGE sorts and
//!   deduplicates, which is what makes an equivalent option set — any order, with
//!   duplicates — resolve to the SAME process-lifetime share.
//! - PROCESS-LIFETIME, NEVER FREED: `elephc_curl_share_free` is a documented no-op for a
//!   persistent entry, so this table's teardown cannot release it either — recorded here
//!   through `open_curl_share_handle(raw, true)` purely so `curl_share_setopt()`/
//!   `curl_share_errno()`/`curl_share_close()` can refuse it the way their AOT
//!   `CurlShareHandle` parameter type does (php-src does not make the persistent class a
//!   subclass).

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_share_init_persistent",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_share_init_persistent($share_options)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_curl_share_init_persistent(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [share_options] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let share_options = eval_expr(share_options, context, scope, values)?;
    eval_curl_share_init_persistent_result(share_options, context, values)
}

/// Dispatches evaluated `curl_share_init_persistent()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_share_init_persistent_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [share_options] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_curl_share_init_persistent_result(*share_options, context, values)
}

/// Validates the `CURL_LOCK_DATA_*` list, encodes it, and boxes the resulting share.
fn eval_curl_share_init_persistent_result(
    share_options: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_curl_require_php_85("curl_share_init_persistent", context, values)?;
    if !eval_curl_is_php_array(share_options, values)? {
        let given = eval_curl_given_type_name(share_options, context, values)?;
        return eval_throw_type_error(
            &format!(
                "curl_share_init_persistent(): Argument #1 ($share_options) must be of type \
                 array, {given} given"
            ),
            context,
            values,
        );
    }
    let mut csv = String::new();
    let len = values.array_len(share_options)?;
    for position in 0..len {
        let key = values.array_iter_key(share_options, position)?;
        let item = values.array_get(share_options, key)?;
        let tag = values.type_tag(item)?;
        if matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT) {
            return eval_curl_share_persistent_value_error(context, values);
        }
        let value = eval_int_value(item, values)?;
        if !matches!(value, 2 | 3 | 4 | 5 | 6) {
            return eval_curl_share_persistent_value_error(context, values);
        }
        if !csv.is_empty() {
            csv.push(',');
        }
        csv.push_str(&value.to_string());
    }
    let Some(raw) = ffi::share_persistent_init(csv.as_bytes()) else {
        return eval_throw_runtime_exception(
            "curl_share_init_persistent(): libcurl could not allocate a share handle",
            context,
            values,
        );
    };
    let table_id = context
        .stream_resources_mut()
        .open_curl_share_handle(raw, true);
    values.curl_handle(table_id)
}

/// php-src's own rejection for anything that is not one of the five `CURL_LOCK_DATA_*`
/// constants, worded exactly as the AOT prelude words it.
fn eval_curl_share_persistent_value_error(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_throw_builtin_value_error(
        "curl_share_init_persistent(): Argument #1 ($share_options) must only contain \
         CURL_LOCK_DATA_* values",
        context,
        values,
    )
}
