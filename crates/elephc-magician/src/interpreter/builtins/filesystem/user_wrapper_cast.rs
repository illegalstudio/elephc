//! Purpose:
//! Dispatches eval userspace stream wrappers to `stream_cast()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::stream_select`.
//!
//! Key details:
//! - Magician keeps `stream_select()` conservative and returns no ready streams,
//!   but wrapper `stream_cast(STREAM_CAST_FOR_SELECT)` is still PHP-observable.

use super::super::super::*;

/// PHP's `STREAM_CAST_FOR_SELECT` value passed to wrapper `stream_cast()`.
pub(in crate::interpreter) const EVAL_STREAM_CAST_FOR_SELECT: i64 = 3;

/// Invokes `stream_cast($cast_as)` for a userspace-wrapper stream resource.
pub(in crate::interpreter) fn eval_user_wrapper_stream_cast_result(
    id: i64,
    cast_as: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(info) = context.stream_resources().user_wrapper_stream_info(id) else {
        return Ok(None);
    };
    let Some((declaring_class, stream_cast)) =
        eval_user_wrapper_method(&info.class_name, "stream_cast", context)
    else {
        return Ok(None);
    };
    let cast_as = values.int(cast_as)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &info.class_name,
        &stream_cast,
        info.object,
        positional_args(vec![cast_as]),
        context,
        values,
    )?;
    Ok(Some(result))
}
