//! Purpose:
//! Dispatches stream option builtins to eval userspace stream wrappers.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::stream_settings` for
//!   `stream_set_blocking()` and `stream_set_timeout()`.
//!
//! Key details:
//! - Mirrors the generated runtime's `stream_set_option($option, $arg1, $arg2)`
//!   dispatch for synthetic wrapper descriptors.

use super::super::super::*;
use super::user_wrapper_streams::eval_user_wrapper_method;

pub(in crate::interpreter) const EVAL_STREAM_OPTION_BLOCKING: i64 = 1;
pub(in crate::interpreter) const EVAL_STREAM_OPTION_READ_TIMEOUT: i64 = 4;

/// Dispatches a stream option update to a wrapper object's `stream_set_option()`.
pub(in crate::interpreter) fn eval_user_wrapper_stream_set_option_result(
    id: i64,
    option: i64,
    arg1: i64,
    arg2: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, stream_set_option)) =
        eval_user_wrapper_method(&info.class_name, "stream_set_option", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let option = values.int(option)?;
    let arg1 = values.int(arg1)?;
    let arg2 = values.int(arg2)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &stream_set_option,
        info.object,
        positional_args(vec![option, arg1, arg2]),
        context,
        values,
    )?;
    let ok = values.truthy(result)?;
    values.release(result)?;
    values.bool_value(ok).map(Some)
}
