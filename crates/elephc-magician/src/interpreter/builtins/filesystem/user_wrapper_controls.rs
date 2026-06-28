//! Purpose:
//! Dispatches control-style userspace stream wrapper methods for eval streams.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::streams` for flush, tell, and
//!   truncate builtins on userspace wrapper resources.
//!
//! Key details:
//! - File-backed resources keep using `EvalStreamResources`; these helpers only
//!   intercept resources created by `stream_wrapper_register()` + `fopen()`.

use super::super::super::*;
use super::user_wrapper_streams::eval_user_wrapper_method;

/// Dispatches `fflush()` to a wrapper object's `stream_flush()`.
pub(in crate::interpreter) fn eval_user_wrapper_fflush_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if context.stream_resources().user_wrapper_stream_info(id).is_none() {
        return Ok(None);
    }
    let Some(result) = eval_user_wrapper_call_no_arg_method(id, "stream_flush", context, values)?
    else {
        return values.bool_value(false).map(Some);
    };
    let ok = values.truthy(result)?;
    values.release(result)?;
    values.bool_value(ok).map(Some)
}

/// Dispatches `ftell()` to a wrapper object's `stream_tell()`.
pub(in crate::interpreter) fn eval_user_wrapper_ftell_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if context.stream_resources().user_wrapper_stream_info(id).is_none() {
        return Ok(None);
    }
    let Some(result) = eval_user_wrapper_call_no_arg_method(id, "stream_tell", context, values)?
    else {
        return values.bool_value(false).map(Some);
    };
    if values.type_tag(result)? == EVAL_TAG_BOOL && !values.truthy(result)? {
        values.release(result)?;
        return values.bool_value(false).map(Some);
    }
    let position = eval_int_value(result, values)?;
    values.release(result)?;
    values.int(position).map(Some)
}

/// Dispatches `ftruncate()` to a wrapper object's `stream_truncate()`.
pub(in crate::interpreter) fn eval_user_wrapper_ftruncate_result(
    id: i64,
    size: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, stream_truncate)) =
        eval_user_wrapper_method(&info.class_name, "stream_truncate", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let size = values.int(i64::try_from(size).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &stream_truncate,
        info.object,
        positional_args(vec![size]),
        context,
        values,
    )?;
    let ok = values.truthy(result)?;
    values.release(result)?;
    values.bool_value(ok).map(Some)
}

/// Calls one no-argument userspace wrapper method on a stream resource.
fn eval_user_wrapper_call_no_arg_method(
    id: i64,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, method)) =
        eval_user_wrapper_method(&info.class_name, method_name, context)
    else {
        return Ok(None);
    };
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &method,
        info.object,
        Vec::new(),
        context,
        values,
    )?;
    Ok(Some(result))
}
