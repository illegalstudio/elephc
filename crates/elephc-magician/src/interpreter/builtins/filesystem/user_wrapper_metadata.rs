//! Purpose:
//! Dispatches path-based metadata changes to eval userspace stream wrappers.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::touch`, `chmod`, and `chown`
//!   builtin owners when the path scheme is a registered wrapper.
//!
//! Key details:
//! - Mirrors the AOT stream-wrapper metadata options used by filesystem
//!   builtins; non-wrapper paths return `None` so local host operations continue.

use super::super::super::*;
use super::user_wrapper_streams::eval_user_wrapper_method;

pub(in crate::interpreter) const EVAL_STREAM_META_TOUCH: i64 = 1;
pub(in crate::interpreter) const EVAL_STREAM_META_OWNER_NAME: i64 = 2;
pub(in crate::interpreter) const EVAL_STREAM_META_OWNER: i64 = 3;
pub(in crate::interpreter) const EVAL_STREAM_META_GROUP_NAME: i64 = 4;
pub(in crate::interpreter) const EVAL_STREAM_META_GROUP: i64 = 5;
pub(in crate::interpreter) const EVAL_STREAM_META_ACCESS: i64 = 6;

/// Dispatches one `stream_metadata($path, $option, $value)` wrapper call.
pub(in crate::interpreter) fn eval_user_wrapper_stream_metadata_result(
    path: &str,
    option: i64,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class_name) = context
        .stream_resources()
        .user_stream_wrapper_class_for_path(path)
    else {
        return Ok(None);
    };
    let Some(class) = context.class(&class_name).cloned() else {
        return values.bool_value(false).map(Some);
    };
    let Some((declaring_class, stream_metadata)) =
        eval_user_wrapper_method(class.name(), "stream_metadata", context)
    else {
        return values.bool_value(false).map(Some);
    };
    let mut scope = ElephcEvalScope::new();
    let object = eval_dynamic_class_new_object(&class, Vec::new(), context, &mut scope, values)?;
    let path = values.string(path)?;
    let option = values.int(option)?;
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        class.name(),
        &stream_metadata,
        object,
        positional_args(vec![path, option, value]),
        context,
        values,
    )?;
    values.release(object)?;
    let ok = values.truthy(result)?;
    values.release(result)?;
    values.bool_value(ok).map(Some)
}
