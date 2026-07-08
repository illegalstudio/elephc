//! Purpose:
//! Declarative eval registry entry for `stream_get_line`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the delimiter-aware stream line helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_get_line",
    area: Filesystem,
    params: [stream, length, ending = EvalBuiltinDefaultValue::String("")],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `stream_get_line` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_get_line_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_get_line(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_get_line` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_get_line_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, length] => eval_stream_get_line_result(*stream, *length, None, context, values),
        [stream, length, ending] => eval_stream_get_line_result(*stream, *length, Some(*ending), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `stream_get_line($stream, $length, $ending = null)`.
pub(in crate::interpreter) fn eval_builtin_stream_get_line(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let length = eval_expr(&args[1], context, scope, values)?;
    let ending = match args.get(2) {
        Some(ending) => Some(eval_expr(ending, context, scope, values)?),
        None => None,
    };
    eval_stream_get_line_result(stream, length, ending, context, values)
}

/// Reads one line-like byte sequence from a materialized stream resource.
pub(in crate::interpreter) fn eval_stream_get_line_result(
    stream: RuntimeCellHandle,
    length: RuntimeCellHandle,
    ending: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let length = eval_nonnegative_usize(length, values)?;
    let ending = match ending {
        Some(ending) if values.type_tag(ending)? != EVAL_TAG_NULL => {
            Some(values.string_bytes(ending)?)
        }
        _ => None,
    };
    if let Some(result) =
        eval_user_wrapper_stream_get_line_result(id, length, ending.as_deref(), context, values)?
    {
        return Ok(result);
    }
    match context
        .stream_resources_mut()
        .read_line(id, length, ending.as_deref(), false, false)
    {
        Some(bytes) if !bytes.is_empty() => values.string_bytes_value(&bytes),
        Some(_) => values.bool_value(false),
        None => values.bool_value(false),
    }
}
