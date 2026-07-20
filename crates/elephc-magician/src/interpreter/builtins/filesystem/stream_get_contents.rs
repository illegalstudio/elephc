//! Purpose:
//! Declarative eval registry entry for `stream_get_contents`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the bounded stream read helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_get_contents",
    area: Filesystem,
    params: [
        stream,
        length = EvalBuiltinDefaultValue::Null,
        offset = EvalBuiltinDefaultValue::Int(-1)
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `stream_get_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_get_contents_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_get_contents(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_get_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_get_contents_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_stream_get_contents_result(*stream, None, None, context, values),
        [stream, length] => eval_stream_get_contents_result(*stream, Some(*length), None, context, values),
        [stream, length, offset] => eval_stream_get_contents_result(*stream, Some(*length), Some(*offset), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `stream_get_contents($stream, $length = null, $offset = -1)`.
pub(in crate::interpreter) fn eval_builtin_stream_get_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let length = match args.get(1) {
        Some(length) => Some(eval_expr(length, context, scope, values)?),
        None => None,
    };
    let offset = match args.get(2) {
        Some(offset) => Some(eval_expr(offset, context, scope, values)?),
        None => None,
    };
    eval_stream_get_contents_result(stream, length, offset, context, values)
}

/// Reads the remaining or bounded contents from a materialized stream resource.
pub(in crate::interpreter) fn eval_stream_get_contents_result(
    stream: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    offset: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let length = eval_optional_stream_length(length, values)?;
    let offset = eval_optional_stream_offset(offset, values)?;
    if let Some(result) =
        eval_user_wrapper_stream_get_contents_result(id, length, offset, context, values)?
    {
        return Ok(result);
    }
    match context
        .stream_resources_mut()
        .get_contents(id, length, offset)
    {
        Some(bytes) => values.string_bytes_value(&bytes),
        None => values.bool_value(false),
    }
}
