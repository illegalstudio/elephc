//! Purpose:
//! Eval home for `curl_multi_info_read(CurlMultiHandle $multi_handle, int
//! &$queued_messages = null): array|false`.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call` (the `&[EvalCallArg]` interception
//!   that keeps the by-reference target reachable).
//! - `crate::interpreter::builtins::curl` dispatch (the by-value fallbacks).
//! - `crate::interpreter::builtins::registry::dynamic_mutation` (dynamic callables).
//!
//! Key details:
//! - `$queued_messages` IS LEFT UNTOUCHED WHEN THE QUEUE IS EMPTY, matching php-src (it
//!   returns `false` before it ever reaches its own `ZEND_TRY_ASSIGN_REF_LONG`) and
//!   `crate::curl_prelude::curl_multi_info_read` verbatim: a caller's variable keeps
//!   whatever it held.
//! - THE `handle` KEY IS OMITTED WHEN THE EASY HANDLE CANNOT BE RESOLVED, again php-src's
//!   own behaviour (`_php_curl_multi_find_easy_handle` returning NULL adds no key). In eval
//!   that only happens for a handle this context never created — the message names a
//!   bridge easy id and `curl_easy_id_for_raw` maps it back to the eval table key.
//! - ONE MESSAGE PER CALL, popped destructively: the bridge parks the popped message and
//!   hands its four fields back one `INFO_FIELD_*` read at a time, so this assembles PHP's
//!   `['msg' => …, 'result' => …, 'handle' => …]` array out of plain integers.

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_multi_info_read",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// Evaluates `curl_multi_info_read()` over full eval call metadata.
pub(in crate::interpreter) fn eval_builtin_curl_multi_info_read_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["multi_handle", "queued_messages"],
        &evaluated_args,
        false,
    )?;
    let multi_handle = required_evaluated_ref_arg(&bound, 0)?;
    let queued = optional_evaluated_ref_arg(&bound, 1);
    let target = queued.as_ref().and_then(|arg| arg.ref_target.clone());
    let supplied = queued.is_some();
    eval_curl_multi_info_read_result(
        multi_handle.value,
        target,
        supplied,
        context,
        values,
    )
}

/// Evaluates `curl_multi_info_read()` over plain eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_multi_info_read(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (multi_handle, target, supplied) = match args {
        [multi_handle] => (eval_expr(multi_handle, context, scope, values)?, None, false),
        [multi_handle, queued] => {
            let multi_handle = eval_expr(multi_handle, context, scope, values)?;
            let (_, target) = eval_call_arg_value(queued, context, scope, values)?;
            (multi_handle, target, true)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_curl_multi_info_read_result(multi_handle, target, supplied, context, values)
}

/// Dispatches evaluated `curl_multi_info_read()` calls through the builtin leaf. This path
/// has no reference targets (see `curl_multi_exec`'s header for the interpreter-wide
/// reason), so a supplied `$queued_messages` warns and is dropped.
pub(in crate::interpreter) fn eval_curl_multi_info_read_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (multi_handle, supplied) = match evaluated_args {
        [multi_handle] => (*multi_handle, false),
        [multi_handle, _queued] => (*multi_handle, true),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_curl_multi_info_read_result(multi_handle, None, supplied, context, values)
}

/// Runs one `curl_multi_info_read()` against a known-writable `$queued_messages` target —
/// the shape `crate::interpreter::builtins::registry::dynamic_mutation` needs for a dynamic
/// callable.
pub(in crate::interpreter) fn eval_curl_multi_info_read_with_target(
    multi_handle: RuntimeCellHandle,
    target: &EvalReferenceTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_curl_multi_info_read_result(multi_handle, Some(target.clone()), true, context, values)
}

/// Pops one completion message and builds PHP's answer array (or `false` when the queue is
/// empty).
fn eval_curl_multi_info_read_result(
    multi_handle: RuntimeCellHandle,
    target: Option<EvalReferenceTarget>,
    queued_supplied: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_curl_multi_raw("curl_multi_info_read", multi_handle, context, values)?;
    if ffi::multi_info_read(raw, ffi::INFO_FIELD_ADVANCE) != 1 {
        // The queue is empty: php-src returns before assigning `$queued_messages`, so the
        // caller's variable keeps whatever it held — including no warning for a by-value
        // argument, since nothing would have been written to it anyway.
        return values.bool_value(false);
    }
    let queued = ffi::multi_info_read(raw, ffi::INFO_FIELD_QUEUED);
    let msg = ffi::multi_info_read(raw, ffi::INFO_FIELD_MSG);
    let result = ffi::multi_info_read(raw, ffi::INFO_FIELD_RESULT);
    let easy_raw = ffi::multi_info_read(raw, ffi::INFO_FIELD_EASY_ID);
    match &target {
        Some(target) => {
            let queued = values.int(queued)?;
            eval_write_direct_ref_target(
                target,
                queued,
                context,
                values,
                Some(ScopeCellOwnership::Owned),
            )?;
        }
        None => {
            if queued_supplied {
                values.warning(
                    "curl_multi_info_read(): Argument #2 ($queued_messages) must be passed by \
                     reference, value given",
                )?;
            }
        }
    }
    let easy_id = context.stream_resources().curl_easy_id_for_raw(easy_raw);
    let mut array = values.assoc_new(3)?;
    let msg_key = values.string("msg")?;
    let msg = values.int(msg)?;
    array = values.array_set(array, msg_key, msg)?;
    let result_key = values.string("result")?;
    let result = values.int(result)?;
    array = values.array_set(array, result_key, result)?;
    if let Some(easy_id) = easy_id {
        let handle_key = values.string("handle")?;
        let handle = values.curl_handle(easy_id)?;
        array = values.array_set(array, handle_key, handle)?;
    }
    Ok(array)
}
